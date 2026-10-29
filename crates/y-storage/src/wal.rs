use crossbeam_channel::{bounded, Receiver, Sender};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use y_types::{AgentError, TurnSummary};

pub const WAL_CHANNEL_CAPACITY: usize = 4096;
const SCHEMA_VERSION: i64 = 1;
const READY_TIMEOUT: Duration = Duration::from_secs(5);

pub enum StorageCommand {
    CommitTurn(TurnSummary),
    Checkpoint,
    Shutdown,
}

pub struct WalWorker {
    sender: Sender<StorageCommand>,
    is_running: Arc<AtomicBool>,
    has_error: Arc<AtomicBool>,
    thread: parking_lot::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WalWorker {
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    /// True once any persist has failed. Surfaces silent data loss to callers.
    pub fn has_error(&self) -> bool {
        self.has_error.load(Ordering::Acquire)
    }

    pub fn start(db_path: impl AsRef<Path>) -> Result<Self, AgentError> {
        let db_path = db_path.as_ref().to_path_buf();
        let (sender, receiver) = bounded(WAL_CHANNEL_CAPACITY);
        let is_running = Arc::new(AtomicBool::new(true));
        let has_error = Arc::new(AtomicBool::new(false));
        let running_flag = is_running.clone();
        let error_flag = has_error.clone();
        let (ready_tx, ready_rx) = bounded(1);

        let thread = std::thread::Builder::new()
            .name("y-storage-wal".to_string())
            .spawn(move || {
                match Self::worker_loop(db_path, receiver, &ready_tx, &error_flag) {
                    Ok(()) => info!("Storage WAL worker stopped cleanly."),
                    Err(e) => {
                        let _ = ready_tx.try_send(Err(e.to_string()));
                        error!("Storage WAL worker thread failed: {e}");
                        error_flag.store(true, Ordering::Release);
                    }
                }
                running_flag.store(false, Ordering::Release);
            })
            .map_err(|e| {
                AgentError::StorageError(format!("Failed to spawn storage thread: {e}"))
            })?;

        // A stalled or slow mount must not hang startup indefinitely.
        match ready_rx.recv_timeout(READY_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                sender,
                is_running,
                has_error,
                thread: parking_lot::Mutex::new(Some(thread)),
            }),
            result => {
                let _ = thread.join();
                Err(AgentError::StorageError(format!(
                    "Storage initialization failed: {result:?}"
                )))
            }
        }
    }

    fn worker_loop(
        db_path: PathBuf,
        receiver: Receiver<StorageCommand>,
        ready: &Sender<Result<(), String>>,
        failed: &AtomicBool,
    ) -> Result<(), AgentError> {
        // The parent directory must already exist: creating it here would turn
        // a misconfigured path into a silently created tree of directories.
        let conn = Connection::open(&db_path)
            .map_err(|e| AgentError::StorageError(format!("Failed to open SQLite DB: {e}")))?;

        Self::init_schema(&conn)?;

        info!("Storage WAL engine initialized at {:?}", db_path);
        let _ = ready.send(Ok(()));

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

                    // Never abort the loop on a single failed persist: doing so
                    // drops the Connection and the Receiver, silently discarding
                    // every queued turn while producers still see Ok(()).
                    if let Err(e) = res {
                        error!(
                            "Failed to persist turn {}: {e}. Queue continues draining.",
                            summary.turn_id
                        );
                        failed.store(true, Ordering::Release);
                    }

                    if last_checkpoint.elapsed() >= checkpoint_interval {
                        Self::checkpoint(&conn, false);
                        last_checkpoint = Instant::now();
                    }
                }
                StorageCommand::Checkpoint => {
                    Self::checkpoint(&conn, false);
                    last_checkpoint = Instant::now();
                }
                StorageCommand::Shutdown => {
                    Self::checkpoint(&conn, true);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Creates the schema under an explicit version, and rejects any database
    /// whose `turns` table does not match the columns this build writes.
    fn init_schema(conn: &Connection) -> Result<(), AgentError> {
        const CREATE: &str = "
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
            PRAGMA user_version = 1;
            ";
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = FULL;
            PRAGMA journal_size_limit = 67108864;
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -2000;
            ",
        )
        .map_err(|e| AgentError::StorageError(format!("Failed to configure DB pragmas: {e}")))?;

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        if version == 0 {
            // `IF NOT EXISTS` accepts a pre-existing table of any shape, so
            // validate the columns we depend on before writing.
            Self::assert_turns_schema(conn)?;
            conn.execute_batch(CREATE).map_err(|e| {
                AgentError::StorageError(format!("Failed to initialize DB schema: {e}"))
            })?;
        } else if version != SCHEMA_VERSION {
            return Err(AgentError::StorageError(format!(
                "Database schema version {version} is not supported (expected {SCHEMA_VERSION}). \
                 Migrate or remove the database file."
            )));
        }

        conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id);")
            .map_err(|e| AgentError::StorageError(format!("Failed to create index: {e}")))?;
        Ok(())
    }

    /// Fails fast rather than letting every INSERT die silently.
    fn assert_turns_schema(conn: &Connection) -> Result<(), AgentError> {
        const REQUIRED: [&str; 7] = [
            "turn_id",
            "session_id",
            "prompt_tokens",
            "completion_tokens",
            "wall_time_ns",
            "harness_overhead_ns",
            "tool_invocations",
        ];
        // A fresh database has no `turns` table yet; nothing to validate.
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='turns'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|e| {
                AgentError::StorageError(format!("Cannot inspect database schema: {e}"))
            })?;
        if !exists {
            return Ok(());
        }

        let stmt = conn
            .prepare("SELECT * FROM turns LIMIT 0")
            .map_err(|e| AgentError::StorageError(format!("Cannot inspect turns table: {e}")))?;
        let found: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(str::to_ascii_lowercase)
            .collect();
        drop(stmt);
        let missing: Vec<&str> = REQUIRED
            .iter()
            .copied()
            .filter(|c| !found.iter().any(|f| f == c))
            .collect();
        if !missing.is_empty() {
            return Err(AgentError::StorageError(format!(
                "Existing database is missing required turns columns: {missing:?}. \
                 Remove the database file to start fresh."
            )));
        }
        Ok(())
    }

    /// `wal_checkpoint` reports blockage in a returned column, not as an SQL
    /// error, so the result must be inspected explicitly.
    fn checkpoint(conn: &Connection, truncate: bool) {
        let pragma = if truncate {
            "PRAGMA wal_checkpoint(TRUNCATE)"
        } else {
            "PRAGMA wal_checkpoint(PASSIVE)"
        };
        match conn.query_row(pragma, [], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        }) {
            Ok((0, _, _)) => debug!("WAL checkpoint completed."),
            Ok((_, log, ckpt)) => warn!(
                "WAL checkpoint blocked (log={log}, checkpointed={ckpt}); \
                 WAL will keep growing until the reader disconnects."
            ),
            Err(e) => error!("WAL checkpoint failed: {e}"),
        }
    }

    /// Submits a turn summary for background persistence. Reports worker death
    /// and prior failures instead of silently accepting into a dead channel.
    #[inline(always)]
    pub fn commit(&self, summary: TurnSummary) -> Result<(), AgentError> {
        if !self.is_running.load(Ordering::Acquire) {
            return Err(AgentError::StorageError(
                "WAL worker thread is not running; the turn was not recorded.".into(),
            ));
        }
        if self.has_error.load(Ordering::Acquire) {
            return Err(AgentError::StorageError(
                "A previous turn failed to persist; storage is degraded.".into(),
            ));
        }
        self.sender
            .try_send(StorageCommand::CommitTurn(summary))
            .map_err(|e| AgentError::StorageError(format!("WAL queue error: {e}")))
    }

    pub fn shutdown(&self) {
        if let Some(thread) = self.thread.lock().take() {
            let _ = self.sender.send(StorageCommand::Shutdown);
            let _ = thread.join();
            if self.has_error.load(Ordering::Acquire) {
                error!("Storage shut down with one or more failed persists.");
            }
        }
    }
}
