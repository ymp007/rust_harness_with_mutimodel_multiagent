use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use y_arena::WorkingArena;
use y_reactor::SessionReactor;

const UI_HTML: &str = include_str!("../../../ui/index.html");
const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 100_000;

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
    /// Raw request head, retained so security-relevant headers can be inspected
    /// without widening the parsed set.
    head: String,
}

async fn request(socket: &mut TcpStream, port: u16) -> Result<Request, &'static str> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let end = loop {
        if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            if pos + 4 > MAX_HEADERS {
                return Err("Headers too large");
            }
            break pos + 4;
        }
        if bytes.len() > MAX_HEADERS {
            return Err("Headers too large");
        }
        let n = socket.read(&mut chunk).await.map_err(|_| "Read failed")?;
        if n == 0 {
            return Err("Incomplete headers");
        }
        bytes.extend_from_slice(&chunk[..n]);
    };
    // Own the head before the body loop mutates `bytes`.
    let head = std::str::from_utf8(&bytes[..end])
        .map_err(|_| "Invalid headers")?
        .to_owned();
    let mut lines = head.lines();
    let mut parts = lines
        .next()
        .ok_or("Missing request line")?
        .split_whitespace();
    let method = parts.next().ok_or("Missing method")?.to_owned();
    let path = parts.next().ok_or("Missing path")?.to_owned();
    if parts.next() != Some("HTTP/1.1") || parts.next().is_some() {
        return Err("Expected HTTP/1.1");
    }
    let mut length = None;
    let mut host = None;
    let mut origin = None;
    let mut content_type = None;
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("Invalid header")?;
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "host" if host.is_none() => host = Some(value),
            "host" => return Err("Duplicate Host"),
            "origin" if origin.is_none() => origin = Some(value),
            "origin" => return Err("Duplicate Origin"),
            "content-type" => content_type = Some(value),
            "content-length" if length.is_none() => {
                length = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "Invalid Content-Length")?,
                )
            }
            "content-length" | "transfer-encoding" => return Err("Ambiguous request framing"),
            _ => (),
        }
    }
    let allowed_hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !host.is_some_and(|h| allowed_hosts.iter().any(|a| a == h)) {
        return Err("Untrusted Host");
    }
    if let Some(origin) = origin {
        if !allowed_hosts
            .iter()
            .any(|h| origin == format!("http://{h}"))
        {
            return Err("Cross-origin requests are denied");
        }
    }
    if method == "POST"
        && !content_type.is_some_and(|c| c.split(';').next() == Some("application/json"))
    {
        return Err("POST requires application/json");
    }
    let length = length.unwrap_or(0);
    if length > MAX_BODY {
        return Err("Body exceeds 100 KiB");
    }
    while bytes.len() < end + length {
        let n = socket.read(&mut chunk).await.map_err(|_| "Read failed")?;
        if n == 0 {
            return Err("Incomplete body");
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    Ok(Request {
        method,
        path,
        body: bytes[end..end + length].to_vec(),
        head,
    })
}

async fn reply(socket: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let header = format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; frame-ancestors 'none'\r\n\r\n", body.len());
    match tokio::time::timeout(Duration::from_secs(10), async {
        socket.write_all(header.as_bytes()).await?;
        socket.write_all(body.as_bytes()).await?;
        socket.flush().await
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::debug!(%e, status, "client write failed"),
        Err(_) => tracing::warn!(status, "client write timed out"),
    }
}

/// Longest upstream model call the dashboard will wait for, regardless of the
/// operator's configured timeout.
const UPSTREAM_TURN_TIMEOUT: Duration = Duration::from_secs(120);

/// Reads the bearer credential from a validated request's raw headers.
fn bearer_matches(raw_head: &str, expected: &str) -> bool {
    raw_head.lines().any(|line| {
        let Some((key, value)) = line.split_once(':') else {
            return false;
        };
        key.trim().eq_ignore_ascii_case("authorization")
            && value.trim() == format!("Bearer {expected}")
    })
}

pub async fn run_web_server(
    port: u16,
    _reactor: Arc<tokio::sync::Mutex<SessionReactor>>,
    arena: Arc<WorkingArena>,
    storage: Arc<y_storage::StorageEngine>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;

    // `/api/turn` spends the operator's model credits, so it must not be
    // reachable by any local process that happens to guess the port.
    let api_token = std::env::var("Y_DAEMON_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    match &api_token {
        Some(_) => tracing::info!(
            "Experimental dashboard: http://127.0.0.1:{port} (token required for /api/turn)"
        ),
        None => tracing::warn!(
            "Experimental dashboard on http://127.0.0.1:{port} with /api/turn DISABLED. \
             Set Y_DAEMON_TOKEN to enable it."
        ),
    }

    let capacity = Arc::new(tokio::sync::Semaphore::new(16));
    loop {
        let (mut socket, _) = listener.accept().await?;

        // Answer rather than resetting the socket, and do not make cheap routes
        // wait behind an in-flight model call.
        let permit = match tokio::time::timeout(
            Duration::from_millis(250),
            capacity.clone().acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) | Err(_) => {
                reply(
                    &mut socket,
                    "503 Service Unavailable",
                    "application/json",
                    "{\"error\":\"server busy; retry with backoff\"}",
                )
                .await;
                continue;
            }
        };

        let arena = arena.clone();
        let storage = storage.clone();
        let api_token = api_token.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let req =
                match tokio::time::timeout(Duration::from_secs(10), request(&mut socket, port))
                    .await
                {
                    Ok(Ok(req)) => req,
                    result => {
                        let message = match result {
                            Ok(Err(message)) => message,
                            _ => "Request timed out",
                        };
                        reply(
                            &mut socket,
                            "400 Bad Request",
                            "application/json",
                            &serde_json::json!({"error":message}).to_string(),
                        )
                        .await;
                        return;
                    }
                };
            match (req.method.as_str(), req.path.as_str()) {
                ("GET", "/" | "/index.html") => reply(&mut socket, "200 OK", "text/html; charset=utf-8", UI_HTML).await,
                ("GET", "/api/status") => {
                    let body = serde_json::json!({"status":"online","mode":"experimental-dashboard","provider_configured":y_gateway::OpenRouterConfig::from_env().is_ok(),"arena_capacity_bytes":arena.capacity(),"arena_allocated_bytes":arena.allocated_bytes(),"turn_api_enabled":api_token.is_some(),"storage_degraded":storage.is_degraded(),"staged_deltas":storage.staged_deltas(),"evicted_deltas":storage.evicted_deltas(),"working_set_mb":null,"overhead_ms":null});
                    reply(&mut socket, "200 OK", "application/json", &body.to_string()).await;
                }
                ("POST", "/api/turn") => {
                    let Some(expected) = api_token.as_deref() else {
                        reply(&mut socket, "503 Service Unavailable", "application/json",
                              "{\"error\":\"/api/turn is disabled; set Y_DAEMON_TOKEN to enable it\"}").await;
                        return;
                    };
                    if !bearer_matches(&req.head, expected) {
                        reply(&mut socket, "401 Unauthorized", "application/json",
                              "{\"error\":\"a valid Y_DAEMON_TOKEN bearer credential is required\"}").await;
                        return;
                    }
                    let input = serde_json::from_slice::<serde_json::Value>(&req.body).ok().and_then(|v| v["input"].as_str().map(str::to_owned)).filter(|s| !s.trim().is_empty());
                    let Some(input) = input else { reply(&mut socket, "400 Bad Request", "application/json", "{\"error\":\"A nonempty input string is required\"}").await; return; };
                    let cfg = match y_gateway::OpenRouterConfig::from_env() {
                        Ok(cfg) => cfg,
                        Err(e) => { reply(&mut socket, "503 Service Unavailable", "application/json", &serde_json::json!({"error":e.to_string()}).to_string()).await; return; }
                    };
                    let bounded = y_gateway::OpenRouterConfig {
                        timeout_secs: cfg.timeout_secs.min(UPSTREAM_TURN_TIMEOUT.as_secs()),
                        ..cfg.clone()
                    };
                    match tokio::time::timeout(UPSTREAM_TURN_TIMEOUT, y_gateway::chat_once(&input, &bounded)).await {
                        Ok(Ok(text)) => reply(&mut socket, "200 OK", "application/json", &serde_json::json!({"text":text,"model":cfg.model,"live":true,"mode":"plain-chat"}).to_string()).await,
                        Ok(Err(e)) => reply(&mut socket, "502 Bad Gateway", "application/json", &serde_json::json!({"error":e.to_string()}).to_string()).await,
                        Err(_) => reply(&mut socket, "504 Gateway Timeout", "application/json",
                                        &serde_json::json!({"error":format!("Upstream model call exceeded {}s", UPSTREAM_TURN_TIMEOUT.as_secs())}).to_string()).await,
                    }
                }
                ("POST", "/api/reset") => reply(&mut socket, "410 Gone", "application/json", "{\"error\":\"Use /clear in ycode for a new coding session. This dashboard has no session.\"}").await,
                _ => reply(&mut socket, "404 Not Found", "application/json", "{\"error\":\"Not found\"}").await,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn parse(raw: &str) -> Result<Request, &'static str> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        client.write_all(raw.as_bytes()).await.unwrap();
        client.shutdown().await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        request(&mut server, 4040).await
    }
    #[tokio::test]
    async fn http_rejects_cross_origin_oversized_and_ambiguous_requests() {
        for raw in [
            "POST /api/turn HTTP/1.1\r\nHost: 127.0.0.1:4040\r\nOrigin: https://evil.example\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n",
            "POST /api/turn HTTP/1.1\r\nHost: 127.0.0.1:4040\r\nContent-Type: application/json\r\nContent-Length: 100001\r\n\r\n",
            "POST /api/turn HTTP/1.1\r\nHost: 127.0.0.1:4040\r\nContent-Type: application/json\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: attacker.example\r\n\r\n",
        ] { assert!(parse(raw).await.is_err()); }
        assert!(
            parse("GET /api/status HTTP/1.1\r\nHost: 127.0.0.1:4040\r\n\r\n")
                .await
                .is_ok()
        );
    }

    #[test]
    fn bearer_credential_must_match_exactly() {
        let head = "POST /api/turn HTTP/1.1\r\nHost: 127.0.0.1:4040\r\nAuthorization: Bearer s3cret\r\n\r\n";
        assert!(bearer_matches(head, "s3cret"));
        // Case-insensitive header name, but the credential itself is exact.
        let lower = "POST /api/turn HTTP/1.1\r\nhost: 127.0.0.1:4040\r\nauthorization: bearer s3cret\r\n\r\n";
        assert!(
            !bearer_matches(lower, "s3cret"),
            "scheme must match exactly"
        );
        assert!(!bearer_matches(head, "wrong"));
        assert!(!bearer_matches(head, ""));
        assert!(!bearer_matches(
            "POST /api/turn HTTP/1.1\r\nHost: 127.0.0.1:4040\r\n\r\n",
            "s3cret"
        ));
    }
}
