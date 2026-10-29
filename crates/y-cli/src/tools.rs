use serde_json::{json, Value};
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Component, Path, PathBuf};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MAX_FILE: usize = 1024 * 1024;
pub const MAX_OUTPUT: usize = 32 * 1024;
/// Upper bound on files inspected by a single `search` call.
pub const MAX_SEARCH_FILES: usize = 2_000;
/// Upper bound on tool invocations per agent turn.
pub const MAX_TOOL_CALLS_PER_TURN: usize = 100;

/// Never render terminal escape/control sequences from model or repository content.
pub fn display(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .filter(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

pub fn bounded(text: &str) -> String {
    if text.len() <= MAX_OUTPUT {
        return text.to_owned();
    }
    let mut end = MAX_OUTPUT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[output truncated]", &text[..end])
}

pub fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub(crate) fn private(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let stem = name.split('.').next().unwrap_or("");
    let device = ["con", "prn", "aux", "nul", "conin$", "conout$"].contains(&stem)
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.as_bytes()[3].is_ascii_digit());
    device
        || name == ".git"
        || name == ".ycode"
        || name == ".ssh"
        || name == ".aws"
        || name == ".codex"
        || name == ".agents"
        || name == ".claude"
        || name == ".gemini"
        || name == ".mcp.json"
        || name == "ycode.mcp.json"
        || name == "ycode.config.json"
        || name == ".npmrc"
        || name == ".netrc"
        || name == ".env"
        || (name.starts_with(".env.") && name != ".env.example")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name == "credentials"
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.contains("openrouter_key")
}

#[derive(Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub skills: std::sync::Arc<crate::skills::Catalog>,
    pub active_skills: std::sync::Arc<std::sync::Mutex<Vec<crate::skills::Invocation>>>,
    pub mcp: std::sync::Arc<crate::mcp::Hub>,
}
impl Workspace {
    pub fn new(path: &Path) -> Result<Self> {
        let root = path.canonicalize()?;
        if !root.is_dir() {
            return Err("Workspace must be a directory".into());
        }
        Ok(Self {
            mcp: crate::mcp::Hub::new(root.clone(), Default::default(), false),
            root,
            skills: Default::default(),
            active_skills: Default::default(),
        })
    }

    /// Reject absolute paths, traversal, alternate data streams, links and secret files.
    pub fn resolve(&self, path: &str) -> Result<PathBuf> {
        if path.is_empty() || path.contains([':', '\0']) {
            return Err("Invalid workspace path".into());
        }
        let mut current = self.root.clone();
        for component in Path::new(path).components() {
            match component {
                Component::CurDir => (),
                Component::Normal(name) => {
                    let name_text = name.to_string_lossy();
                    if private(&name_text) || name_text.ends_with(['.', ' ']) {
                        return Err("Access to private or ambiguous paths is denied".into());
                    }
                    current.push(name);
                    match fs::symlink_metadata(&current) {
                        Ok(meta) if is_link(&meta) => {
                            return Err("Symbolic links and junctions are denied".into())
                        }
                        Ok(_) => (),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                        Err(e) => return Err(e.into()),
                    }
                }
                _ => return Err("Path must stay inside the workspace (relative paths only)".into()),
            }
        }
        Ok(current)
    }

