use crate::tools::{Result, MAX_OUTPUT};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;

// A job/group owns descendants too, so cancellation and timeouts cannot leave compilers running.
#[cfg(windows)]
pub(crate) struct ProcessTree(std::os::windows::io::OwnedHandle);
#[cfg(windows)]
impl ProcessTree {
    pub(crate) fn attach(child: &tokio::process::Child) -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Win32::System::JobObjects::*;
        // SAFETY: every handle returned by CreateJobObjectW is checked for null
        // before use and closed exactly once by `Drop`.
        #[expect(unsafe_code, reason = "Win32 job object FFI is unavoidable here")]
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let tree = Self(std::os::windows::io::OwnedHandle::from_raw_handle(job as _));
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                tree.0.as_raw_handle() as _,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(
                    tree.0.as_raw_handle() as _,
                    child.raw_handle().ok_or("Missing process handle")? as _,
                ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(tree)
        }
    }
}

#[cfg(unix)]
pub(crate) struct ProcessTree(pub(crate) u32);
#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        // SAFETY: `kill` is async-signal-safe and takes no pointers. The negated
        // pid targets this child's own process group, created via
        // `process_group(0)`.
        #[expect(unsafe_code, reason = "libc has no safe process-group API")]
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

async fn capture(mut reader: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let keep = n.min(MAX_OUTPUT.saturating_sub(result.len()));
        result.extend_from_slice(&buf[..keep]);
        // Continue draining after the cap to prevent subprocess pipe deadlock.
    }
    Ok(result)
}

pub async fn run(root: &Path, command: &str) -> Result<String> {
    run_with_timeout(root, command, Duration::from_secs(120)).await
}

async fn run_with_timeout(root: &Path, command: &str, timeout: Duration) -> Result<String> {
    #[cfg(windows)]
    let mut cmd = {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let mut c = tokio::process::Command::new(
            Path::new(&system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        let script = format!("$ErrorActionPreference = 'Stop'; $global:LASTEXITCODE = 0; & {{ {command}\n}}; if (-not $?) {{ exit 1 }}; exit $LASTEXITCODE");
        c.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ]);
        c.creation_flags(0x08000000);
        c
    };
    #[cfg(unix)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.args(["-c", command]);
        c.process_group(0);
        c
    };
    cmd.current_dir(root)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Keep build tool discovery, remove API keys and inherited service credentials.
    for name in [
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "LANG",
    ] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    let mut child = cmd.spawn()?;
    #[cfg(windows)]
    let tree = ProcessTree::attach(&child)?;
    #[cfg(unix)]
    let tree = ProcessTree(child.id().ok_or("Missing process ID")?);
    let out = child.stdout.take().ok_or("Missing stdout")?;
    let err = child.stderr.take().ok_or("Missing stderr")?;
    let work = async {
        let (out, err, status) = tokio::join!(capture(out), capture(err), child.wait());
        Ok::<_, std::io::Error>((out?, err?, status?))
    };
    let result = tokio::time::timeout(timeout, work).await;
    match result {
        Ok(Ok((out, err, status))) => {
            // On Unix the child has been reaped, so its pid is eligible for
            // reuse. Killing the process group now could signal an unrelated
            // group, so the guard is disarmed instead of dropped.
            disarm(tree);
            Ok(format!(
                "Exit code: {:?}\nstdout:\n{}\nstderr:\n{}\n[each stream capped at 32 KiB]",
                status.code(),
                String::from_utf8_lossy(&out),
                String::from_utf8_lossy(&err)
            ))
        }
        Ok(Err(e)) => {
            disarm(tree);
            Err(e.into())
        }
        Err(_) => {
            // The child is still running, so tearing down the group is correct.
            let _ = child.kill().await;
            drop(tree);
            Err(format!(
                "Command exceeded {} seconds and was terminated",
                timeout.as_secs_f64()
            )
            .into())
        }
    }
}

/// Prevents `Drop` from running for an already-reaped process tree.
fn disarm(tree: ProcessTree) {
    #[cfg(unix)]
    std::mem::forget(tree); // No handle resource; avoid signalling a recycled pid.
    #[cfg(windows)]
    drop(tree); // Close the owned Job Object and clean up any remaining descendants.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn timeout_terminates_command() {
        let temp = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let command = "Start-Sleep -Seconds 30";
        #[cfg(unix)]
        let command = "sleep 30";
        let start = std::time::Instant::now();
        assert!(
            run_with_timeout(temp.path(), command, Duration::from_millis(300))
                .await
                .unwrap_err()
                .to_string()
                .contains("terminated")
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
