use serde::{Deserialize, Serialize};
use std::env;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use y_types::{SessionId, TurnSummary};

pub const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\y_ipc";
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let prompt = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        "Test turn from Y Code CLI".to_string()
    };

    println!("⚡ Y Code Native CLI Driver");
    println!("Connecting to warm y-daemon IPC channel...");

    let start = Instant::now();

    #[cfg(windows)]
    let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(WINDOWS_PIPE_NAME)
        .map_err(|e| {
            format!(
                "Could not connect to y-daemon Named Pipe ({WINDOWS_PIPE_NAME}). Is y-daemon running? Error: {e}"
            )
        })?;

    #[cfg(not(windows))]
    let mut client = tokio::net::UnixStream::connect(UNIX_SOCKET_PATH)
        .await
        .map_err(|e| {
            format!(
                "Could not connect to y-daemon socket ({UNIX_SOCKET_PATH}). Is y-daemon running? Error: {e}"
            )
        })?;

    let req = IpcRequest::ExecuteTurn {
        session_id: SessionId::new(),
        input: prompt,
    };

    let req_bytes = serde_json::to_vec(&req)?;
    client.write_all(&req_bytes).await?;

    let mut buf = vec![0u8; 64 * 1024];
    let n = client.read(&mut buf).await?;

    let resp: IpcResponse = serde_json::from_slice(&buf[..n])?;
    let elapsed = start.elapsed();

    match resp {
        IpcResponse::TurnResult(summary) => {
            println!("✅ Turn Dispatched Successfully!");
            println!("   Turn ID:             {}", summary.turn_id);
            println!("   Prompt Tokens:       {}", summary.prompt_tokens);
            println!("   Completion Tokens:   {}", summary.completion_tokens);
            println!("   Harness Overhead:    {:.2} ms", summary.harness_overhead_ns as f64 / 1_000_000.0);
            println!("   Total CLI Roundtrip: {:.2} ms", elapsed.as_secs_f64() * 1000.0);
        }
        IpcResponse::Pong => println!("Pong!"),
        IpcResponse::Error(err) => eprintln!("❌ Daemon Error: {err}"),
        IpcResponse::ShutdownAck => println!("Daemon shutting down."),
    }

    Ok(())
}
