use uuid::Uuid;

pub struct ToolOutputSanitizer;

impl ToolOutputSanitizer {
    /// Sanitizes and cryptographically frames untrusted tool output.
    /// Defends against Indirect Prompt Injection and BiDi token confusion.
    pub fn frame_untrusted_output(tool_name: &str, raw_output: &str) -> String {
        let exec_id = Uuid::new_v4();

        // 1. Strip unprintable/BiDi override characters
        let mut sanitized = String::with_capacity(raw_output.len());
        for ch in raw_output.chars() {
            match ch {
                '\u{202A}'..='\u{202E}' => continue, // BiDi overrides
                '\u{200B}'..='\u{200F}' => continue, // Zero-width spaces & marks
                _ => sanitized.push(ch),
            }
        }

        // 2. Escape system instruction tags
        let defanged = sanitized
            .replace("<system>", "&lt;system&gt;")
            .replace("</system>", "&lt;/system&gt;")
            .replace("[INSTRUCTION]", "[ESCAPED_INSTRUCTION]");

        // 3. Wrap in cryptographic boundary tags
        format!(
            "<untrusted_tool_output tool=\"{}\" exec_id=\"{}\">\n{}\n</untrusted_tool_output>",
            tool_name, exec_id, defanged
        )
    }
}
