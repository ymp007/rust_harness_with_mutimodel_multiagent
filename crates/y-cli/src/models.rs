use crate::{
    session,
    team_board::Role,
    tools::{Result, Workspace},
};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read};
use y_gateway::OpenRouterClient;

pub fn enabled_default() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    pub developer: Option<String>,
    pub reviewer: Option<String>,
    pub qa: Option<String>,
    #[serde(default)]
    pub developer_provider: Option<String>,
    #[serde(default)]
    pub reviewer_provider: Option<String>,
    #[serde(default)]
    pub qa_provider: Option<String>,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            enabled: true,
            developer: None,
            reviewer: None,
            qa: None,
            developer_provider: None,
            reviewer_provider: None,
            qa_provider: None,
        }
    }
}
impl Profile {
    pub fn worker_client(
        &self,
        client: &OpenRouterClient,
        role: Role,
        model: &str,
    ) -> Result<OpenRouterClient> {
        let provider = if self.enabled {
            match role {
                Role::Developer => &self.developer_provider,
                Role::Reviewer => &self.reviewer_provider,
                Role::Qa => &self.qa_provider,
            }
        } else {
            &None
        };
        let client = match provider {
            Some(name) => client.with_provider(name)?,
            None => client.clone(),
        };
        Ok(client.with_model(model)?)
    }
    pub fn configured(&self, role: Role) -> Option<&str> {
        match role {
            Role::Developer => self.developer.as_deref(),
            Role::Reviewer => self.reviewer.as_deref(),
            Role::Qa => self.qa.as_deref(),
        }
    }
    pub fn resolve<'a>(
        &'a self,
        role: Role,
        manager: &'a str,
        requested: Option<&'a str>,
    ) -> Result<&'a str> {
        if !self.enabled {
            return Ok(manager);
        }
        if let Some(model) = requested {
            if model != manager && self.configured(role) != Some(model) {
                return Err("Task model must be the selected manager or the user-configured model for its role".into());
            }
            return Ok(model);
        }
        Ok(self.configured(role).unwrap_or(manager))
    }
    pub fn configure(&mut self, client: &OpenRouterClient, command: &str) -> Result<()> {
        let (role, model) = command
            .trim()
            .split_once(' ')
            .ok_or("Use /models worker|developer|reviewer|qa MODEL (or inherit)")?;
        let mut words = model.split_whitespace();
        let model = words.next().ok_or("Missing model")?;
        let provider = words.next().map(str::to_owned);
        if words.next().is_some() {
            return Err("Use /models ROLE MODEL [PROVIDER]".into());
        }
        if let Some(name) = &provider {
            client.with_provider(name)?;
        }
        let selected = if model == "inherit" {
            None
        } else {
            client.with_model(model)?;
            Some(model.to_owned())
        };
        match role {
            "worker" => {
                self.developer = selected.clone();
                self.reviewer = selected.clone();
                self.qa = selected;
                self.developer_provider = provider.clone();
                self.reviewer_provider = provider.clone();
                self.qa_provider = provider;
            }
            "developer" => {
                self.developer = selected;
                self.developer_provider = provider;
            }
            "reviewer" => {
                self.reviewer = selected;
                self.reviewer_provider = provider;
            }
            "qa" => {
                self.qa = selected;
                self.qa_provider = provider;
            }
            _ => return Err("Unknown model role; use worker, developer, reviewer or qa".into()),
        }
        Ok(())
    }
    pub fn status(&self, manager: &str) -> String {
        format!(
            "Multi-model: {} | manager/plan/design={} | developer={} | reviewer={} | QA={}{} | worker APIs: developer={}, reviewer={}, QA={}",
            if self.enabled { "ON" } else { "OFF" },
            manager,
            self.resolve(Role::Developer, manager, None)
                .unwrap_or(manager),
            self.resolve(Role::Reviewer, manager, None)
                .unwrap_or(manager),
            self.resolve(Role::Qa, manager, None).unwrap_or(manager),
            if self.enabled
                && self.developer.is_none()
                && self.reviewer.is_none()
                && self.qa.is_none()
            {
                " (no worker overrides configured; /models worker MODEL)"
            } else {
                ""
            },
            if self.enabled {self.developer_provider.as_deref().unwrap_or("manager")} else {"manager"},
            if self.enabled {self.reviewer_provider.as_deref().unwrap_or("manager")} else {"manager"},
            if self.enabled {self.qa_provider.as_deref().unwrap_or("manager")} else {"manager"}
        )
    }
}

#[derive(Serialize, Deserialize)]
struct ModeRecord {
    session_id: String,
    enabled: bool,
}
pub fn save_mode(ws: &Workspace, id: &str, enabled: bool) -> Result<()> {
    let root = ws.root.join(".ycode");
    let meta = fs::symlink_metadata(&root)?;
    if crate::tools::is_link(&meta) || !meta.is_dir() {
        return Err("Profile state must be a regular directory".into());
    }
    session::atomic_write(
        &root.join("multimodel-mode.json"),
        &serde_json::to_vec(&ModeRecord {
            session_id: id.into(),
            enabled,
        })?,
    )
}
pub fn load_mode(ws: &Workspace, id: &str) -> Result<Option<bool>> {
    let root = ws.root.join(".ycode");
    match fs::symlink_metadata(&root) {
        Ok(meta) if crate::tools::is_link(&meta) || !meta.is_dir() => {
            return Err("Invalid profile state directory".into())
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let path = root.join("multimodel-mode.json");
    match fs::symlink_metadata(&path) {
        Ok(meta) if crate::tools::is_link(&meta) || !meta.is_file() => {
            return Err("Invalid profile mode file".into())
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(1025).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 {
        return Err("Profile mode file exceeds 1 KiB".into());
    }
    let record: ModeRecord = serde_json::from_slice(&bytes)?;
    Ok((record.session_id == id).then_some(record.enabled))
}
