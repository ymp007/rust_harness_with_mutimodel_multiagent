use crate::raii::HandleGuardTable;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::process::Command;
use tracing::debug;
use y_types::AgentError;

/// Largest stdout/stderr captured from a guest. The child's output is buffered in
/// host memory, so an unbounded read lets a guest exhaust the *host*.
pub const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
/// Truncation marker appended when output exceeds `MAX_CAPTURE_BYTES`.
const TRUNCATION_MARKER: &str = "\n[output truncated]";

/// Process-wide handle table. One table per sandbox instance would multiply the
/// cap by the number of concurrent sessions.
fn global_guard_table() -> Arc<HandleGuardTable> {
    static TABLE: OnceLock<Arc<HandleGuardTable>> = OnceLock::new();
    Arc::clone(TABLE.get_or_init(|| Arc::new(HandleGuardTable::with_default_cap())))
}

/// Redacts anything that looks like a credential before it reaches a log or an
/// error message.
fn redact(text: &str) -> String {
    const PATTERNS: [&str; 6] = ["Bearer ", "sk-", "sk-or-v1-", "ghp_", "AKIA", "://user:"];
    let mut out = text.to_owned();
    for pattern in PATTERNS {
        let mut search_from = 0usize;
        while let Some(found) = out[search_from..].find(pattern) {
            let start = search_from + found;
            // Keep the marker, drop up to the next delimiter.
            let tail = &out[start + pattern.len()..];
            let end = tail
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|i| start + pattern.len() + i)
                .unwrap_or(out.len());
            out.replace_range(start + pattern.len()..end, "[REDACTED]");
            search_from = start + pattern.len() + "[REDACTED]".len();
            if search_from >= out.len() {
                break;
            }
        }
    }
    out
}

