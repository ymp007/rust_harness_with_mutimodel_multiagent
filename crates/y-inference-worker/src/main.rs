use tracing::info;
use y_shm::ShmRingBuffer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("Initializing Y Code Out-of-Process GPU Inference Worker...");

    let mut ring = ShmRingBuffer::with_default_capacity();
    info!("Initialized isolated SHM Ring Buffer. Ready for model dispatch.");

    // Ready signal
    let ready_msg = b"INFERENCE_WORKER_READY";
    ring.enqueue(ready_msg)?;

    info!("GPU worker listening on shared memory channel.");
    Ok(())
}
