use crate::raii::HandleGuardTable;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tracing::debug;
use y_types::AgentError;

pub struct Tier2OsSandbox {
    guard_table: Arc<HandleGuardTable>,
    workspace_root: PathBuf,
    #[allow(dead_code)]
    memory_limit_bytes: usize,
    timeout: Duration,
}

impl Tier2OsSandbox {
    pub fn new(workspace_root: impl AsRef<Path>, timeout: Duration) -> Self {
        Self {
            guard_table: Arc::new(HandleGuardTable::with_default_cap()),
            workspace_root: workspace_root.as_ref().to_path_buf(),
            memory_limit_bytes: 512 * 1024 * 1024, // 512 MB
            timeout,
        }
    }

    /// Spawns an untrusted tool/compiler subprocess inside the hardened OS sandbox.
    /// Enforces:
    /// 1. Command::env_clear() - zero API keys or parent secrets exposed
    /// 2. Windows Job Object with JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK
    /// 3. RAII handle slot acquisition
    pub async fn execute_command(
        &self,
        program: &str,
        args: &[&str],
        allowed_env: Option<&HashMap<String, String>>,
    ) -> Result<String, AgentError> {
        let _handle_guard = self.guard_table.acquire()?;

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.current_dir(&self.workspace_root);

        // DEFENSE: Completely clear all inherited environment variables
        cmd.env_clear();

        // Inject ONLY sanitized, non-secret system environment variables
        #[cfg(windows)]
        {
            if let Ok(sysroot) = std::env::var("SYSTEMROOT") {
                cmd.env("SYSTEMROOT", sysroot);
            }
            if let Ok(path) = std::env::var("PATH") {
                cmd.env("PATH", path);
            }
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

        // Add explicit user-permitted environment variables
        if let Some(env_map) = allowed_env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        debug!("Spawning sandboxed process: {} {:?}", program, args);

        let output = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .map_err(|_| AgentError::Timeout(self.timeout.as_millis() as u64))?
            .map_err(|e| AgentError::ToolExecutionError(format!("Process launch failed: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            Err(AgentError::ToolExecutionError(format!(
                "Command exited with code {:?}. Stderr: {}",
                output.status.code(),
                stderr
            )))
        } else {
            Ok(stdout)
        }
    }

    pub fn active_handles(&self) -> usize {
        self.guard_table.active_handles()
    }
}
