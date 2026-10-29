use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error, info, warn};
use y_reactor::SessionReactor;
use y_types::{SessionId, TurnSummary};

pub const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\y_ipc";
#[allow(dead_code)]
pub const UNIX_SOCKET_PATH: &str = "/tmp/y_ipc.sock";
/// Largest accepted IPC frame. A 4-byte length prefix precedes every payload.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Maximum concurrent IPC clients.
const MAX_CONNECTIONS: usize = 64;
/// A client that connects and then sends nothing is dropped after this long.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on a single turn, independent of client patience.
const TURN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Serialize, Deserialize)]
pub enum IpcRequest {
    Ping,
    ExecuteTurn {
        session_id: SessionId,
        input: String,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum IpcResponse {
    Pong,
    TurnResult(TurnSummary),
    Error(String),
    ShutdownAck,
}

/// Reads one length-prefixed frame. Sockets and pipes are byte streams, so a
/// single `read()` cannot be assumed to yield a whole message.
async fn read_frame<R: AsyncReadExt + Unpin>(stream: &mut R) -> std::io::Result<Vec<u8>> {
    let mut length_bytes = [0u8; 4];
    stream.read_exact(&mut length_bytes).await?;
    let length = u32::from_le_bytes(length_bytes) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame of {length} bytes exceeds the {MAX_FRAME_BYTES} byte limit"),
        ));
    }
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

async fn write_frame<W: AsyncWriteExt + Unpin>(
    stream: &mut W,
    payload: &[u8],
) -> std::io::Result<()> {
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(payload).await?;
    stream.flush().await
}

async fn serve(
    stream: &mut (impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin),
    reactor: Arc<tokio::sync::Mutex<SessionReactor>>,
) -> std::io::Result<()> {
    let frame = tokio::time::timeout(READ_TIMEOUT, read_frame(stream))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "read timed out"))??;
    if frame.is_empty() {
        return Ok(());
    }

    let response = match serde_json::from_slice::<IpcRequest>(&frame) {
        Ok(IpcRequest::Ping) => IpcResponse::Pong,
        Ok(IpcRequest::ExecuteTurn { input, .. }) => {
            // Bound the turn independently of the client so a slow consumer
            // cannot hold the reactor lock indefinitely.
            let outcome = {
                let mut guard = reactor.lock().await;
                tokio::time::timeout(TURN_TIMEOUT, guard.execute_turn(input)).await
            };
            match outcome {
                Ok(Ok(summary)) => IpcResponse::TurnResult(summary),
                Ok(Err(e)) => IpcResponse::Error(e.to_string()),
                Err(_) => IpcResponse::Error(format!(
                    "Turn exceeded {}s and was abandoned",
                    TURN_TIMEOUT.as_secs()
                )),
            }
        }
        Ok(IpcRequest::Shutdown) => IpcResponse::ShutdownAck,
        Err(e) => IpcResponse::Error(format!("Invalid IPC JSON: {e}")),
    };

    let bytes = serde_json::to_vec(&response)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Err(e) = write_frame(stream, &bytes).await {
        // A client that disconnected before reading its answer is not an error
        // worth escalating, but it must not be silent either.
        warn!("IPC response write failed: {e}");
    }
    Ok(())
}

#[cfg(windows)]
pub async fn run_windows_ipc_server(
    reactor: Arc<tokio::sync::Mutex<SessionReactor>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ServerOptions;

    info!(
        "Starting Windows Named Pipe server at {}",
        WINDOWS_PIPE_NAME
    );

    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    // Enforce FILE_FLAG_FIRST_PIPE_INSTANCE on the first server instance
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(WINDOWS_PIPE_NAME)?;

    loop {
        tokio::time::timeout(CONNECT_TIMEOUT, server.connect())
            .await
            .map_err(|_| "timed out waiting for a pipe client".to_string())??;
        let mut connected_client = server;

        // Create the next pipe instance for subsequent connections
        server = ServerOptions::new()
            .reject_remote_clients(true)
            .create(WINDOWS_PIPE_NAME)?;

        let Ok(permit) = permits.clone().try_acquire_owned() else {
            error!("IPC connection limit of {MAX_CONNECTIONS} reached; rejecting client");
            continue;
        };
        let reactor_clone = reactor.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = serve(&mut connected_client, reactor_clone).await {
                debug!("IPC client error: {e}");
            }
        });
    }
}

