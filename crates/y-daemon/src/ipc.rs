use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;
use y_reactor::SessionReactor;
use y_types::{SessionId, TurnSummary};

pub const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\y_ipc";
#[allow(dead_code)]
pub const UNIX_SOCKET_PATH: &str = "/tmp/y_ipc.sock";

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

#[cfg(windows)]
pub async fn run_windows_ipc_server(
    reactor: Arc<tokio::sync::Mutex<SessionReactor>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ServerOptions;

    info!("Starting Windows Named Pipe server at {}", WINDOWS_PIPE_NAME);

    // Enforce FILE_FLAG_FIRST_PIPE_INSTANCE on the first server instance
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(WINDOWS_PIPE_NAME)?;

    loop {
        server.connect().await?;
        let connected_client = server;

        // Create the next pipe instance for subsequent connections
        server = ServerOptions::new()
            .reject_remote_clients(true)
            .create(WINDOWS_PIPE_NAME)?;

        let reactor_clone = reactor.clone();
        tokio::spawn(async move {
            let mut client = connected_client;
            let mut buf = vec![0u8; 64 * 1024];

            match client.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let req: Result<IpcRequest, _> = serde_json::from_slice(&buf[..n]);
                    let resp = match req {
                        Ok(IpcRequest::Ping) => IpcResponse::Pong,
                        Ok(IpcRequest::ExecuteTurn { input, .. }) => {
                            let mut locked = reactor_clone.lock().await;
                            match locked.execute_turn(input).await {
                                Ok(summary) => IpcResponse::TurnResult(summary),
                                Err(e) => IpcResponse::Error(e.to_string()),
                            }
                        }
                        Ok(IpcRequest::Shutdown) => IpcResponse::ShutdownAck,
                        Err(e) => IpcResponse::Error(format!("Invalid IPC JSON: {e}")),
                    };

                    if let Ok(resp_bytes) = serde_json::to_vec(&resp) {
                        let _ = client.write_all(&resp_bytes).await;
                    }
                }
                _ => {}
            }
        });
    }
}

#[cfg(not(windows))]
pub async fn run_unix_ipc_server(
    reactor: Arc<tokio::sync::Mutex<SessionReactor>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::UnixListener;
    let _ = std::fs::remove_file(UNIX_SOCKET_PATH);
    let listener = UnixListener::bind(UNIX_SOCKET_PATH)?;
    info!("Starting Unix Domain Socket server at {}", UNIX_SOCKET_PATH);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let reactor_clone = reactor.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            if let Ok(n) = stream.read(&mut buf).await {
                if n > 0 {
                    let req: Result<IpcRequest, _> = serde_json::from_slice(&buf[..n]);
                    let resp = match req {
                        Ok(IpcRequest::Ping) => IpcResponse::Pong,
                        Ok(IpcRequest::ExecuteTurn { input, .. }) => {
                            let mut locked = reactor_clone.lock().await;
                            match locked.execute_turn(input).await {
                                Ok(summary) => IpcResponse::TurnResult(summary),
                                Err(e) => IpcResponse::Error(e.to_string()),
                            }
                        }
                        Ok(IpcRequest::Shutdown) => IpcResponse::ShutdownAck,
                        Err(e) => IpcResponse::Error(format!("Invalid IPC JSON: {e}")),
                    };

                    if let Ok(resp_bytes) = serde_json::to_vec(&resp) {
                        let _ = stream.write_all(&resp_bytes).await;
                    }
                }
            }
        });
    }
}
