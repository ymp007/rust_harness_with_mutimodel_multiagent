use crate::{
    design::DesignContext,
    session,
    tools::{is_link, Result, Workspace},
};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::PathBuf};

pub const MAX_TASKS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Developer,
    Reviewer,
    Qa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Passed,
    Failed,
    Interrupted,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub role: Role,
    pub prompt: String,
    pub files: Vec<String>,
    pub dependencies: Vec<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub model_override: bool,
    #[serde(default)]
    pub executed_model: Option<String>,
    #[serde(default)]
    pub executed_provider: Option<String>,
    pub status: Status,
    pub report: String,
    pub commands: Vec<String>,
    pub revision: u64,
    pub epoch: u64,
    pub worktree: Option<PathBuf>,
    pub attempt: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Board {
    pub version: u32,
    pub id: String,
    pub goal: String,
    pub design: String,
    pub model: String,
    #[serde(default)]
    pub model_profile: crate::models::Profile,
    #[serde(default)]
    pub manager_provider: Option<String>,
    pub revision: u64,
    pub epoch: u64,
    pub tasks: Vec<Task>,
    pub base_commit: String,
    pub model_calls: usize,
    pub max_model_calls: usize,
    pub comments: Vec<String>,
    pub directives: Vec<String>,
    pub stopped: bool,
    pub complete: bool,
}

pub fn directory(ws: &Workspace, id: &str) -> Result<PathBuf> {
    if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("Invalid team run ID".into());
    }
    let mut path = ws.root.clone();
    for part in [".ycode", "teams", id] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if is_link(&meta) || !meta.is_dir() => {
                return Err("Team state must use regular directories without links".into())
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&path)?,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}

impl Board {
    pub fn steer(&mut self, note: &str) -> Result<()> {
        if self.complete
            || self.directives.len() >= 32
            || note.trim().is_empty()
            || note.len() > 8192
        {
            return Err(
                "Steering rejected: completed project, empty note, or note limits exceeded".into(),
            );
        }
        self.directives.push(note.into());
        self.epoch += 1;
        Ok(())
    }
    pub fn new(ws: &Workspace, model: &str, goal: &str) -> Result<Self> {
        if goal.len() > 100_000 {
            return Err("Team goal exceeds 100 KiB".into());
        }
        Ok(Self {
            version: 1,
            id: y_types::SessionId::new().to_string(),
            goal: goal.into(),
            design: DesignContext::load(ws)?.summary,
            model: model.into(),
            model_profile: crate::models::Profile::default(),
            manager_provider: None,
            revision: 0,
            epoch: 0,
            tasks: vec![],
            base_commit: String::new(),
            model_calls: 0,
            max_model_calls: 120,
            comments: vec![],
            directives: vec![],
            stopped: false,
            complete: false,
        })
    }

