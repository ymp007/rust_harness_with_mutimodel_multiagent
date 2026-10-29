use std::sync::atomic::{AtomicUsize, Ordering};
use y_types::AgentError;

/// 128-byte cache-line aligned SPSC ring header.
/// Padding keeps the producer and consumer cursors on separate cache lines.
#[repr(align(128))]
pub struct ShmHeader {
    pub head: AtomicUsize,
    _pad1: [u8; 120],
    pub tail: AtomicUsize,
    _pad2: [u8; 120],
    pub capacity: usize,
    pub mask: usize,
    _pad3: [u8; 112],
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

/// In-process framed byte ring with exclusive access.
///
/// This buffer is heap memory, not an OS shared-memory mapping, and `&mut self`
/// on both accessors means a single producer/consumer split is not
/// representable. The Acquire/Release fences below are correct for that
/// single-threaded use; they are not sufficient for cross-process SPSC, which
/// would require cursors over a shared mapping.
pub struct ShmRingBuffer {
    header: Box<ShmHeader>,
    buffer: Box<[u8]>,
}

const FRAME_HEADER_BYTES: usize = 8;

impl ShmRingBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity >= 8 && capacity.is_power_of_two(),
            "Capacity must be a power of two >= 8"
        );
        let header = Box::new(ShmHeader::new(capacity));
        let buffer = vec![0u8; capacity].into_boxed_slice();

        Self { header, buffer }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(1024 * 1024) // 1 MB ring buffer
    }

    /// Producer: enqueues a length-prefixed frame.
    pub fn enqueue(&mut self, payload: &[u8]) -> Result<(), AgentError> {
        let len = payload.len();
        let frame_len = len
            .checked_add(FRAME_HEADER_BYTES)
            .ok_or_else(|| AgentError::InternalError("Frame length overflow".into()))?;
        if frame_len > self.header.capacity {
            return Err(AgentError::ResourceExhausted(format!(
                "Frame of {len} bytes exceeds ring capacity {}",
                self.header.capacity
            )));
        }

        let head = self.header.head.load(Ordering::Relaxed);
        let tail = self.header.tail.load(Ordering::Acquire);
        let used = head.wrapping_sub(tail);
        if used > self.header.capacity {
            return Err(AgentError::SecurityViolation("Invalid ring cursors".into()));
        }
        let available = self.header.capacity - used;

        if available < frame_len {
            return Err(AgentError::ResourceExhausted(
                "SHM Ring buffer full".to_string(),
            ));
        }

        let mask = self.header.mask;

        // 1. Length prefix
        let len_bytes = (len as u64).to_le_bytes();
        for (i, byte) in len_bytes.iter().enumerate() {
            self.buffer[head.wrapping_add(i) & mask] = *byte;
        }

        // 2. Payload
        for (i, byte) in payload.iter().enumerate() {
            self.buffer[head.wrapping_add(FRAME_HEADER_BYTES).wrapping_add(i) & mask] = *byte;
        }

        // 3. Publish
        self.header
            .head
            .store(head.wrapping_add(frame_len), Ordering::Release);
        Ok(())
    }

    /// Consumer: copies one complete frame into the caller's buffer, then
    /// advances `tail`.
    ///
    /// Returns `ResourceExhausted` without advancing `tail` when the frame does
    /// not fit the target buffer, so the caller can grow its buffer and retry.
    /// A frame whose declared length exceeds the ring's remaining bytes is
    /// unrecoverable and is skipped, because re-reading it forever would wedge
    /// the ring permanently.
    pub fn dequeue_into_private(
        &mut self,
        target_buf: &mut [u8],
    ) -> Result<Option<usize>, AgentError> {
        let tail = self.header.tail.load(Ordering::Relaxed);
        let head = self.header.head.load(Ordering::Acquire);

        if head == tail {
            return Ok(None);
        }

        let mask = self.header.mask;
        let available = head.wrapping_sub(tail);
        if available < FRAME_HEADER_BYTES || available > self.header.capacity {
            // No valid frame boundary exists; resynchronize on the producer.
            self.header.tail.store(head, Ordering::Release);
            return Err(AgentError::SecurityViolation(
                "Invalid ring frame boundary; resynchronized".into(),
            ));
        }

        let mut len_bytes = [0u8; FRAME_HEADER_BYTES];
        for (i, byte) in len_bytes.iter_mut().enumerate() {
            *byte = self.buffer[tail.wrapping_add(i) & mask];
        }
        let len = usize::try_from(u64::from_le_bytes(len_bytes))
            .map_err(|_| AgentError::SecurityViolation("Frame length overflow".into()))?;

        let payload_available = available - FRAME_HEADER_BYTES;
        if len > payload_available {
            // The declared length cannot fit in the ring, so this frame is
            // corrupt. Advancing past it keeps the ring draining.
            self.header.tail.store(head, Ordering::Release);
            return Err(AgentError::SecurityViolation(format!(
                "Corrupt frame declares {len} bytes but only {payload_available} are buffered; \
                 frame skipped"
            )));
        }

        if len > target_buf.len() {
            // Retryable: leave `tail` untouched.
            return Err(AgentError::ResourceExhausted(format!(
                "Frame needs {len} bytes but the read buffer holds {}; \
                 grow the buffer and retry",
                target_buf.len()
            )));
        }

        for (i, byte) in target_buf.iter_mut().take(len).enumerate() {
            *byte = self.buffer[tail.wrapping_add(FRAME_HEADER_BYTES).wrapping_add(i) & mask];
        }

        self.header.tail.store(
            tail.wrapping_add(FRAME_HEADER_BYTES).wrapping_add(len),
            Ordering::Release,
        );
        Ok(Some(len))
    }

    /// Largest frame this ring can ever hold.
    pub fn max_frame_len(&self) -> usize {
        self.header.capacity - FRAME_HEADER_BYTES
    }

    pub fn is_empty(&self) -> bool {
        self.header.head.load(Ordering::Acquire) == self.header.tail.load(Ordering::Acquire)
    }

    pub fn capacity(&self) -> usize {
        self.header.capacity
    }

    /// Bytes currently occupied by queued frames.
    pub fn used_bytes(&self) -> usize {
        let head = self.header.head.load(Ordering::Acquire);
        let tail = self.header.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail).min(self.header.capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_too_large_for_the_buffer_is_retryable() {
        let mut ring = ShmRingBuffer::new(1024);
        ring.enqueue(&vec![b'x'; 512]).unwrap();
        let mut small = [0u8; 64];

        for _ in 0..5 {
            let err = ring.dequeue_into_private(&mut small).unwrap_err();
            assert!(
                matches!(err, AgentError::ResourceExhausted(_)),
                "expected a retryable error, got {err:?}"
            );
        }

        // Growing the buffer must let the same frame through, and the ring
        // must still accept new frames afterwards.
        let mut large = vec![0u8; 512];
        assert_eq!(ring.dequeue_into_private(&mut large).unwrap(), Some(512));
        assert!(large.iter().all(|b| *b == b'x'));
        ring.enqueue(b"after").unwrap();
        assert!(!ring.is_empty());
    }

    #[test]
    fn corrupt_frame_is_skipped_instead_of_wedging_the_ring() {
        let mut ring = ShmRingBuffer::new(1024);
        ring.enqueue(b"good").unwrap();
        // Corrupt the length prefix of the queued frame.
        let bogus = u64::MAX.to_le_bytes();
        for (i, byte) in bogus.iter().enumerate() {
            ring.buffer[i] = *byte;
        }

        let mut out = [0u8; 64];
        assert!(ring.dequeue_into_private(&mut out).is_err());
        // Ring drains and accepts traffic again.
        for _ in 0..64 {
            ring.enqueue(b"payload").unwrap();
        }
        assert!(ring.used_bytes() <= ring.capacity());
    }

    #[test]
    fn roundtrips_across_frame_boundaries() {
        let mut ring = ShmRingBuffer::new(1024);
        for payload in [b"a".as_slice(), b"bb", b"ccc", b"", b"dddd"] {
            ring.enqueue(payload).unwrap();
        }
        let mut out = vec![0u8; 64];
        let mut seen = Vec::new();
        while let Some(n) = ring.dequeue_into_private(&mut out).unwrap() {
            seen.push(out[..n].to_vec());
        }
        assert_eq!(
            seen,
            vec![
                b"a".to_vec(),
                b"bb".to_vec(),
                b"ccc".to_vec(),
                Vec::new(),
                b"dddd".to_vec()
            ]
        );
    }

    #[test]
    fn frame_larger_than_capacity_is_rejected() {
        let mut ring = ShmRingBuffer::new(64);
        let payload = vec![0u8; ring.max_frame_len() + 1];
        assert!(matches!(
            ring.enqueue(&payload),
            Err(AgentError::ResourceExhausted(_))
        ));
    }
}
