use std::sync::atomic::{AtomicUsize, Ordering};
use y_types::AgentError;

/// 128-byte cache-line aligned SPSC Shared-Memory Circular Ring Buffer Header.
/// Prevents false sharing across core caches on modern x86_64 and Apple Silicon.
#[repr(align(128))]
pub struct ShmHeader {
    pub head: AtomicUsize,
    _pad1: [u8; 120], // Pad to 128 bytes
    pub tail: AtomicUsize,
    _pad2: [u8; 120], // Pad to 128 bytes
    pub capacity: usize,
    pub mask: usize,
    _pad3: [u8; 112], // Pad to 128 bytes
}

impl ShmHeader {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be power of two");
        Self {
            head: AtomicUsize::new(0),
            _pad1: [0u8; 120],
            tail: AtomicUsize::new(0),
            _pad2: [0u8; 120],
            capacity,
            mask: capacity - 1,
            _pad3: [0u8; 112],
        }
    }
}

/// SPSC Lockless Shared Memory Ring Buffer with Single-Copy Ingestion Rule.
pub struct ShmRingBuffer {
    header: Box<ShmHeader>,
    buffer: Box<[u8]>,
}

unsafe impl Send for ShmRingBuffer {}
unsafe impl Sync for ShmRingBuffer {}

impl ShmRingBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be power of two");
        let header = Box::new(ShmHeader::new(capacity));
        let buffer = vec![0u8; capacity].into_boxed_slice();

        Self { header, buffer }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(1024 * 1024) // 1 MB ring buffer
    }

    /// Producer: Enqueues a byte payload into the ring buffer.
    pub fn enqueue(&mut self, payload: &[u8]) -> Result<(), AgentError> {
        let len = payload.len();
        if len + 8 > self.header.capacity {
            return Err(AgentError::InternalError("Payload exceeds ring capacity".to_string()));
        }

        let head = self.header.head.load(Ordering::Acquire);
        let tail = self.header.tail.load(Ordering::Acquire);
        let available = self.header.capacity - (head - tail);

        if available < len + 8 {
            return Err(AgentError::InternalError("SHM Ring buffer full".to_string()));
        }

        let mask = self.header.mask;
        let buf_ptr = self.buffer.as_mut_ptr();

        // 1. Write 8-byte length prefix
        let len_bytes = (len as u64).to_le_bytes();
        for i in 0..8 {
            let idx = (head + i) & mask;
            unsafe {
                *buf_ptr.add(idx) = len_bytes[i];
            }
        }

        // 2. Write payload bytes
        for i in 0..len {
            let idx = (head + 8 + i) & mask;
            unsafe {
                *buf_ptr.add(idx) = payload[i];
            }
        }

        // 3. Commit head with Release ordering
        self.header.head.store(head + 8 + len, Ordering::Release);
        Ok(())
    }

    /// Consumer: Implements the "Single-Copy Ingestion Rule" for Untrusted Shared Memory.
    /// Atomically copies payload bytes into a private target buffer BEFORE any inspection,
    /// completely eliminating the Double-Fetch TOCTOU vulnerability.
    pub fn dequeue_into_private(&self, target_buf: &mut [u8]) -> Result<Option<usize>, AgentError> {
        let tail = self.header.tail.load(Ordering::Acquire);
        let head = self.header.head.load(Ordering::Acquire);

        if head == tail {
            return Ok(None); // Buffer empty
        }

        let mask = self.header.mask;
        let buf_ptr = self.buffer.as_ptr();

        // 1. Read 8-byte length prefix
        let mut len_bytes = [0u8; 8];
        for i in 0..8 {
            let idx = (tail + i) & mask;
            unsafe {
                len_bytes[i] = *buf_ptr.add(idx);
            }
        }
        let len = u64::from_le_bytes(len_bytes) as usize;

        if len > target_buf.len() {
            return Err(AgentError::SecurityViolation(format!(
                "Payload size {} exceeds private target buffer capacity {}",
                len,
                target_buf.len()
            )));
        }

        // 2. Atomic Single-Copy snapshot into private non-shared memory
        for i in 0..len {
            let idx = (tail + 8 + i) & mask;
            unsafe {
                target_buf[i] = *buf_ptr.add(idx);
            }
        }

        // 3. Update tail pointer with Release ordering
        self.header.tail.store(tail + 8 + len, Ordering::Release);
        Ok(Some(len))
    }

    pub fn is_empty(&self) -> bool {
        self.header.head.load(Ordering::Acquire) == self.header.tail.load(Ordering::Acquire)
    }

    pub fn capacity(&self) -> usize {
        self.header.capacity
    }
}
