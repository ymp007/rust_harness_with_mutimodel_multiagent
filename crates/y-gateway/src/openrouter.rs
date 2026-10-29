use futures_util::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;
use y_types::AgentError;

pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CONTEXT_BYTES: usize = 256 * 1024;
/// Hard ceiling on any single request/stream, regardless of configuration.
pub const MAX_REQUEST_SECS: u64 = 1800;
/// Idle gap allowed between stream chunks before the read is abandoned.
const STREAM_READ_TIMEOUT_SECS: u64 = 120;

#[derive(Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub model: String,
    pub referer: String,
    pub title: String,
    pub timeout_secs: u64,
    pub max_tokens: u32,
}

impl std::fmt::Debug for OpenRouterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterConfig")
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("timeout_secs", &self.timeout_secs)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

fn error(message: impl Into<String>) -> AgentError {
    AgentError::ProviderError(message.into())
}

/// Only provider settings are loaded. Explicit environment variables win.
pub fn load_dotenv(path: impl AsRef<std::path::Path>) -> Result<(), AgentError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(error(format!("Cannot read .env: {e}"))),
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.strip_prefix("export ").unwrap_or(line).split_once('=')
        else {
            continue;
        };
        let key = key.trim();
        if !key.ends_with("_API_KEY")
            && ![
                "OPENROUTER_API_KEY",
                "OPENROUTER_MODEL",
                "OPENROUTER_TIMEOUT_SECS",
                "OPENROUTER_MAX_TOKENS",
                "OPENROUTER_REFERER",
                "OPENROUTER_TITLE",
            ]
            .contains(&key)
        {
            continue;
        }
        let value = value.trim().trim_matches(['\'', '"']);
        if !value.is_empty() && !value.contains("REPLACE_ME") && std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

impl OpenRouterConfig {
    pub fn from_env() -> Result<Self, AgentError> {
        let api_key = std::env::var("OPENROUTER_API_KEY").map_err(|_| {
            error("OPENROUTER_API_KEY is not set. Set it in your environment or project .env.")
        })?;
        if api_key.trim().len() < 10 || api_key.contains("REPLACE_ME") || api_key.trim() != api_key
        {
            return Err(error(
                "OPENROUTER_API_KEY is empty, a placeholder, or contains surrounding whitespace.",
            ));
        }
        fn number(name: &str, default: u64, max: u64) -> Result<u64, AgentError> {
            match std::env::var(name) {
                Ok(v) => v
                    .parse::<u64>()
                    .ok()
                    .filter(|v| *v > 0 && *v <= max)
                    .ok_or_else(|| error(format!("{name} must be between 1 and {max}."))),
                Err(_) => Ok(default),
            }
        }
        let model =
            std::env::var("OPENROUTER_MODEL").unwrap_or_else(|_| "openai/gpt-4o-mini".into());
        if model.trim().is_empty() {
            return Err(error("OPENROUTER_MODEL must not be empty."));
        }
        Ok(Self {
            api_key,
            model,
            referer: std::env::var("OPENROUTER_REFERER")
                .unwrap_or_else(|_| "http://localhost".into()),
            title: std::env::var("OPENROUTER_TITLE").unwrap_or_else(|_| "Y Code".into()),
            timeout_secs: number("OPENROUTER_TIMEOUT_SECS", 300, MAX_REQUEST_SECS)?,
            max_tokens: number("OPENROUTER_MAX_TOKENS", 4096, 131072)? as u32,
        })
    }
    pub fn redacted_model(&self) -> &str {
        &self.model
    }
}

/// A pooled client shared by all rounds of an agent session.
#[derive(Clone)]
pub struct OpenRouterClient {
    client: reqwest::Client,
    cfg: OpenRouterConfig,
    endpoint: String,
    format: crate::ApiFormat,
    routes: std::sync::Arc<std::collections::HashMap<String, crate::ApiEndpoint>>,
}

