use std::io::Write;

const MAX_VISIBLE: usize = 8;
const NAME_W: usize = 20;
const PREVIEW_W: usize = 28;
const INNER_W: usize = NAME_W + PREVIEW_W + 3;

pub struct TuiDropdown {
    pub visible: bool,
    items: Vec<(String, String)>,
    selected: usize,
    rendered_lines: usize,
}

impl TuiDropdown {
    pub fn new() -> Self {
        Self {
            visible: false,
            items: Vec::new(),
            selected: 0,
            rendered_lines: 0,
        }
    }

    pub fn update(&mut self, items: Vec<(String, String)>) {
        self.items = items;
        self.selected = 0;
        self.visible = !self.items.is_empty();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.selected = 0;
    }

    pub fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.items.len().min(MAX_VISIBLE) - 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len().min(MAX_VISIBLE) - 1;
        if self.selected < max {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.items.get(self.selected).map(|(n, _)| n.as_str())
    }

    pub fn clear(&mut self, w: &mut impl Write) {
        if self.rendered_lines == 0 {
            return;
        }
        let n = self.rendered_lines;
        // Move cursor down, clearing each line
        let mut buf = String::with_capacity(n * 10);
        for _ in 0..n {
            buf.push_str("\r\n\x1b[2K");
        }
        // Move back up
        buf.push_str(&format!("\x1b[{}A\r", n));
        w.write_all(buf.as_bytes()).ok();
        w.flush().ok();
        self.rendered_lines = 0;
    }

    pub fn render(&mut self, w: &mut impl Write) {
        if self.items.is_empty() {
            return;
        }

        let count = self.items.len().min(MAX_VISIBLE);
        let total_lines = count + 2; // top border + items + bottom border
        let border: String = "-".repeat(INNER_W);

        let mut buf = String::with_capacity(2048);

        // First: scroll to ensure space below cursor.
        // Print newlines to create room, then move back up.
        for _ in 0..total_lines {
            buf.push('\n');
        }
        buf.push_str(&format!("\x1b[{}A", total_lines));

        // Now render each line below the cursor.
        // Top border
        buf.push_str(&format!("\r\n\x1b[2K \x1b[90m+{}+\x1b[0m", border));

        // Items
        for i in 0..count {
            let (ref name, ref preview) = self.items[i];
            let dname = truncate(name, NAME_W);
            let dprev = truncate(preview, PREVIEW_W);

            buf.push_str("\r\n\x1b[2K");
            if i == self.selected {
                buf.push_str(&format!(
                    " \x1b[90m|\x1b[0m\x1b[7m {:nw$} {:>pw$} \x1b[0m\x1b[90m|\x1b[0m",
                    dname, dprev, nw = NAME_W, pw = PREVIEW_W,
                ));
            } else {
                buf.push_str(&format!(
                    " \x1b[90m|\x1b[0m {:nw$} \x1b[2m{:>pw$}\x1b[0m \x1b[90m|\x1b[0m",
                    dname, dprev, nw = NAME_W, pw = PREVIEW_W,
                ));
            }
        }

        // Bottom border
        buf.push_str(&format!("\r\n\x1b[2K \x1b[90m+{}+\x1b[0m", border));

        // Move cursor back up to original position
        buf.push_str(&format!("\x1b[{}A\r", total_lines));

        self.rendered_lines = total_lines;

        w.write_all(buf.as_bytes()).ok();
        w.flush().ok();
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}
