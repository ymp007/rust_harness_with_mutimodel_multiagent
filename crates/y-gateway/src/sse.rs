use y_types::AgentError;

/// High-performance resilient SSE stream parser.
/// Correctly buffers across chunk boundaries until complete newline delimiters arrive.
pub struct SseStreamParser {
    line_buf: Vec<u8>,
}

impl SseStreamParser {
    pub fn new() -> Self {
        Self {
            line_buf: Vec::with_capacity(4096),
        }
    }

    /// Parses an incoming chunk of SSE streaming data.
    /// Resiliently handles arbitrary chunk boundaries and split multibyte UTF-8 characters.
    pub fn parse_chunk(&mut self, chunk: &[u8]) -> Result<Vec<String>, AgentError> {
        self.line_buf.extend_from_slice(chunk);
        let mut tokens = Vec::new();

        while let Some(pos) = self.line_buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.line_buf.drain(..=pos).collect();
            let line_str = match std::str::from_utf8(&line_bytes) {
                Ok(s) => s.trim(),
                Err(_) => continue,
            };

            if line_str.starts_with("data:") {
                let data = line_str.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    break;
                }

                // Defensive sanitization: Strip zero-width & BiDi overrides
                let mut sanitized = String::with_capacity(data.len());
                for ch in data.chars() {
                    match ch {
                        '\u{202A}'..='\u{202E}' => continue, // BiDi overrides
                        '\u{200B}'..='\u{200F}' => continue, // Zero-width characters
                        _ => sanitized.push(ch),
                    }
                }
                tokens.push(sanitized);
            }
        }

        Ok(tokens)
    }

    pub fn buffered_bytes(&self) -> usize {
        self.line_buf.len()
    }
}

impl Default for SseStreamParser {
    fn default() -> Self {
        Self::new()
    }
}
