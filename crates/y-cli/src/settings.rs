//! User-wide defaults. Credentials remain environment references, never model context.
use crate::tools::{Result, Workspace};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use y_gateway::ApiEndpoint;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Defaults {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub worker_provider: Option<String>,
    pub worker_model: Option<String>,
    pub developer_provider: Option<String>,
    pub developer_model: Option<String>,
    pub reviewer_provider: Option<String>,
    pub reviewer_model: Option<String>,
    pub qa_provider: Option<String>,
    pub qa_model: Option<String>,
    pub multi_model: Option<bool>,
    pub plan: Option<bool>,
    pub sudo: Option<bool>,
    pub team: Option<bool>,
    pub workers: Option<u32>,
    pub team_budget: Option<u32>,
    pub max_steps: Option<u32>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub defaults: Defaults,
    pub providers: HashMap<String, ApiEndpoint>,
    pub skills: crate::skills::Settings,
    #[serde(alias = "mcpServers")]
    pub mcp_servers: std::collections::BTreeMap<String, crate::mcp::Server>,
}
pub fn path(matches: &clap::ArgMatches) -> Result<PathBuf> {
    if let Some(path) = matches.get_one::<String>("config") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("YCODE_CONFIG") {
        if path.is_empty() {
            return Err("YCODE_CONFIG cannot be empty".into());
        }
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or("Cannot locate home directory; set YCODE_CONFIG or --config FILE")?;
    Ok(PathBuf::from(home).join(".config/ycode/config.json"))
}
fn read(path: &Path, required: bool) -> Result<Settings> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
            return Ok(Settings::default())
        }
        Err(e) => return Err(e.into()),
    };
    if crate::tools::is_link(&meta) || !meta.is_file() {
        return Err("Configuration must be a regular file without links".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("Configuration exceeds 64 KiB".into());
    }
    let mut settings: Settings = serde_json::from_slice(&bytes).map_err(|_| {
        "Invalid configuration JSON or unsupported fields; use ycode config init for the schema"
    })?;
    for root in &mut settings.skills.paths {
        if root.is_relative() {
            *root = path.parent().unwrap_or(Path::new(".")).join(&*root);
        }
    }
    settings.validate()?;
    Ok(settings)
}
impl Settings {
    fn validate(&self) -> Result<()> {
        for (value, max) in [
            (self.defaults.workers, 8),
            (self.defaults.team_budget, 1000),
            (self.defaults.max_steps, 100),
        ] {
            if value.is_some_and(|n| n == 0 || n > max) {
                return Err(format!("Configuration limit must be between 1 and {max}").into());
            }
        }
        for model in [
            &self.defaults.model,
            &self.defaults.worker_model,
            &self.defaults.developer_model,
            &self.defaults.reviewer_model,
            &self.defaults.qa_model,
        ]
        .into_iter()
        .flatten()
        {
            if model.is_empty()
                || model.len() > 256
                || model.chars().any(|c| c.is_whitespace() || c.is_control())
            {
                return Err("Invalid model ID in configuration".into());
            }
        }
        crate::mcp::validate(&self.mcp_servers)?;
        crate::providers::validate(&self.providers)
    }
    pub fn load(ws: &Workspace, matches: &clap::ArgMatches) -> Result<Self> {
        let mut settings = if matches.get_flag("no-global-config") {
            Self::default()
        } else {
            read(
                &path(matches)?,
                matches.get_one::<String>("config").is_some()
                    || std::env::var_os("YCODE_CONFIG").is_some(),
            )?
        };
        let project = read(&ws.root.join("ycode.config.json"), false)?;
        // Overlay only supplied fields; omitted project defaults preserve globals.
        let mut merged = serde_json::to_value(&settings.defaults)?;
        for (key, value) in serde_json::to_value(project.defaults)?
            .as_object()
            .ok_or("Invalid defaults")?
        {
            if !value.is_null() {
                merged[key] = value.clone();
            }
        }
        settings.defaults = serde_json::from_value(merged)?;
        settings.providers.extend(project.providers);
        settings.mcp_servers.extend(project.mcp_servers);
        settings
            .mcp_servers
            .extend(crate::mcp::load_project(&ws.root)?);
        if project.skills.enabled.is_some() {
            settings.skills.enabled = project.skills.enabled;
        }
        if project.skills.discover_external.is_some() {
            settings.skills.discover_external = project.skills.discover_external;
        }
        settings.skills.paths.splice(0..0, project.skills.paths);
        settings.providers.extend(crate::providers::load(ws)?);
        settings.validate()?;
        Ok(settings)
    }
    pub fn apply(&self, session: &mut crate::session::Session) {
        let d = &self.defaults;
        session.developer_model = d.developer_model.clone().or_else(|| d.worker_model.clone());
        session.reviewer_model = d.reviewer_model.clone().or_else(|| d.worker_model.clone());
        session.qa_model = d.qa_model.clone().or_else(|| d.worker_model.clone());
        session.developer_provider = d
            .developer_provider
            .clone()
            .or_else(|| d.worker_provider.clone());
        session.reviewer_provider = d
            .reviewer_provider
            .clone()
            .or_else(|| d.worker_provider.clone());
        session.qa_provider = d.qa_provider.clone().or_else(|| d.worker_provider.clone());
        session.multi_model = d.multi_model.unwrap_or(true);
        session.plan_mode = d.plan.unwrap_or(true);
        session.sudo_mode = d.sudo.unwrap_or(true);
        session.team_mode = d.team.unwrap_or(true);
        if let Some(n) = d.workers {
            session.worker_limit = n as usize;
        }
        if let Some(n) = d.team_budget {
            session.team_budget = n as usize;
        }
    }
}
pub fn init(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    for ancestor in parent.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(ancestor) {
            if crate::tools::is_link(&meta) || !meta.is_dir() {
                return Err("Configuration directories must not contain links".into());
            }
        }
    }
    fs::create_dir_all(parent)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    let settings = Settings {
        providers: serde_json::from_str(include_str!("../../../ycode.providers.example.json"))?,
        defaults: Defaults {
            provider: Some("openrouter".into()),
            model: Some("YOUR_MANAGER_MODEL".into()),
            worker_model: Some("YOUR_WORKER_MODEL".into()),
            ..Default::default()
        },
        skills: crate::skills::Settings::default(),
        mcp_servers: Default::default(),
    };
    file.write_all(&serde_json::to_vec_pretty(&settings)?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merged_defaults_apply_to_new_sessions_but_do_not_replace_resumed_choices() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let config = dir.path().join("global.json");
        fs::write(&config,r#"{"defaults":{"worker_model":"global-worker","reviewer_model":"global-reviewer","multi_model":false,"plan":false,"workers":2,"team_budget":15,"max_steps":7}}"#).unwrap();
        fs::write(
            dir.path().join("ycode.config.json"),
            r#"{"defaults":{"developer_model":"project-dev","workers":4}}"#,
        )
        .unwrap();
        let args = crate::cli()
            .try_get_matches_from([
                "ycode",
                "--config",
                config.to_str().unwrap(),
                "--worker-model",
                "cli-worker",
                "--plan",
                "--multi-model",
            ])
            .unwrap();
        let settings = Settings::load(&ws, &args).unwrap();
        let mut session = crate::session::Session::new(&ws, "manager").unwrap();
        settings.apply(&mut session);
        assert_eq!(session.developer_model.as_deref(), Some("project-dev"));
        assert_eq!(session.reviewer_model.as_deref(), Some("global-reviewer"));
        assert_eq!(session.worker_limit, 4);
        assert!(!session.multi_model);
        assert!(!session.plan_mode);
        crate::apply_modes(&args, &mut session, false);
        assert!(session.plan_mode);
        assert!(session.multi_model);
        assert_eq!(session.developer_model.as_deref(), Some("cli-worker"));
        session.save(&ws).unwrap();
        let mut resumed = crate::session::Session::load(&ws, "last").unwrap();
        let args = crate::cli()
            .try_get_matches_from(["ycode", "--resume"])
            .unwrap();
        crate::apply_modes(&args, &mut resumed, false);
        assert_eq!(resumed.developer_model.as_deref(), Some("cli-worker"));
        assert_eq!(settings.defaults.max_steps, Some(7));
    }
    #[test]
    fn malformed_or_oversized_config_and_invalid_limits_fail_without_secret_echo() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        for content in [
            r#"{"defaults":{"workers":0}}"#,
            r#"{"defaults":{"max_steps":101}}"#,
            r#"{"api_key":"secret-value"}"#,
            "not-json",
        ] {
            fs::write(&file, content).unwrap();
            let error = read(&file, true).err().unwrap().to_string();
            assert!(!error.contains("secret-value"));
        }
        fs::write(&file, vec![b' '; 65537]).unwrap();
        assert!(read(&file, true).is_err());
        assert!(read(&dir.path().join("absent"), true).is_err());
        assert!(read(&dir.path().join("absent"), false).is_ok());
    }
}