    pub fn save(&self, ws: &Workspace) -> Result<()> {
        let root = directory(ws, &self.id)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Team board exceeds 2 MiB".into());
        }
        session::atomic_write(&root.join("board.json"), &bytes)?;
        let mut text = format!(
            "# Project coordination\n\n{}\n\n## Shared design reference\n\n{}\n\n## Tasks\n",
            self.goal, self.design
        );
        for t in &self.tasks {
            text.push_str(&format!(
                "\n### {} ({:?}, {:?})\n\n{}\n\nDependencies: {:?}\n\nReport: {}\n",
                t.id, t.role, t.status, t.prompt, t.dependencies, t.report
            ));
        }
        text.push_str("\n## Agent comments (untrusted reports)\n\n");
        text.push_str(&self.comments.join("\n\n"));
        text.push_str("\n\n## User steering\n\n");
        text.push_str(&self.directives.join("\n\n"));
        session::atomic_write(&root.join("coordination.md"), text.as_bytes())
    }

    pub fn load(ws: &Workspace, id: &str) -> Result<Self> {
        let path = directory(ws, id)?.join("board.json");
        let meta = fs::symlink_metadata(&path)?;
        if is_link(&meta) || !meta.is_file() {
            return Err("Team board must be a regular file".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Team board too large".into());
        }
        let board: Self = serde_json::from_slice(&bytes)?;
        if board.version != 1
            || board.id != id
            || board.tasks.len() > MAX_TASKS
            || board.goal.len() > 100_000
            || board.design.len() > crate::design::MAX_DESIGN_BYTES
            || board.comments.len() > 256
            || board.directives.len() > 32
            || !(1..=1000).contains(&board.max_model_calls)
        {
            return Err("Invalid team board".into());
        }
        for (n, task) in board.tasks.iter().enumerate() {
            if task.id != format!("task-{}", n + 1)
                || task.prompt.len() > 8192
                || task.report.len() > 8192
                || task.files.len() > 32
                || task.dependencies.len() > MAX_TASKS
                || task.commands.len() > 16
            {
                return Err("Invalid stored task".into());
            }
            for file in &task.files {
                ws.resolve(file)?;
            }
            if task
                .dependencies
                .iter()
                .any(|id| !board.tasks[..n].iter().any(|t| &t.id == id))
            {
                return Err("Invalid stored dependency graph".into());
            }
        }
        Ok(board)
    }

    pub fn assign(
        &mut self,
        ws: &Workspace,
        role: Role,
        prompt: String,
        files: Vec<String>,
        dependencies: Vec<String>,
        model: Option<String>,
    ) -> Result<String> {
        if self.tasks.len() >= MAX_TASKS
            || prompt.trim().is_empty()
            || prompt.len() > 8192
            || files.len() > 32
            || dependencies.len() > MAX_TASKS
        {
            return Err("Task limits exceeded or empty task".into());
        }
        let mut normalized = Vec::new();
        for file in &files {
            ws.resolve(file)?;
            if file == "." || ws.resolve(file)?.is_dir() {
                return Err("Declare exact source file paths, not directories".into());
            }
            normalized.push(
                ws.resolve(file)?
                    .strip_prefix(&ws.root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        if role == Role::Developer && files.is_empty() {
            return Err("Developer needs explicit owned file paths".into());
        }
        for id in &dependencies {
            if !self.tasks.iter().any(|t| &t.id == id) {
                return Err("Dependency must reference an existing task".into());
            }
        }
        if role != Role::Developer
            && !dependencies.iter().any(|id| {
                self.tasks
                    .iter()
                    .any(|t| &t.id == id && t.role == Role::Developer)
            })
        {
            return Err("Review and QA must depend on developer tasks they verify".into());
        }
        let id = format!("task-{}", self.tasks.len() + 1);
        self.tasks.push(Task {
            id: id.clone(),
            role,
            prompt,
            files: normalized,
            dependencies,
            model_override: model.is_some(),
            model,
            executed_model: None,
            executed_provider: None,
            status: Status::Pending,
            report: String::new(),
            commands: vec![],
            revision: 0,
            epoch: self.epoch,
            worktree: None,
            attempt: 0,
        });
        Ok(id)
    }

    pub fn comment(&mut self, author: &str, recipient: Option<&str>, text: &str) -> Result<()> {
        if text.trim().is_empty() || text.len() > 2048 || self.comments.len() >= 256 {
            return Err("Comment limits exceeded".into());
        }
        if let Some(id) = recipient {
            if !self.tasks.iter().any(|t| t.id == id) {
                return Err("Unknown comment recipient".into());
            }
        }
        self.comments.push(format!(
            "[{author} -> {}] {text}",
            recipient.unwrap_or("team")
        ));
        Ok(())
    }

    pub fn ready(&self, task: &Task) -> bool {
        task.status == Status::Pending
            && task.dependencies.iter().all(|id| {
                self.tasks
                    .iter()
                    .any(|t| &t.id == id && t.status == Status::Passed)
            })
            && (task.role == Role::Developer
                || !self.tasks.iter().any(|t| {
                    t.role == Role::Developer
                        && matches!(t.status, Status::Pending | Status::Running)
                }))
    }

    pub fn finish_allowed(&self) -> Result<()> {
        if self.stopped
            || self.tasks.is_empty()
            || self
                .tasks
                .iter()
                .any(|t| !matches!(t.status, Status::Passed | Status::Cancelled))
        {
            return Err("Unfinished or failed tasks remain".into());
        }
        let devs: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.role == Role::Developer && t.status == Status::Passed)
            .collect();
        if devs.is_empty() {
            return Err("No completed development tasks".into());
        }
        for dev in devs {
            for role in [Role::Reviewer, Role::Qa] {
                if !self.tasks.iter().any(|t| {
                    t.role == role
                        && t.status == Status::Passed
                        && t.revision == self.revision
                        && t.epoch == self.epoch
                        && t.dependencies.contains(&dev.id)
                        && (role != Role::Qa || !t.commands.is_empty())
                }) {
                    return Err(format!("{} needs a passing {:?} report on the current revision and user direction; QA requires actual successful command evidence", dev.id, role).into());
                }
            }
        }
        Ok(())
    }
}