impl OpenRouterClient {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    /// Change models while retaining the pooled transport and provider endpoint.
    pub fn with_model(&self, model: &str) -> Result<Self, AgentError> {
        if model.is_empty()
            || model.len() > 256
            || model.chars().any(char::is_whitespace)
            || model.chars().any(char::is_control)
        {
            return Err(error(
                "Model ID must be 1–256 characters without whitespace or controls.",
            ));
        }
        let mut cfg = self.cfg.clone();
        cfg.model = model.into();
        Ok(Self {
            client: self.client.clone(),
            cfg,
            endpoint: self.endpoint.clone(),
            format: self.format,
            routes: self.routes.clone(),
        })
    }

    pub fn new(cfg: OpenRouterConfig) -> Result<Self, AgentError> {
        Self::with_endpoint(cfg, OPENROUTER_URL)
    }

    pub fn with_endpoint(cfg: OpenRouterConfig, endpoint: &str) -> Result<Self, AgentError> {
        register_secret(&cfg.api_key);
        let url = reqwest::Url::parse(endpoint).map_err(|_| error("Invalid provider URL"))?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(error("Provider URLs cannot contain credentials, query strings or fragments; use key_env for authentication"));
        }
        if url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        {
            return Err(error("Provider endpoint requires HTTPS or loopback HTTP."));
        }
        // A whole-request deadline would also bound reading the response body, which
        // silently truncates long streaming generations. Bound the connect phase and
        // each individual read instead, so a slow but progressing stream survives.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(STREAM_READ_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| error(format!("HTTP client build failed: {e}")))?;
        Ok(Self {
            client,
            cfg,
            endpoint: endpoint.into(),
            format: crate::ApiFormat::Openai,
            routes: Default::default(),
        })
    }

    pub fn with_routes(
        mut self,
        routes: std::collections::HashMap<String, crate::ApiEndpoint>,
    ) -> Self {
        self.routes = std::sync::Arc::new(routes);
        self
    }

    pub fn with_provider(&self, name: &str) -> Result<Self, AgentError> {
        let route = self
            .routes
            .get(name)
            .ok_or_else(|| error(format!("Unknown provider: {name}")))?;
        let key = match &route.key_env {
            Some(var) => {
                if var.is_empty() || !var.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    return Err(error("Invalid provider key environment variable name"));
                }
                std::env::var(var).map_err(|_| {
                    error(format!(
                        "Provider credential environment variable {var} is not set"
                    ))
                })?
            }
            None => String::new(),
        };
        if route.key_env.is_some()
            && (key.trim().is_empty() || key.trim() != key || key.contains("REPLACE_ME"))
        {
            return Err(error(
                "Provider key is empty, a placeholder or contains whitespace",
            ));
        }
        let mut cfg = self.cfg.clone();
        cfg.api_key = key;
        register_secret(&cfg.api_key);
        let mut next = Self::with_endpoint(cfg, &route.url)?;
        next.client = self.client.clone();
        next.format = route.format;
        next.routes = self.routes.clone();
        Ok(next)
    }

    fn request(
        &self,
        messages: &[Value],
        tools: &[Value],
        stream: bool,
    ) -> Result<Value, AgentError> {
        if serde_json::to_vec(messages)
            .map_err(|e| error(e.to_string()))?
            .len()
            > MAX_CONTEXT_BYTES
        {
            return Err(error(
                "Conversation exceeds 256 KiB. Start a new session with /clear.",
            ));
        }
        if self.format != crate::ApiFormat::Openai {
            return crate::protocol::request(
                self.format,
                &self.cfg.model,
                self.cfg.max_tokens,
                messages,
                tools,
            );
        }
        let messages: Vec<Value> = messages
            .iter()
            .map(|m| {
                let mut m = m.clone();
                if let Some(o) = m.as_object_mut() {
                    o.remove("_gemini_parts");
                    o.remove("_anthropic_content");
                }
                m
            })
            .collect();
        let mut body = json!({"model": self.cfg.model, "messages": messages, "max_tokens": self.cfg.max_tokens, "stream": stream});
        if !tools.is_empty() {
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
            if reqwest::Url::parse(&self.endpoint)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .as_deref()
                == Some("openrouter.ai")
            {
                body["provider"] = json!({"require_parameters": true});
            }
        }
        Ok(body)
    }

    async fn send(&self, body: &Value) -> Result<reqwest::Response, AgentError> {
        // Only explicit rejections are retried, avoiding duplicate billing on ambiguous network errors.
        for attempt in 0..3 {
            let endpoint = if self.format == crate::ApiFormat::Gemini {
                let model = self
                    .cfg
                    .model
                    .strip_prefix("models/")
                    .unwrap_or(&self.cfg.model);
                if !model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                {
                    return Err(error("Gemini model ID must be a bare model name"));
                }
                self.endpoint.replace("{model}", model)
            } else {
                self.endpoint.clone()
            };
            let mut request = self.client.post(endpoint);
            if !self.cfg.api_key.is_empty() {
                request = match self.format {
                    crate::ApiFormat::Openai => request.bearer_auth(&self.cfg.api_key),
                    crate::ApiFormat::Anthropic => request.header("x-api-key", &self.cfg.api_key),
                    crate::ApiFormat::Gemini => request.header("x-goog-api-key", &self.cfg.api_key),
                };
            }
            if self.format == crate::ApiFormat::Anthropic {
                request = request.header("anthropic-version", "2023-06-01");
            }
            let response =
                request.json(body).send().await.map_err(|_| {
                    error("Provider request failed; check endpoint and connectivity")
                })?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            if attempt < 2 && matches!(status.as_u16(), 429 | 503) {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .min(30);
                drop(response);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            let bytes = bounded_body(response).await?;
            let hint = match status.as_u16() {
                401 => "Check the configured API key environment variable (OPENROUTER_API_KEY by default).",
                402 => "OpenRouter credits exhausted.",
                429 => "Rate limited; try again later.",
                404 => "Check model ID and tool support.",
                _ => "Provider rejected request.",
            };
            let snippet: String = String::from_utf8_lossy(&bytes).chars().take(500).collect();
            return Err(error(format!(
                "Provider HTTP {status}: {hint} {}",
                self.redact(&snippet)
            )));
        }
        unreachable!()
    }

    pub async fn complete(&self, messages: &[Value], tools: &[Value]) -> Result<Value, AgentError> {
        let response = self.send(&self.request(messages, tools, false)?).await?;
        let bytes = bounded_body(response).await?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| error(format!("Invalid provider JSON: {e}")))?;
        self.check_error(&value)?;
        if self.format != crate::ApiFormat::Openai {
            return crate::protocol::response(self.format, &value);
        }
        let choice = value["choices"]
            .as_array()
            .and_then(|c| c.first())
            .ok_or_else(|| error("Empty choices from OpenRouter."))?;
        if matches!(
            choice["finish_reason"].as_str(),
            Some("length" | "content_filter" | "error")
        ) {
            return Err(error(format!(
                "Provider stopped with finish_reason={}. Increase max tokens if truncated.",
                choice["finish_reason"]
            )));
        }
        let message = &choice["message"];
        if message["role"] != "assistant"
            || (message["content"].as_str().unwrap_or("").is_empty()
                && message["tool_calls"]
                    .as_array()
                    .is_none_or(|t| t.is_empty()))
        {
            return Err(error(
                "Provider returned an empty or invalid assistant message.",
            ));
        }
        Ok(message.clone())
    }

    fn check_error(&self, value: &Value) -> Result<(), AgentError> {
        if let Some(err) = value.get("error").filter(|v| !v.is_null()) {
            let detail = self.redact(&err.to_string());
            return Err(error(format!(
                "OpenRouter error: {}",
                detail.chars().take(500).collect::<String>()
            )));
        }
        Ok(())
    }

    fn redact(&self, text: &str) -> String {
        if self.cfg.api_key.is_empty() {
            text.into()
        } else {
            text.replace(&self.cfg.api_key, "[REDACTED]")
        }
    }

    pub async fn stream<F: FnMut(&str)>(
        &self,
        messages: &[Value],
        mut on_token: F,
    ) -> Result<String, AgentError> {
        if self.format != crate::ApiFormat::Openai {
            return Err(error("Streaming is currently supported for OpenAI-compatible APIs only. Omit --stream for this provider."));
        }
        let response = self.send(&self.request(messages, &[], true)?).await?;
        let mut stream = response.bytes_stream();
        let mut parser = crate::SseStreamParser::new();
        let mut full = String::new();
        let mut finished = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(self.cfg.timeout_secs);
        loop {
            let chunk = tokio::time::timeout_at(deadline, stream.next())
                .await
                .map_err(|_| {
                    error(format!(
                        "Stream exceeded {}s without completing",
                        self.cfg.timeout_secs
                    ))
                })?
                .ok_or_else(|| error("Provider stream disconnected unexpectedly."))?;
            let chunk = chunk.map_err(|e| error(format!("Stream failed: {e}")))?;
            for raw in parser.parse_chunk(&chunk)? {
                if raw.trim() == "[DONE]" {
                    finished = true;
                    break;
                }
                let value: Value = serde_json::from_str(&raw)
                    .map_err(|_| error("Invalid JSON in provider stream."))?;
                self.check_error(&value)?;
                if matches!(
                    value["choices"][0]["finish_reason"].as_str(),
                    Some("length" | "content_filter" | "error")
                ) {
                    return Err(error("Provider stream was truncated or filtered."));
                }
                if let Some(piece) = value["choices"][0]["delta"]["content"].as_str() {
                    if full.len() + piece.len() > MAX_BODY_BYTES {
                        return Err(error("Stream output limit exceeded."));
                    }
                    full.push_str(piece);
                    on_token(piece);
                }
            }
            if finished {
                break;
            }
        }
        if !finished {
            // Surface what was received rather than discarding paid-for output.
            return Err(error(if full.is_empty() {
                "Provider stream disconnected before [DONE].".to_string()
            } else {
                format!(
                    "Provider stream ended after {} bytes without [DONE]. Partial output:\n{full}",
                    full.len()
                )
            }));
        }
        if full.is_empty() {
            return Err(error("Empty response from OpenRouter."));
        }
        Ok(full)
    }
}

