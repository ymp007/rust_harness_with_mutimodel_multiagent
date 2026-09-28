use y_shm::ShmRingBuffer;

#[test]
fn test_shm_enqueue_and_single_copy_dequeue() {
    let mut ring = ShmRingBuffer::new(1024);
    assert!(ring.is_empty());

    let payload = b"Hello, Zero-Copy Y Code SHM!";
    ring.enqueue(payload).expect("Enqueue failed");
    assert!(!ring.is_empty());

    let mut private_buf = [0u8; 128];
    let len = ring
        .dequeue_into_private(&mut private_buf)
        .expect("Dequeue failed")
        .expect("Should have payload");

    assert_eq!(len, payload.len());
    assert_eq!(&private_buf[..len], payload);
    assert!(ring.is_empty());
}

#[test]
fn test_shm_wrap_around_ring() {
    let mut ring = ShmRingBuffer::new(256);
    let payload = b"Test 16-byte msg";

    for _ in 0..100 {
        ring.enqueue(payload).expect("Wrap enqueue failed");
        let mut buf = [0u8; 64];
        let len = ring.dequeue_into_private(&mut buf).unwrap().unwrap();
        assert_eq!(&buf[..len], payload);
    }
}
