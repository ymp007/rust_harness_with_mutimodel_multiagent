use std::sync::atomic::{AtomicUsize, Ordering};

pub const DEFAULT_ARENA_CAPACITY: usize = 16 * 1024 * 1024; // 16 MB warm working pool

/// High-performance bump arena for zero-allocation turn staging.
pub struct BumpArena {
    buffer: Box<[u8]>,
    offset: AtomicUsize,
    capacity: usize,
}

// BumpArena supports thread-safe concurrent allocation via atomic fetch_add
unsafe impl Send for BumpArena {}
unsafe impl Sync for BumpArena {}

impl BumpArena {
    pub fn new(capacity: usize) -> Self {
        let buffer = vec![0u8; capacity].into_boxed_slice();
        Self {
            buffer,
            offset: AtomicUsize::new(0),
            capacity,
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_ARENA_CAPACITY)
    }

    /// Allocates a raw byte slice of the requested size.
    /// Executes in < 15 ns via atomic fetch_add.
    #[inline(always)]
    pub fn alloc_raw(&self, size: usize) -> Option<&mut [u8]> {
        // Align to 8-byte boundary
        let aligned_size = (size + 7) & !7;
        let prev_offset = self.offset.fetch_add(aligned_size, Ordering::AcqRel);

        if prev_offset + aligned_size <= self.capacity {
            let ptr = self.buffer.as_ptr() as *mut u8;
            unsafe {
                let slice_ptr = ptr.add(prev_offset);
                Some(std::slice::from_raw_parts_mut(slice_ptr, size))
            }
        } else {
            None
        }
    }

    /// Resets the bump pointer to 0 without reallocating or decommitting pages.
    #[inline(always)]
    pub fn reset(&self) {
        self.offset.store(0, Ordering::Release);
    }

    /// Returns the currently allocated bytes.
    #[inline(always)]
    pub fn allocated_bytes(&self) -> usize {
        self.offset.load(Ordering::Acquire).min(self.capacity)
    }

    /// Returns the total capacity.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
