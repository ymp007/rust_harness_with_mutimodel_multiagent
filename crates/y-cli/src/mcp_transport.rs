//! Bounded MCP transports. Protocol/lifecycle behavior is owned by the official SDK.
use crate::tools::Result;
use futures_util::{stream::BoxStream, StreamExt};
use http::{HeaderName, HeaderValue};
use rmcp::{
    model::{ClientJsonRpcMessage, ServerJsonRpcMessage},
    transport::{streamable_http_client::*, Transport},
    RoleClient,
};
use sse_stream::{Sse, SseStream};
use std::{
    collections::HashMap,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, ReadBuf};

pub const MAX_WIRE: usize = 1024 * 1024;
pub type Events = BoxStream<'static, std::result::Result<Sse, sse_stream::Error>>;

/// Reject a newline-delimited frame before the SDK accumulates an unbounded line.
pub struct BoundedRead<R> {
    pub inner: R,
    pub size: usize,
}
impl<R: AsyncRead + Unpin> AsyncRead for BoundedRead<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            for byte in &buf.filled()[before..] {
                if *byte == b'\n' {
                    self.size = 0;
                } else {
                    self.size += 1;
                }
                if self.size > MAX_WIRE {
                    buf.set_filled(before);
                    return Poll::Ready(Err(io::Error::other("MCP frame exceeds 1 MiB")));
                }
            }
        }
        result
    }
}

