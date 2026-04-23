/// Tracks what the user has typed on the current command line.
/// This is a best-effort tracker based on raw keystrokes.
pub struct LineTracker {
    line: String,
    prefix_end: usize, // where the tool prefix ends (e.g., "pnpm " = 5)
}

impl LineTracker {
    pub fn new() -> Self {
        Self {
            line: String::new(),
            prefix_end: 0,
        }
    }

    /// Feed raw input bytes from the user.
    pub fn feed(&mut self, data: &[u8]) {
        for &byte in data {
            match byte {
                // Enter / Ctrl-C / Ctrl-D: reset
                0x0d | 0x0a | 0x03 | 0x04 => {
                    self.reset();
                }
                // Backspace / DEL
                0x08 | 0x7f => {
                    self.line.pop();
                }
                // Ctrl-U: clear line
                0x15 => {
                    self.line.clear();
                }
                // Ctrl-W: delete last word
                0x17 => {
                    let trimmed = self.line.trim_end();
                    if let Some(pos) = trimmed.rfind(' ') {
                        self.line.truncate(pos + 1);
                    } else {
                        self.line.clear();
                    }
                }
                // Escape sequences (arrow keys etc): skip
                0x1b => {
                    // Will be followed by more bytes, but we handle those
                    // as they come (they'll be non-printable)
                }
                // Printable ASCII
                0x20..=0x7e => {
                    self.line.push(byte as char);
                }
                // Ignore other control chars and escape sequence bytes
                _ => {}
            }
        }
    }

    pub fn current_line(&self) -> String {
        self.line.clone()
    }

    /// Get the text after the tool prefix (the filter for fuzzy matching).
    pub fn filter_text(&self) -> String {
        if let Some((_tool, filter)) = peek_core::tools::match_tool_prefix(&self.line) {
            filter.to_string()
        } else {
            String::new()
        }
    }

    /// Replace the filter portion of the line with new text.
    pub fn replace_filter(&mut self, replacement: &str) {
        if let Some((tool, _filter)) = peek_core::tools::match_tool_prefix(&self.line) {
            // Find the prefix
            for prefix in tool.trigger_prefixes() {
                if self.line.starts_with(prefix) {
                    self.line = format!("{}{}", prefix, replacement);
                    return;
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.line.clear();
        self.prefix_end = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_typing() {
        let mut tracker = LineTracker::new();
        tracker.feed(b"pnpm dev");
        assert_eq!(tracker.current_line(), "pnpm dev");
    }

    #[test]
    fn test_backspace() {
        let mut tracker = LineTracker::new();
        tracker.feed(b"pnpm de");
        tracker.feed(&[0x7f]); // backspace
        assert_eq!(tracker.current_line(), "pnpm d");
    }

    #[test]
    fn test_enter_resets() {
        let mut tracker = LineTracker::new();
        tracker.feed(b"pnpm dev");
        tracker.feed(&[0x0d]); // enter
        assert_eq!(tracker.current_line(), "");
    }

    #[test]
    fn test_filter_text() {
        let mut tracker = LineTracker::new();
        tracker.feed(b"pnpm dv");
        assert_eq!(tracker.filter_text(), "dv");
    }
}
