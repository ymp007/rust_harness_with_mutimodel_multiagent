pub const DEFAULT_ARENA_CAPACITY: usize = 16 * 1024 * 1024; // 16 MB warm working pool
/// Alignment guaranteed for the arena base and every allocation from it.
pub const ARENA_ALIGNMENT: usize = 64;

/// A bump arena over a cache-line-aligned buffer.
///
/// Allocation is deliberately exclusive: `alloc_raw` takes `&mut self` and
/// adjusts the cursor in place, so it is correct for a single owner and makes no
/// claim to be safe for concurrent callers.
pub struct BumpArena {
    /// Over-allocated so the aligned window below always fits inside the Vec.
    storage: Vec<u8>,
    /// Index of the first `ARENA_ALIGNMENT`-aligned byte.
    base: usize,
    offset: usize,
    capacity: usize,
}

impl BumpArena {
    pub fn new(capacity: usize) -> Self {
        // Vec<u8> only guarantees alignment 1, so reserve extra bytes and pick
        // the first 64-aligned address inside it.
        let storage = vec![0u8; capacity.saturating_add(ARENA_ALIGNMENT)];
        let addr = storage.as_ptr() as usize;
        let base = addr.next_multiple_of(ARENA_ALIGNMENT) - addr;
        debug_assert!(base + capacity <= storage.len());
        Self {
            storage,
            base,
            offset: 0,
            capacity,
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_ARENA_CAPACITY)
    }

    /// Allocates a zeroed slice of `size` bytes, aligned to
    /// [`ARENA_ALIGNMENT`] on both offset and address.
    pub fn alloc_raw(&mut self, size: usize) -> Option<&mut [u8]> {
        // Align the offset, not the size: rounding the request up would leave
        // the next allocation starting unaligned.
        let start = self.offset.next_multiple_of(ARENA_ALIGNMENT);
        let end = start.checked_add(size)?;
        if end > self.capacity {
            return None;
        }
        let region = &mut self.storage[self.base + start..self.base + end];
        // Zero the whole region so a short write cannot expose another turn's
        // contents.
        region.fill(0);
        self.offset = end;
        Some(region)
    }

    /// Rewinds the cursor and scrubs the used region, so the next allocation
    /// cannot observe bytes from a previous turn or session.
    pub fn reset(&mut self) {
        let used = self.offset.min(self.capacity);
        self.storage[self.base..self.base + used].fill(0);
        self.offset = 0;
    }

    /// Rewinds and releases the backing buffer entirely, then re-establishes an
    /// aligned window so the arena stays usable after a purge.
    pub fn reset_and_reclaim(&mut self) {
        self.reset();
        *self = Self::new(self.capacity);
    }

    /// Returns the currently allocated bytes.
    pub fn allocated_bytes(&self) -> usize {
        self.offset.min(self.capacity)
    }

    /// Returns the total capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_are_aligned_and_distinct() {
        let mut arena = BumpArena::new(4096);
        for size in [1usize, 7, 64, 3, 100] {
            let ptr = arena.alloc_raw(size).map(|s| s.as_ptr() as usize).unwrap();
            assert_eq!(
                ptr % ARENA_ALIGNMENT,
                0,
                "allocation of {size} bytes was not {ARENA_ALIGNMENT}-aligned"
            );
        }
    }

    #[test]
    fn sequential_allocations_do_not_overlap() {
        let mut arena = BumpArena::new(4096);
        let first = arena.alloc_raw(10).unwrap().as_ptr() as usize;
        let second = arena.alloc_raw(10).unwrap().as_ptr() as usize;
        assert!(first + 10 <= second, "allocations overlapped");
    }

    #[test]
    fn reset_scrubs_previous_contents() {
        let mut arena = BumpArena::new(1024);
        arena
            .alloc_raw(14)
            .unwrap()
            .copy_from_slice(b"SECRET-CONTENT");
        arena.reset();
        assert_eq!(arena.allocated_bytes(), 0);
        let fresh = arena.alloc_raw(16).unwrap();
        assert!(
            fresh.iter().all(|b| *b == 0),
            "previous turn's bytes survived reset"
        );
    }

    #[test]
    fn reclaim_keeps_the_arena_usable() {
        let mut arena = BumpArena::new(1024);
        arena.alloc_raw(64).unwrap();
        arena.reset_and_reclaim();
        assert_eq!(arena.allocated_bytes(), 0);
        assert_eq!(arena.capacity(), 1024);
        let ptr = arena.alloc_raw(64).unwrap().as_ptr() as usize;
        assert_eq!(ptr % ARENA_ALIGNMENT, 0);
    }

    #[test]
    fn exhaustion_returns_none_without_panicking() {
        let mut arena = BumpArena::new(128);
        assert!(arena.alloc_raw(4096).is_none());
        // The cursor must be unchanged after a failed allocation.
        assert_eq!(arena.allocated_bytes(), 0);
    }
}