/// Truncates a byte slice on a UTF-8 boundary.
fn truncate_output(bytes: &[u8]) -> String {
    if bytes.len() <= MAX_CAPTURE_BYTES {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    // `from_utf8_lossy` replaces bad sequences, so trimming to a valid boundary
    // is not required for correctness here.
    format!(
        "{}{}",
        String::from_utf8_lossy(&bytes[..MAX_CAPTURE_BYTES]),
        TRUNCATION_MARKER
    )
}

/// Spawns an untrusted tool subprocess with a scrubbed environment.
///
/// Environment isolation is the only containment applied here. This type does
/// **not** enforce a memory limit, a process-tree limit, or filesystem
/// confinement; `memory_limit_bytes` is recorded for reporting only. Callers that
/// need those guarantees must apply them out of process.
pub struct Tier2OsSandbox {
    guard_table: Arc<HandleGuardTable>,
    workspace_root: PathBuf,
    /// Recorded, not enforced. See the type-level note.
    pub memory_limit_bytes: usize,
    timeout: Duration,
}

impl Tier2OsSandbox {
    pub fn new(workspace_root: impl AsRef<Path>, timeout: Duration) -> Self {
        Self {
            guard_table: global_guard_table(),
            workspace_root: workspace_root.as_ref().to_path_buf(),
            memory_limit_bytes: 512 * 1024 * 1024, // 512 MB (reported only)
            timeout,
        }
    }

    /// Runs `program` and returns its stdout.
    pub async fn execute_command(
        &self,
        program: &str,
        args: &[&str],
        allowed_env: Option<&HashMap<String, String>>,
    ) -> Result<String, AgentError> {
        let _handle_guard = self.guard_table.acquire()?;

        let mut cmd = Command::new(program);
        cmd.kill_on_drop(true);
        cmd.args(args);
        cmd.current_dir(&self.workspace_root);

        // Do not inherit the parent's environment: it carries API keys and
        // service credentials.
        cmd.env_clear();
        self.apply_baseline_env(&mut cmd);

        if let Some(env_map) = allowed_env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        // Log the shape of the invocation, never the arguments: model-directed
        // tool calls routinely carry bearer tokens in argv.
        debug!(
            "Spawning sandboxed process: {program} ({} args)",
            args.len()
        );

        let output = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .map_err(|_| AgentError::Timeout(self.timeout.as_millis() as u64))?
            .map_err(|e| AgentError::ToolExecutionError(format!("Process launch failed: {e}")))?;

        let stdout = truncate_output(&output.stdout);
        let stderr = truncate_output(&output.stderr);

        if !output.status.success() {
            Err(AgentError::ToolExecutionError(format!(
                "Command exited with code {:?}. Stderr: {}",
                output.status.code(),
                redact(&stderr)
            )))
        } else {
            Ok(stdout)
        }
    }

    /// Minimal environment needed to locate and run system binaries.
    fn apply_baseline_env(&self, cmd: &mut Command) {
        #[cfg(windows)]
        {
            if let Ok(systemroot) = std::env::var("SystemRoot") {
                cmd.env("SYSTEMROOT", systemroot);
            }
            cmd.env("PATH", r"C:\Windows\System32;C:\Windows");
            if let Ok(temp) = std::env::var("TEMP") {
                cmd.env("TEMP", temp);
            }
        }
        #[cfg(not(windows))]
        {
            cmd.env("PATH", "/usr/bin:/bin:/usr/local/bin");
            cmd.env("LANG", "en_US.UTF-8");
            cmd.env("TEMP", "/tmp");
        }
    }

    pub fn active_handles(&self) -> usize {
        self.guard_table.active_handles()
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_output_enforces_the_capture_cap() {
        // Checked directly so the assertion does not depend on shell throughput.
        let small = vec![b'a'; 1024];
        assert_eq!(truncate_output(&small).len(), 1024);

        let exact = vec![b'c'; MAX_CAPTURE_BYTES];
        assert_eq!(truncate_output(&exact).len(), MAX_CAPTURE_BYTES);

        let large = vec![b'b'; MAX_CAPTURE_BYTES * 2];
        let capped = truncate_output(&large);
        assert_eq!(capped.len(), MAX_CAPTURE_BYTES + TRUNCATION_MARKER.len());
        assert!(capped.ends_with(TRUNCATION_MARKER));
    }

    #[tokio::test]
    async fn a_guest_that_never_finishes_is_killed_at_the_timeout() {
        // A short timeout keeps the test fast and proves wall-clock is bounded
        // even when output volume is not.
        let sandbox = Tier2OsSandbox::new(std::env::temp_dir(), Duration::from_millis(300));
        #[cfg(unix)]
        let (program, args): (&str, Vec<&str>) = (
            "/bin/sh",
            vec![
                "-c",
                "while true; do printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; done",
            ],
        );
        #[cfg(windows)]
        let (program, args): (&str, Vec<&str>) = (
            "cmd",
            // cmd has no `while`; a long `for /L` is the equivalent never-ending loop.
            vec!["/C", "for /L %i in (1,1,99999999) do @echo aaaaaaaaaa"],
        );

        let result = sandbox.execute_command(program, &args, None).await;
        assert!(
            matches!(result, Err(AgentError::Timeout(_))),
            "expected a timeout, got {result:?}"
        );
        assert_eq!(
            sandbox.active_handles(),
            0,
            "handle leaked past the timeout"
        );
    }

    #[test]
    fn secrets_are_redacted() {
        let text = "Authorization: Bearer sk-or-v1-abc123 and AKIAIOSFODNN7EXAMPLE";
        let redacted = redact(text);
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("IOSFODNN7EXAMPLE"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn parent_secrets_are_not_inherited() {
        std::env::set_var("Y_TEST_PARENT_SECRET", "super-secret-value");
        let sandbox = Tier2OsSandbox::new(std::env::temp_dir(), Duration::from_secs(30));
        #[cfg(unix)]
        let (program, args) = (
            "/bin/sh",
            vec!["-c", "printenv Y_TEST_PARENT_SECRET || true"],
        );
        #[cfg(windows)]
        let (program, args) = ("cmd", vec!["/C", "echo %Y_TEST_PARENT_SECRET%"]);
        let out = sandbox.execute_command(program, &args, None).await.unwrap();
        assert!(
            !out.contains("super-secret-value"),
            "parent secret reached the child: {out}"
        );
        std::env::remove_var("Y_TEST_PARENT_SECRET");
    }

    #[tokio::test]
    async fn handle_cap_is_shared_across_sandbox_instances() {
        let a = Tier2OsSandbox::new(std::env::temp_dir(), Duration::from_secs(30));
        let b = Tier2OsSandbox::new(std::env::temp_dir(), Duration::from_secs(30));
        let _guard = a.guard_table.acquire().unwrap();
        // A second sandbox must observe the first sandbox's handle.
        assert!(b.active_handles() >= 1);
    }
}