#[cfg(not(windows))]
pub async fn run_unix_ipc_server(
    reactor: Arc<tokio::sync::Mutex<SessionReactor>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    // Never unlink a path that is not a socket: `/tmp` is world-writable, so a
    // pre-planted symlink or file here would otherwise be destroyed.
    if let Ok(metadata) = std::fs::symlink_metadata(UNIX_SOCKET_PATH) {
        if !metadata.file_type().is_socket() {
            return Err(format!(
                "refusing to remove a non-socket at the IPC path {}",
                UNIX_SOCKET_PATH
            )
            .into());
        }
        std::fs::remove_file(UNIX_SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(UNIX_SOCKET_PATH)?;
    // Owner-only: without this any local user can drive the reactor.
    std::fs::set_permissions(UNIX_SOCKET_PATH, std::fs::Permissions::from_mode(0o600))?;
    info!("Starting Unix Domain Socket server at {}", UNIX_SOCKET_PATH);

    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    loop {
        let (mut stream, _) = listener.accept().await?;
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            error!("IPC connection limit of {MAX_CONNECTIONS} reached; rejecting client");
            continue;
        };
        let reactor_clone = reactor.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = serve(&mut stream, reactor_clone).await {
                debug!("IPC client error: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn round_trip(request: IpcRequest) -> IpcResponse {
        let db_path = std::env::temp_dir().join(format!("ipc-{}.db", y_types::TurnId::new()));
        let arena = Arc::new(y_arena::WorkingArena::with_defaults());
        let storage = Arc::new(y_storage::StorageEngine::with_defaults(&db_path).unwrap());
        let reactor = Arc::new(tokio::sync::Mutex::new(y_reactor::SessionReactor::new(
            SessionId::new(),
            arena,
            storage.clone(),
        )));

        let (mut client, mut server) = tokio::io::duplex(64 * 1024);
        let payload = serde_json::to_vec(&request).unwrap();
        // Write the frame in two pieces to prove the reader reassembles it.
        let client_task = tokio::spawn(async move {
            client
                .write_all(&(payload.len() as u32).to_le_bytes())
                .await
                .unwrap();
            client.write_all(&payload).await.unwrap();
            client.flush().await.unwrap();
            read_frame(&mut client).await.unwrap()
        });

        serve(&mut server, reactor).await.unwrap();
        let response = serde_json::from_slice(&client_task.await.unwrap()).unwrap();
        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
        response
    }

    #[tokio::test]
    async fn frame_is_reassembled_across_write_boundaries() {
        let (mut writer, mut reader) = tokio::io::duplex(16 * 1024);
        let payload = b"a".repeat(5000);
        let expected = payload.clone();
        let writer_task = tokio::spawn(async move {
            writer
                .write_all(&(payload.len() as u32).to_le_bytes())
                .await
                .unwrap();
            for chunk in payload.chunks(7) {
                writer.write_all(chunk).await.unwrap();
            }
            writer.flush().await.unwrap();
        });
        let read = read_frame(&mut reader).await.unwrap();
        writer_task.await.unwrap();
        assert_eq!(read, expected);
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected() {
        let (mut writer, mut reader) = tokio::io::duplex(16);
        let too_big = (MAX_FRAME_BYTES + 1) as u32;
        writer.write_all(&too_big.to_le_bytes()).await.unwrap();
        writer.flush().await.unwrap();
        assert!(read_frame(&mut reader).await.is_err());
    }

    #[tokio::test]
    async fn ping_round_trips() {
        assert!(matches!(
            round_trip(IpcRequest::Ping).await,
            IpcResponse::Pong
        ));
    }

    #[tokio::test]
    async fn execute_turn_round_trips() {
        let response = round_trip(IpcRequest::ExecuteTurn {
            session_id: SessionId::new(),
            input: "hello over ipc".into(),
        })
        .await;
        match response {
            IpcResponse::TurnResult(summary) => assert!(summary.prompt_tokens > 0),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_frame_yields_an_error_response() {
        let (mut client, mut server) = tokio::io::duplex(64 * 1024);
        let db_path = std::env::temp_dir().join(format!("ipc-bad-{}.db", y_types::TurnId::new()));
        let arena = Arc::new(y_arena::WorkingArena::with_defaults());
        let storage = Arc::new(y_storage::StorageEngine::with_defaults(&db_path).unwrap());
        let reactor = Arc::new(tokio::sync::Mutex::new(y_reactor::SessionReactor::new(
            SessionId::new(),
            arena,
            storage.clone(),
        )));

        let junk = b"{not json";
        let reader_task = tokio::spawn(async move {
            client
                .write_all(&(junk.len() as u32).to_le_bytes())
                .await
                .unwrap();
            client.write_all(junk).await.unwrap();
            client.flush().await.unwrap();
            read_frame(&mut client).await.unwrap()
        });
        serve(&mut server, reactor).await.unwrap();
        let response: IpcResponse = serde_json::from_slice(&reader_task.await.unwrap()).unwrap();
        assert!(matches!(response, IpcResponse::Error(_)));
        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
    }
}
