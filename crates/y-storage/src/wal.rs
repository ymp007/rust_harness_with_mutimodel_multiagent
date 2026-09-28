use crossbeam_channel::{bounded, Receiver, Sender};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info};
use y_types::{AgentError, TurnSummary};

pub const WAL_CHANNEL_CAPACITY: usize = 4096;

pub enum StorageCommand {
    CommitTurn(TurnSummary),
    Checkpoint,
    Shutdown,
}

pub struct WalWorker {
    sender: Sender<StorageCommand>,
    is_running: Arc<AtomicBool>,
}

impl WalWorker {
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }
    pub fn start(db_path: impl AsRef<Path>) -> Result<Self, AgentError> {
        let db_path = db_path.as_ref().to_path_buf();
        let (sender, receiver) = bounded(WAL_CHANNEL_CAPACITY);
        let is_running = Arc::new(AtomicBool::new(true));
        let running_flag = is_running.clone();

        std::thread::Builder::new()
            .name("y-storage-wal".to_string())
            .spawn(move || {
                if let Err(e) = Self::worker_loop(db_path, receiver, running_flag) {
                    error!("Storage WAL worker thread failed: {e}");
                }
            })
            .map_err(|e| AgentError::StorageError(format!("Failed to spawn storage thread: {e}")))?;

        Ok(Self { sender, is_running })
    }

    fn worker_loop(
        db_path: PathBuf,
        receiver: Receiver<StorageCommand>,
        is_running: Arc<AtomicBool>,
    ) -> Result<(), AgentError> {
        let conn = Connection::open(&db_path)
            .map_err(|e| AgentError::StorageError(format!("Failed to open SQLite DB: {e}")))?;

        // Configure hardened low-overhead PRAGMAs
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA journal_size_limit = 67108864; -- 64 MB Hard WAL Cap
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -2000; -- 2 MB Page Cache

            CREATE TABLE IF NOT EXISTS turns (
                turn_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                wall_time_ns INTEGER NOT NULL,
                harness_overhead_ns INTEGER NOT NULL,
                tool_invocations INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id);
            ",
        )
        .map_err(|e| AgentError::StorageError(format!("Failed to initialize DB schema: {e}")))?;

        info!("Storage WAL engine initialized at {:?}", db_path);

        let mut last_checkpoint = Instant::now();
        let checkpoint_interval = Duration::from_secs(10);

        while let Ok(cmd) = receiver.recv() {
            match cmd {
                StorageCommand::CommitTurn(summary) => {
                    let res = conn.execute(
                        "INSERT OR REPLACE INTO turns 
                        (turn_id, session_id, prompt_tokens, completion_tokens, wall_time_ns, harness_overhead_ns, tool_invocations)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            summary.turn_id.to_string(),
                            summary.session_id.to_string(),
                            summary.prompt_tokens,
                            summary.completion_tokens,
                            summary.wall_time_ns,
                            summary.harness_overhead_ns,
                            summary.tool_invocations,
                        ],
                    );

                    if let Err(e) = res {
                        error!("Failed to persist turn {}: {e}", summary.turn_id);
                    }

                    // Passive checkpoint check
                    if last_checkpoint.elapsed() >= checkpoint_interval {
                        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
                        last_checkpoint = Instant::now();
                        debug!("Passive WAL checkpoint executed");
                    }
                }
                StorageCommand::Checkpoint => {
                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
                    last_checkpoint = Instant::now();
                }
                StorageCommand::Shutdown => {
                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                    break;
                }
            }
        }

        is_running.store(false, Ordering::Release);
        info!("Storage WAL engine cleanly shut down.");
        Ok(())
    }

    /// Asynchronously submits a turn summary for background persistence.
    #[inline(always)]
    pub fn commit(&self, summary: TurnSummary) -> Result<(), AgentError> {
        self.sender
            .send(StorageCommand::CommitTurn(summary))
            .map_err(|e| AgentError::StorageError(format!("WAL queue error: {e}")))
    }

    pub fn shutdown(&self) {
        let _ = self.sender.send(StorageCommand::Shutdown);
    }
}
