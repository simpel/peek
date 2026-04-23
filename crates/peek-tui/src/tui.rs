use std::io::Write;

const MAX_VISIBLE: usize = 8;

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

    /// Erase the previously rendered dropdown from the terminal.
    pub fn clear(&mut self, w: &mut impl Write) {
        if self.rendered_lines == 0 {
            return;
        }
        // Save cursor
        write!(w, "\x1b7").ok();
        // Move down and clear each rendered line
        for _ in 0..self.rendered_lines {
            write!(w, "\n\x1b[2K").ok();
        }
        // Move back up
        write!(w, "\x1b[{}A", self.rendered_lines).ok();
        // Restore cursor
        write!(w, "\x1b8").ok();
        self.rendered_lines = 0;
    }

    /// Render the dropdown below the current cursor line.
    pub fn render(&mut self, w: &mut impl Write) {
        if self.items.is_empty() {
            return;
        }

        let count = self.items.len().min(MAX_VISIBLE);
        let name_width = 20;
        let preview_width = 28;
        let inner_width = name_width + preview_width + 3; // spaces between

        // Save cursor
        write!(w, "\x1b7").ok();

        // Top border
        write!(w, "\n\x1b[2K").ok();
        write!(w, "\x1b[90m ╭").ok();
        for _ in 0..inner_width {
            write!(w, "─").ok();
        }
        write!(w, "╮\x1b[0m").ok();

        // Items
        for i in 0..count {
            let (ref name, ref preview) = self.items[i];

            let display_name = truncate(name, name_width);
            let display_preview = truncate(preview, preview_width);

            write!(w, "\n\x1b[2K").ok();

            if i == self.selected {
                // Selected: cyan background, bold name
                write!(
                    w,
                    "\x1b[90m │\x1b[0m\x1b[46;30;1m {:<nw$}\x1b[22m \x1b[90m{:>pw$}\x1b[0m\x1b[46;30m \x1b[0m\x1b[90m│\x1b[0m",
                    display_name,
                    display_preview,
                    nw = name_width,
                    pw = preview_width,
                )
                .ok();
            } else {
                write!(
                    w,
                    "\x1b[90m │\x1b[0m {:<nw$} \x1b[2m{:>pw$}\x1b[0m \x1b[90m│\x1b[0m",
                    display_name,
                    display_preview,
                    nw = name_width,
                    pw = preview_width,
                )
                .ok();
            }
        }

        // Bottom border
        write!(w, "\n\x1b[2K").ok();
        write!(w, "\x1b[90m ╰").ok();
        for _ in 0..inner_width {
            write!(w, "─").ok();
        }
        write!(w, "╯\x1b[0m").ok();

        // Total lines = top border + items + bottom border
        self.rendered_lines = count + 2;

        // Restore cursor
        write!(w, "\x1b8").ok();
        w.flush().ok();
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}…", &s[..max_len - 1])
    } else {
        s[..max_len].to_string()
    }
}