pub fn events(response: reqwest::Response) -> Events {
    // Limit raw event bytes before parsing, including unterminated lines and comments.
    let mut size = 0usize;
    let mut line = 0usize;
    let mut previous_cr = false;
    let mut failed = false;
    let bytes = response
        .bytes_stream()
        .map(move |chunk| -> io::Result<bytes::Bytes> {
            if failed {
                return Err(io::Error::other(
                    "MCP SSE stream closed after oversized event",
                ));
            }
            let chunk = chunk.map_err(io::Error::other)?;
            for byte in &chunk {
                if previous_cr {
                    previous_cr = false;
                    if *byte == b'\n' {
                        continue;
                    }
                }
                size += 1;
                if matches!(*byte, b'\r' | b'\n') {
                    if line == 0 {
                        size = 0;
                    }
                    line = 0;
                    previous_cr = *byte == b'\r';
                } else {
                    line += 1;
                }
                if size > MAX_WIRE {
                    failed = true;
                    return Err(io::Error::other("MCP SSE event exceeds 1 MiB"));
                }
            }
            Ok(chunk)
        });
    SseStream::from_bytes_stream(bytes).boxed()
}
pub async fn body(mut response: reqwest::Response) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
        if bytes.len() + chunk.len() > MAX_WIRE {
            return Err(io::Error::other("MCP HTTP body exceeds 1 MiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Clone)]
pub struct Http {
    pub client: reqwest::Client,
}
type HttpResult<T> = std::result::Result<T, StreamableHttpError<io::Error>>;
impl Http {
    fn request(
        &self,
        method: reqwest::Method,
        uri: &str,
        session: Option<&str>,
        auth: Option<&str>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> reqwest::RequestBuilder {
        let mut request = self
            .client
            .request(method, uri)
            .header("accept", "application/json, text/event-stream");
        if let Some(session) = session {
            request = request.header("mcp-session-id", session);
        }
        if let Some(auth) = auth {
            request = request.bearer_auth(auth);
        }
        for (name, value) in headers {
            request = request.header(name, value);
        }
        request
    }
    async fn checked(&self, response: reqwest::Response) -> HttpResult<reqwest::Response> {
        match response.status().as_u16() {
            401 => Err(StreamableHttpError::Io(io::Error::other("MCP authentication required: configure bearer_env or headers_env; browser OAuth is not integrated"))),
            403 => Err(StreamableHttpError::Io(io::Error::other("MCP access denied: check token scopes and server authorization"))),
            404 => Err(StreamableHttpError::SessionExpired),
            n if !(200..300).contains(&n) => Err(StreamableHttpError::Io(io::Error::other(format!("MCP HTTP status {n}")))),
            _ => Ok(response),
        }
    }
}
impl StreamableHttpClient for Http {
    type Error = io::Error;
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> HttpResult<StreamableHttpPostResponse> {
        let response = self
            .request(
                reqwest::Method::POST,
                &uri,
                session.as_deref(),
                auth.as_deref(),
                headers,
            )
            .json(&message)
            .send()
            .await
            .map_err(io::Error::other)?;
        let status = response.status();
        // Modern discovery against older HTTP middleware often returns a non-JSON 4xx.
        // Re-correlate its error so the official dual-era lifecycle can fall back safely.
        if session.is_none() && status.is_client_error() && !matches!(status.as_u16(), 401 | 403) {
            if let ClientJsonRpcMessage::Request(request) = &message {
                if serde_json::to_value(&request.request)?["method"] == "server/discover" {
                    let bytes = body(response).await?;
                    let error = match serde_json::from_slice::<ServerJsonRpcMessage>(&bytes) {
                        Ok(ServerJsonRpcMessage::Error(e)) => e.error,
                        _ => rmcp::model::ErrorData::invalid_request(
                            format!("Discovery rejected with HTTP {status}"),
                            None,
                        ),
                    };
                    return Ok(StreamableHttpPostResponse::Json(
                        ServerJsonRpcMessage::error(error, Some(request.id.clone())),
                        None,
                    ));
                }
            }
        }
        let response = self.checked(response).await?;
        if response.status().as_u16() == 202 {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        let session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if session.as_ref().is_some_and(|s| s.len() > 1024) {
            return Err(io::Error::other("Oversized MCP session ID").into());
        }
        let content = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if content.starts_with("text/event-stream") {
            return Ok(StreamableHttpPostResponse::Sse(events(response), session));
        }
        if !content.starts_with("application/json") {
            return Err(StreamableHttpError::UnexpectedContentType(Some(
                content.into(),
            )));
        }
        let bytes = body(response).await?;
        Ok(StreamableHttpPostResponse::Json(
            serde_json::from_slice(&bytes)?,
            session,
        ))
    }
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last: Option<String>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> HttpResult<Events> {
        let mut request = self.request(
            reqwest::Method::GET,
            &uri,
            session.as_deref(),
            auth.as_deref(),
            headers,
        );
        if let Some(last) = last {
            request = request.header("last-event-id", last);
        }
        let response = request.send().await.map_err(io::Error::other)?;
        if response.status().as_u16() == 405 {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        }
        let response = self.checked(response).await?;
        if !response
            .headers()
            .get("content-type")
            .is_some_and(|v| v.as_bytes().starts_with(b"text/event-stream"))
        {
            return Err(StreamableHttpError::UnexpectedContentType(None));
        }
        Ok(events(response))
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session: Arc<str>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> HttpResult<()> {
        let response = self
            .request(
                reqwest::Method::DELETE,
                &uri,
                Some(&session),
                auth.as_deref(),
                headers,
            )
            .send()
            .await
            .map_err(io::Error::other)?;
        if matches!(response.status().as_u16(), 404 | 405) {
            return Ok(());
        }
        self.checked(response).await?;
        Ok(())
    }
}

/// Two-endpoint HTTP+SSE for existing 2024-11-05 servers. Never follow another origin.
pub struct LegacySse {
    client: reqwest::Client,
    endpoint: String,
    events: Events,
}
impl LegacySse {
    pub async fn connect(client: reqwest::Client, uri: &str) -> Result<Self> {
        let response = client
            .get(uri)
            .header("accept", "text/event-stream")
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(format!("Legacy MCP SSE returned HTTP {}", response.status()).into());
        }
        if !response
            .headers()
            .get("content-type")
            .is_some_and(|v| v.as_bytes().starts_with(b"text/event-stream"))
        {
            return Err("Legacy MCP SSE needs text/event-stream".into());
        }
        let base = reqwest::Url::parse(uri)?;
        let mut events = events(response);
        while let Some(event) = events.next().await {
            let event = event?;
            if event.event.as_deref() == Some("endpoint") {
                let endpoint =
                    base.join(event.data.as_deref().ok_or("Missing MCP SSE endpoint")?)?;
                if endpoint.origin() != base.origin()
                    || !endpoint.username().is_empty()
                    || endpoint.password().is_some()
                    || endpoint.fragment().is_some()
                {
                    return Err("MCP SSE endpoint must retain the configured origin without embedded credentials".into());
                }
                return Ok(Self {
                    client,
                    endpoint: endpoint.to_string(),
                    events,
                });
            }
        }
        Err("Legacy MCP SSE did not advertise a message endpoint".into())
    }
}
impl Transport<RoleClient> for LegacySse {
    type Error = io::Error;
    fn send(
        &mut self,
        item: ClientJsonRpcMessage,
    ) -> impl std::future::Future<Output = io::Result<()>> + Send + 'static {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        async move {
            let response = client
                .post(&endpoint)
                .json(&item)
                .send()
                .await
                .map_err(io::Error::other)?;
            if !response.status().is_success() {
                return Err(io::Error::other(format!(
                    "Legacy MCP POST status {}",
                    response.status()
                )));
            }
            body(response).await?;
            Ok(())
        }
    }
    async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
        while let Some(event) = self.events.next().await {
            let event = event.ok()?;
            if matches!(event.event.as_deref(), None | Some("message")) {
                if let Some(data) = event.data {
                    return serde_json::from_str(&data).ok();
                }
            }
        }
        None
    }
    async fn close(&mut self) -> io::Result<()> {
        self.events = futures_util::stream::empty().boxed();
        Ok(())
    }
}
