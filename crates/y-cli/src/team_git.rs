use crate::tools::{Result, Workspace};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn git(
    root: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args([
        "-c",
        "core.hooksPath=",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "core.longpaths=true",
        "-c",
        "user.name=Y Code",
        "-c",
        "user.email=ycode@localhost",
    ])
    .args(args)
    .current_dir(root)
    .env_clear()
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
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
        "LANG",
    ] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    if let Some(index) = index {
        cmd.env("GIT_INDEX_FILE", git_path(index)?);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut child = cmd.spawn()?;
    let mut stdin = child.stdin.take().ok_or("Missing Git stdin")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("Missing Git stdout")?
        .take(8 * 1024 * 1024 + 1);
    let mut stderr = child
        .stderr
        .take()
        .ok_or("Missing Git stderr")?
        .take(64 * 1024 + 1);
    let work = async {
        let write = async {
            if let Some(input) = input {
                stdin.write_all(input).await?;
            }
            drop(stdin);
            Ok::<_, std::io::Error>(())
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (written, read_out, read_err, status) = tokio::join!(
            write,
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err),
            child.wait()
        );
        written?;
        read_out?;
        read_err?;
        if out.len() > 8 * 1024 * 1024 || err.len() > 64 * 1024 {
            return Err("Git output exceeds limit".into());
        }
        if !status?.success() {
            return Err(format!(
                "Git operation failed: {}",
                crate::tools::display(&String::from_utf8_lossy(&err))
            )
            .into());
        }
        Ok(out)
    };
    tokio::time::timeout(Duration::from_secs(60), work)
        .await
        .map_err(|_| "Git operation timed out")?
}

pub async fn head(ws: &Workspace) -> Result<String> {
    let root =
        String::from_utf8(git(&ws.root, &["rev-parse", "--show-toplevel"], None, None).await?)?;
    if Path::new(root.trim()).canonicalize()? != ws.root {
        return Err("Team mode requires --cwd to be the Git repository root".into());
    }
    Ok(
        String::from_utf8(git(&ws.root, &["rev-parse", "--verify", "HEAD"], None, None).await?)?
            .trim()
            .into(),
    )
}

/// Temporary Git index preserves the user's staged and unstaged changes.
pub async fn snapshot(ws: &Workspace, parent: &str) -> Result<String> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("snapshot-index");
    git(&ws.root, &["read-tree", "--empty"], Some(&index), None).await?;
    let listing = git(
        &ws.root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        None,
        None,
    )
    .await?;
    let mut paths = Vec::new();
    for bytes in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let name = std::str::from_utf8(bytes)?;
        if let Ok(path) = ws.resolve(name) {
            if path.is_file() {
                paths.extend_from_slice(bytes);
                paths.push(0);
            }
        }
    }
    if !paths.is_empty() {
        git(
            &ws.root,
            &[
                "--literal-pathspecs",
                "add",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            Some(&index),
            Some(&paths),
        )
        .await?;
    }
    let tree = String::from_utf8(git(&ws.root, &["write-tree"], Some(&index), None).await?)?;
    let commit = git(
        &ws.root,
        &[
            "commit-tree",
            tree.trim(),
            "-p",
            parent,
            "-m",
            "Y Code isolated team snapshot",
        ],
        Some(&index),
        None,
    )
    .await?;
    Ok(String::from_utf8(commit)?.trim().into())
}

pub async fn worktree(ws: &Workspace, path: &Path, commit: &str) -> Result<Workspace> {
    if path.exists() {
        return Err("Worker checkout already exists; preserve it and use a new task".into());
    }
    let path_arg = git_path(path)?;
    git(
        &ws.root,
        &["worktree", "add", "--detach", &path_arg, commit],
        None,
        None,
    )
    .await?;
    let mut worker = Workspace::new(path)?;
    worker.skills = ws.skills.clone();
    worker.active_skills = ws.active_skills.clone();
    worker.mcp = crate::mcp::Hub::new(
        worker.root.clone(),
        ws.mcp.servers.clone(),
        ws.mcp.preapproved,
    );
    Ok(worker)
}

fn git_path(path: &Path) -> Result<String> {
    let text = path.to_str().ok_or("Non UTF-8 Git path")?;
    #[cfg(windows)]
    {
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return Ok(rest.into());
        }
    }
    Ok(text.into())
}

pub async fn changes(ws: &Workspace, base: &str, commit: &str) -> Result<Vec<String>> {
    let bytes = git(
        &ws.root,
        &["diff", "--name-only", "-z", base, commit],
        None,
        None,
    )
    .await?;
    bytes
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .map(|b| Ok(std::str::from_utf8(b)?.to_owned()))
        .collect()
}

pub async fn integrate(ws: &Workspace, base: &str, commit: &str) -> Result<()> {
    let patch = git(
        &ws.root,
        &["diff", "--binary", base, commit, "--"],
        None,
        None,
    )
    .await?;
    if patch.is_empty() {
        return Ok(());
    }
    git(&ws.root, &["apply", "--check", "-"], None, Some(&patch)).await?;
    git(&ws.root, &["apply", "-"], None, Some(&patch)).await?;
    Ok(())
}
