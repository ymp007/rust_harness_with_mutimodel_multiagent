use std::time::Duration;
use y_arena::{BumpArena, HysteresisPurger, WorkingArena};

#[test]
fn test_bump_arena_allocation_and_reset() {
    let arena = BumpArena::new(1024);
    assert_eq!(arena.allocated_bytes(), 0);

    let slice1 = arena.alloc_raw(64).expect("Allocation failed");
    assert_eq!(slice1.len(), 64);
    assert!(arena.allocated_bytes() >= 64);

    let slice2 = arena.alloc_raw(128).expect("Allocation failed");
    assert_eq!(slice2.len(), 128);

    arena.reset();
    assert_eq!(arena.allocated_bytes(), 0);

    // Can allocate again after reset
    let slice3 = arena.alloc_raw(256).expect("Allocation after reset failed");
    assert_eq!(slice3.len(), 256);
}

#[test]
fn test_working_arena_defaults() {
    let arena = WorkingArena::with_defaults();
    assert_eq!(arena.capacity(), 16 * 1024 * 1024);

    let mem = arena.alloc(1024).expect("Allocation failed");
    assert_eq!(mem.len(), 1024);

    arena.reset_turn();
    assert_eq!(arena.allocated_bytes(), 0);
}

#[test]
fn test_hysteresis_purger_threshold() {
    let purger = HysteresisPurger::new(Duration::from_millis(50));
    assert!(!purger.should_purge());

    std::thread::sleep(Duration::from_millis(60));
    assert!(purger.should_purge());

    let purged = purger.try_purge();
    assert!(purged);

    // Second purge should return false (already purged)
    assert!(!purger.try_purge());

    // Mark active resets state
    purger.mark_active();
    assert!(!purger.should_purge());
}