    pub fn read(&self, path: &str) -> Result<String> {
        let file_path = self.resolve(path)?;
        let file = fs::File::open(file_path)?;
        if !file.metadata()?.is_file() {
            return Err("Expected a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take((MAX_FILE + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE {
            return Err("File exceeds 1 MiB; narrow your request".into());
        }
        Ok(String::from_utf8(bytes).map_err(|_| "Binary file cannot be read as UTF-8")?)
    }

    pub fn files(&self, path: &str) -> Result<Vec<String>> {
        let base = self.resolve(path)?;
        if base.is_file() {
            return Ok(vec![base
                .strip_prefix(&self.root)?
                .to_string_lossy()
                .replace('\\', "/")]);
        }
        let mut pending = vec![(base, 0)];
        let mut files = Vec::new();
        let mut visited = 0;
        while let Some((dir, depth)) = pending.pop() {
            if depth > 12 {
                continue;
            }
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                visited += 1;
                if visited > 5000 {
                    files.push("[listing truncated at 5000 entries]".into());
                    return Ok(files);
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if private(&name)
                    || ["target", "node_modules", "vendor", ".venv", "dist", "build"]
                        .contains(&name.as_str())
                {
                    continue;
                }
                let meta = fs::symlink_metadata(entry.path())?;
                if is_link(&meta) {
                    continue;
                }
                if meta.is_dir() {
                    pending.push((entry.path(), depth + 1));
                } else if meta.is_file() {
                    files.push(
                        entry
                            .path()
                            .strip_prefix(&self.root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        files.sort();
        Ok(files)
    }
}

#[derive(Clone, Copy)]
pub struct Permissions {
    pub read_only: bool,
    pub allow_write: bool,
    pub allow_shell: bool,
}
impl Permissions {
    async fn approve(&self, kind: &str, detail: &str) -> Result<bool> {
        if self.read_only {
            return Ok(false);
        }
        if (kind == "Run command" && self.allow_shell)
            || (kind != "Run command" && self.allow_write)
        {
            crate::ui::eprintln!("\n{}: {}", kind, display(detail));
            return Ok(true);
        }
        self.prompt(kind, detail).await
    }
    pub(crate) async fn prompt(&self, kind: &str, detail: &str) -> Result<bool> {
        if !io::stdin().is_terminal() {
            return Ok(false);
        }
        if crate::input::active() {
            return crate::input::approve(kind, detail).await;
        }
        crate::ui::eprintln!("\n{}: {}", kind, display(detail));
        tokio::task::spawn_blocking(|| {
            eprint!("Approve this action? [y/N] ");
            io::stderr().flush()?;
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(matches!(
                line.trim().to_ascii_lowercase().as_str(),
                "y" | "yes"
            ))
        })
        .await?
    }
}

pub fn definitions(read_only: bool) -> Vec<Value> {
    fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
        json!({"type":"function", "function":{"name":name,"description":description,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}})
    }
    let string = json!({"type":"string"});
    let mut tools = vec![
        tool("load_skill", "Load portable skill instructions by catalog name or ID. Manual-only skills require explicit user selection. Does not execute scripts or grant permissions.", json!({"name":string,"arguments":string}), &["name"]),
        tool("read_skill_resource", "Read a UTF-8 reference or script relative to a discovered skill package. Read-only; no traversal, secrets or links. Max 32 KiB.", json!({"name":string,"path":string}), &["name","path"]),
        tool("list_files", "List source files recursively, excluding secrets, links and build directories. Paths are relative to workspace.", json!({"path":string}), &["path"]),
        tool("read_file", "Read UTF-8 source with line numbers. Use start_line and end_line for large files. Max 1 MiB file, 32 KiB result.", json!({"path":string,"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1}}), &["path"]),
        tool("search", "Find literal text in source files (case sensitive). Max 100 matches.", json!({"path":string,"query":string}), &["path","query"]),
    ];
    if !read_only {
        tools.extend([
            tool("write_file", "Create a NEW UTF-8 file. Parent directory must exist. Never overwrites. Requires user permission.", json!({"path":string,"content":string}), &["path","content"]),
            tool("edit_file", "Replace one unique exact occurrence of old_text with new_text. Preserve surrounding content. Requires user permission.", json!({"path":string,"old_text":string,"new_text":string}), &["path","old_text","new_text"]),
            tool("run_command", "Run a shell command in the workspace. Windows PowerShell or Unix sh. Requires user permission; shell has normal OS access, not filesystem isolation. Timeout 120s, bounded output.", json!({"command":string}), &["command"]),
        ]);
    }
    tools
}

pub fn definitions_for(ws: &Workspace, read_only: bool) -> Vec<Value> {
    let mut definitions = definitions(read_only);
    if ws.skills.entries.is_empty() {
        definitions.retain(|d| {
            !matches!(
                d["function"]["name"].as_str(),
                Some("load_skill" | "read_skill_resource")
            )
        });
    }
    definitions.extend(crate::mcp::definitions(ws, read_only));
    definitions
}

fn arg<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .ok_or_else(|| format!("Missing string argument: {name}").into())
}

pub async fn execute(
    ws: &Workspace,
    permissions: &Permissions,
    name: &str,
    args: &Value,
) -> Result<String> {
    if matches!(
        name,
        "mcp_list" | "mcp_call" | "mcp_read_resource" | "mcp_get_prompt"
    ) {
        return crate::mcp::execute(ws, permissions, name, args).await;
    }
    let result = match name {
        "load_skill" => ws.skills.load(
            arg(args, "name")?,
            args["arguments"].as_str().unwrap_or(""),
            &ws.active_skills
                .lock()
                .map_err(|_| "Skill state poisoned")?,
        )?,
        "read_skill_resource" => ws.skills.resource(
            arg(args, "name")?,
            arg(args, "path")?,
            &ws.active_skills
                .lock()
                .map_err(|_| "Skill state poisoned")?,
        )?,
        "list_files" => ws.files(arg(args, "path")?)?.join("\n"),
        "read_file" => {
            let text = ws.read(arg(args, "path")?)?;
            let start = args
                .get("start_line")
                .map(|v| {
                    v.as_u64()
                        .filter(|n| *n > 0)
                        .ok_or("start_line must be positive")
                })
                .transpose()?
                .unwrap_or(1);
            let end = args
                .get("end_line")
                .map(|v| {
                    v.as_u64()
                        .filter(|n| *n > 0)
                        .ok_or("end_line must be positive")
                })
                .transpose()?
                .unwrap_or(u64::MAX);
            if end < start {
                return Err("end_line must be >= start_line".into());
            }
            let mut result = String::new();
            for (n, line) in text
                .lines()
                .enumerate()
                .filter(|(n, _)| (*n as u64 + 1) >= start && (*n as u64 + 1) <= end)
            {
                result.push_str(&format!("{}: {line}\n", n + 1));
                if result.len() > MAX_OUTPUT {
                    break;
                }
            }
            result
        }
        "search" => {
            let query = arg(args, "query")?;
            if query.is_empty() {
                return Err("query must not be empty".into());
            }
            let mut result = String::new();
            let mut count = 0;
            let mut scanned = 0usize;
            'files: for file in ws.files(arg(args, "path")?)? {
                // Bound the scan: without a budget, one model-directed call can
                // read the entire workspace.
                scanned += 1;
                if scanned > MAX_SEARCH_FILES {
                    result.push_str("[search truncated: scan budget reached]\n");
                    break 'files;
                }
                if let Ok(text) = ws.read(&file) {
                    for (n, line) in text.lines().enumerate() {
                        if line.contains(query) {
                            result.push_str(&format!("{file}:{}: {line}\n", n + 1));
                            count += 1;
                            if count >= 100 || result.len() > MAX_OUTPUT {
                                result.push_str("[search truncated]\n");
                                break 'files;
                            }
                        }
                    }
                }
            }
            if result.is_empty() {
                "No matches".into()
            } else {
                result
            }
        }
        "write_file" | "edit_file" => {
            let path = arg(args, "path")?;
            let target = ws.resolve(path)?;
            let original = if name == "edit_file" {
                Some(ws.read(path)?)
            } else {
                None
            };
            if name == "write_file" && target.exists() {
                return Err("File already exists. Use edit_file.".into());
            }
            let content = if let Some(ref original) = original {
                let old = arg(args, "old_text")?;
                let new = arg(args, "new_text")?;
                // Model-visible line reads normalize separators; preserve the existing Windows file style.
                let crlf =
                    original.contains("\r\n") && !original.replace("\r\n", "").contains('\n');
                let normalize = |text: &str| {
                    if crlf {
                        text.replace("\r\n", "\n").replace('\n', "\r\n")
                    } else {
                        text.to_owned()
                    }
                };
                let old = normalize(old);
                let new = normalize(new);
                if old.is_empty() || original.match_indices(&old).count() != 1 {
                    return Err("old_text must match exactly once; reread the file".into());
                }
                original.replacen(&old, &new, 1)
            } else {
                arg(args, "content")?.into()
            };
            if content.len() > MAX_FILE {
                return Err("File exceeds 1 MiB".into());
            }
            let preview = if name == "edit_file" {
                format!(
                    "{path}\n--- remove\n{}\n+++ insert\n{}",
                    arg(args, "old_text")?,
                    arg(args, "new_text")?
                )
            } else {
                format!("{path}\n+++ new file\n{content}")
            };
            let preview = format!("Workspace: {}\n{preview}", ws.root.display());
            if !permissions.approve("Edit file", &preview).await? {
                return Ok(
                    "Permission denied. Do not retry this action without new user instructions."
                        .into(),
                );
            }
            // Recheck after human review; never silently overwrite concurrent changes.
            let checked = ws.resolve(path)?;
            if checked != target {
                return Err("Path changed during approval".into());
            }
            if let Some(ref original) = original {
                if ws.read(path)? != *original {
                    return Err("File changed during approval; reread before editing".into());
                }
            }
            let mut temp = tempfile::NamedTempFile::new_in(
                target.parent().ok_or("Missing parent directory")?,
            )?;
            if original.is_some() {
                temp.as_file()
                    .set_permissions(fs::metadata(&target)?.permissions())?;
            }
            temp.write_all(content.as_bytes())?;
            temp.as_file().sync_all()?;
            if original.is_some() {
                temp.persist(&target)?;
            } else {
                temp.persist_noclobber(&target)?;
            }
            format!("Saved {path} ({} bytes)", content.len())
        }
        "run_command" => {
            let command = arg(args, "command")?;
            if command.is_empty() || command.len() > 16 * 1024 {
                return Err("Command must be 1..16384 bytes".into());
            }
            if !permissions
                .approve(
                    "Run command",
                    &format!("Workspace: {}\n{command}", ws.root.display()),
                )
                .await?
            {
                return Ok(
                    "Permission denied. Do not retry this action without new user instructions."
                        .into(),
                );
            }
            crate::process::run(&ws.root, command).await?
        }
        _ => return Err(format!("Unknown tool: {name}").into()),
    };
    let result = if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if key.is_empty() {
            result
        } else {
            result.replace(&key, "[REDACTED]")
        }
    } else {
        result
    };
    Ok(bounded(&y_gateway::openrouter::redact_secrets(&result)))
}
