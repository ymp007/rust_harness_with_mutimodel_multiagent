use y_types::AgentError;

/// Largest single SSE line accepted before the stream is rejected.
pub const MAX_SSE_LINE_BYTES: usize = 256 * 1024;
/// Largest single SSE event (all `data:` fields) accepted.
pub const MAX_SSE_EVENT_BYTES: usize = 1024 * 1024;

/// Resilient SSE stream parser.
///
/// Buffers across chunk boundaries and splits lines on LF, CR, or CRLF as the
/// event-stream specification requires. Size limits are enforced here rather
/// than in callers, so the bound cannot be lost by refactoring.
pub struct SseStreamParser {
    line_buf: Vec<u8>,
    event_data: Vec<String>,
    ended: bool,
}

impl SseStreamParser {
    pub fn new() -> Self {
        Self {
            line_buf: Vec::with_capacity(4096),
            event_data: Vec::new(),
            ended: false,
        }
    }

    fn event_bytes(&self) -> usize {
        self.event_data.iter().map(|s| s.len() + 1).sum()
    }

    fn check_limits(&self) -> Result<(), AgentError> {
        if self.line_buf.len() > MAX_SSE_LINE_BYTES {
            return Err(AgentError::ProviderError(format!(
                "SSE line exceeded {MAX_SSE_LINE_BYTES} bytes"
            )));
        }
        if self.event_bytes() > MAX_SSE_EVENT_BYTES {
            return Err(AgentError::ProviderError(format!(
                "SSE event exceeded {MAX_SSE_EVENT_BYTES} bytes"
            )));
        }
        Ok(())
    }

    /// Parses an incoming chunk of SSE streaming data.
    pub fn parse_chunk(&mut self, chunk: &[u8]) -> Result<Vec<String>, AgentError> {
        if self.ended {
            return Ok(Vec::new());
        }
        self.line_buf.extend_from_slice(chunk);
        self.check_limits()?;
        let mut tokens = Vec::new();

        // A line ends at CR, LF, or CRLF. CRLF must consume both bytes so the
        // LF is not mistaken for a blank line.
        while let Some(pos) = self.line_buf.iter().position(|&b| b == b'\n' || b == b'\r') {
            let line_bytes: Vec<u8> = self.line_buf.drain(..=pos).collect();
            if line_bytes.last() == Some(&b'\r') && self.line_buf.first() == Some(&b'\n') {
                self.line_buf.drain(..1);
            }
            let line_str = match std::str::from_utf8(&line_bytes) {
                Ok(s) => s.trim_end_matches(['\r', '\n']),
                Err(_) => {
                    return Err(AgentError::ProviderError(
                        "Invalid UTF-8 in SSE event".into(),
                    ))
                }
            };

            if line_str.is_empty() {
                if !self.event_data.is_empty() {
                    let data = self.event_data.join("\n");
                    self.event_data.clear();
                    self.ended = data.trim() == "[DONE]";
                    tokens.push(data);
                    if self.ended {
                        self.line_buf.clear();
                        break;
                    }
                }
            } else if let Some(data) = line_str.strip_prefix("data:") {
                // SSE strips one optional space, preserving remaining content.
                self.event_data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
                self.check_limits()?;
            }
        }

        Ok(tokens)
    }

    pub fn buffered_bytes(&self) -> usize {
        self.line_buf.len() + self.event_data.iter().map(|s| s.len() + 1).sum::<usize>()
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }
}

impl Default for SseStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_cr_line_endings_are_split() {
        let mut parser = SseStreamParser::new();
        let tokens = parser.parse_chunk(b"data: hello\rdata: world\r\r").unwrap();
        assert_eq!(tokens, vec!["hello\nworld"]);
    }

    #[test]
    fn crlf_is_not_treated_as_a_blank_line() {
        let mut parser = SseStreamParser::new();
        let tokens = parser.parse_chunk(b"data: a\r\ndata: b\r\n\r\n").unwrap();
        assert_eq!(tokens, vec!["a\nb"]);
    }

    #[test]
    fn lf_line_endings_still_work() {
        let mut parser = SseStreamParser::new();
        assert_eq!(parser.parse_chunk(b"data: x\n\n").unwrap(), vec!["x"]);
    }

    #[test]
    fn events_survive_arbitrary_chunk_boundaries() {
        let mut parser = SseStreamParser::new();
        let mut tokens = Vec::new();
        for chunk in [b"da".as_slice(), b"ta: sp", b"lit\n", b"\ndata: next\n\n"] {
            tokens.extend(parser.parse_chunk(chunk).unwrap());
        }
        assert_eq!(tokens, vec!["split", "next"]);
    }

    #[test]
    fn done_marker_ends_the_stream() {
        let mut parser = SseStreamParser::new();
        assert_eq!(
            parser.parse_chunk(b"data: [DONE]\n\n").unwrap(),
            vec!["[DONE]"]
        );
        assert!(parser.is_ended());
        assert!(parser.parse_chunk(b"data: ignored\n\n").unwrap().is_empty());
    }

    #[test]
    fn unbounded_line_is_rejected_internally() {
        let mut parser = SseStreamParser::new();
        let err = parser
            .parse_chunk(&vec![b'x'; MAX_SSE_LINE_BYTES + 1])
            .unwrap_err();
        assert!(matches!(err, AgentError::ProviderError(_)));
    }

    #[test]
    fn unbounded_event_is_rejected_internally() {
        let mut parser = SseStreamParser::new();
        let line = format!("data: {}\n", "y".repeat(MAX_SSE_LINE_BYTES - 16));
        for _ in 0..8 {
            if parser.parse_chunk(line.as_bytes()).is_err() {
                return;
            }
        }
        panic!("event size limit was never enforced");
    }
}
