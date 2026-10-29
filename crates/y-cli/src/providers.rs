use crate::tools::{Result, Workspace};
use std::{collections::HashMap, io::Read};
use y_gateway::{ApiEndpoint, OpenRouterClient, OpenRouterConfig};

pub fn load(ws: &Workspace) -> Result<HashMap<String, ApiEndpoint>> {
    let path = ws.root.join("ycode.providers.json");
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e.into()),
    };
    if crate::tools::is_link(&meta) || !meta.is_file() {
        return Err("Provider configuration must be a regular file".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("Provider configuration exceeds 64 KiB".into());
    }
    let routes: HashMap<String, ApiEndpoint> = serde_json::from_slice(&bytes)?;
    validate(&routes)?;
    Ok(routes)
}
pub fn validate(routes: &HashMap<String, ApiEndpoint>) -> Result<()> {
    if routes.len() > 32 {
        return Err("At most 32 providers may be configured".into());
    }
    for (name, route) in routes {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(
                "Provider names must contain letters, numbers, underscores or hyphens".into(),
            );
        }
        // Validate the URL without loading or requiring credentials for unused roles.
        OpenRouterClient::with_endpoint(
            OpenRouterConfig {
                api_key: String::new(),
                model: "validation".into(),
                referer: String::new(),
                title: String::new(),
                timeout_secs: 300,
                max_tokens: 4096,
            },
            &route.url,
        )?;
    }
    Ok(())
}
