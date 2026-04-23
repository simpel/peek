use std::io::Write;

const MAX_VISIBLE: usize = 8;

pub struct Dropdown {
    pub visible: bool,
    items: Vec<(String, String)>, // (name, preview)
    selected: usize,
    rendered_height: usize,
}

impl Dropdown {
    pub fn new() -> Self {
        Self {
            visible: false,
            items: Vec::new(),
            selected: 0,
            rendered_height: 0,
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
            self.selected = self.items.len() - 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected < self.items.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.items.get(self.selected).map(|(name, _)| name.as_str())
    }

    /// Clear the previously rendered dropdown from the terminal.
    pub fn clear(&mut self, w: &mut impl Write) {
        if self.rendered_height == 0 {
            return;
        }

        // Save cursor position
        write!(w, "\x1b7").ok();

        // Move down and clear each line (border + items + border)
        let total_lines = self.rendered_height + 2;
        for _ in 0..total_lines {
            write!(w, "\n\x1b[2K").ok();
        }

        // Move back up
        write!(w, "\x1b[{}A", total_lines).ok();

        // Restore cursor position
        write!(w, "\x1b8").ok();

        self.rendered_height = 0;
    }

    /// Render the dropdown below the current cursor position.
    pub fn render(&mut self, w: &mut impl Write) {
        if self.items.is_empty() {
            return;
        }

        let count = self.items.len().min(MAX_VISIBLE);
        self.rendered_height = count;

        // Calculate scroll window
        let start = if self.selected >= count {
            self.selected - count + 1
        } else {
            0
        };

        let box_width = 52;

        // Save cursor position
        write!(w, "\x1b7").ok();

        // Move to next line, draw the box
        write!(w, "\n").ok();

        // Top border
        write!(w, "\x1b[2K\x1b[90m┌").ok();
        for _ in 0..box_width {
            write!(w, "─").ok();
        }
        write!(w, "┐\x1b[0m\n").ok();

        // Items
        for i in start..(start + count).min(self.items.len()) {
            let (ref name, ref preview) = self.items[i];

            let display_name = if name.len() > 22 {
                format!("{}...", &name[..19])
            } else {
                name.clone()
            };

            let display_preview = if preview.len() > 26 {
                format!("{}...", &preview[..23])
            } else {
                preview.clone()
            };

            write!(w, "\x1b[2K").ok();

            if i == self.selected {
                // Selected item: highlighted
                write!(
                    w,
                    "\x1b[90m│\x1b[0m \x1b[7;1m {:<22}\x1b[22;2m{:>26} \x1b[0m\x1b[90m│\x1b[0m\n",
                    display_name, display_preview
                )
                .ok();
            } else {
                write!(
                    w,
                    "\x1b[90m│\x1b[0m  {:<22}\x1b[2m{:>26}\x1b[0m \x1b[90m│\x1b[0m\n",
                    display_name, display_preview
                )
                .ok();
            }
        }

        // Bottom border
        write!(w, "\x1b[2K\x1b[90m└").ok();
        for _ in 0..box_width {
            write!(w, "─").ok();
        }
        write!(w, "┘\x1b[0m").ok();

        // Restore cursor position (go back up to where we were)
        write!(w, "\x1b8").ok();

        w.flush().ok();
    }
}