static SECRETS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
/// Register a credential for tool/error redaction without logging or persisting it.
pub fn register_secret(key: &str) {
    if !key.is_empty() {
        if let Ok(mut keys) = SECRETS.get_or_init(Default::default).lock() {
            if !keys.iter().any(|k| k == key) {
                keys.push(key.into());
            }
        }
    }
}
/// Redact configured credentials from tool output without persisting their values.
pub fn redact_secrets(text: &str) -> String {
    let mut text = text.to_owned();
    if let Some(keys) = SECRETS.get() {
        if let Ok(keys) = keys.lock() {
            for key in keys.iter() {
                text = text.replace(key, "[REDACTED]");
            }
        }
    }
    text
}

async fn bounded_body(response: reqwest::Response) -> Result<Vec<u8>, AgentError> {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| error(format!("Read body failed: {e}")))?;
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(error("Response body exceeds 8 MiB."));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub async fn chat_once(prompt: &str, cfg: &OpenRouterConfig) -> Result<String, AgentError> {
    let message = OpenRouterClient::new(cfg.clone())?
        .complete(&[json!({"role":"user","content":prompt})], &[])
        .await?;
    message["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| error("Missing assistant text"))
}
pub async fn chat_stream<F: FnMut(&str)>(
    prompt: &str,
    cfg: &OpenRouterConfig,
    on_token: F,
) -> Result<String, AgentError> {
    OpenRouterClient::new(cfg.clone())?
        .stream(&[json!({"role":"user","content":prompt})], on_token)
        .await
}
