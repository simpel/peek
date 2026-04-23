use std::io::Write;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};

pub struct OverlayProcess {
    child: Child,
}

impl OverlayProcess {
    pub fn spawn() -> Result<Self> {
        // Find peek-overlay next to our binary
        let overlay_bin = find_overlay_binary()?;

        let child = Command::new(&overlay_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn overlay: {}", overlay_bin.display()))?;

        Ok(Self { child })
    }

    pub fn show(
        &mut self,
        items: &[(String, String)],
        selected: usize,
        cursor_col: usize,
        cursor_row: usize,
        term_rows: u16,
        term_cols: u16,
    ) {
        let items_json: Vec<serde_json::Value> = items
            .iter()
            .map(|(name, preview)| {
                serde_json::json!({
                    "name": name,
                    "preview": preview,
                })
            })
            .collect();

        let cmd = serde_json::json!({
            "action": "show",
            "items": items_json,
            "selected": selected,
            "cursorCol": cursor_col,
            "cursorRow": cursor_row,
            "terminalRows": term_rows,
            "terminalCols": term_cols,
        });

        self.send_command(&cmd);
    }

    pub fn update_selection(&mut self, selected: usize) {
        let cmd = serde_json::json!({
            "action": "update",
            "selected": selected,
        });
        self.send_command(&cmd);
    }

    pub fn hide(&mut self) {
        let cmd = serde_json::json!({
            "action": "hide",
        });
        self.send_command(&cmd);
    }

    pub fn kill(&mut self) {
        self.child.kill().ok();
    }

    fn send_command(&mut self, cmd: &serde_json::Value) {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let mut json = serde_json::to_string(cmd).unwrap_or_default();
            json.push('\n');
            stdin.write_all(json.as_bytes()).ok();
            stdin.flush().ok();
        }
    }
}

fn find_overlay_binary() -> Result<std::path::PathBuf> {
    // Look next to our own binary
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap();
        let overlay = dir.join("peek-overlay");
        if overlay.exists() {
            return Ok(overlay);
        }
    }

    // Fall back to PATH
    Ok(std::path::PathBuf::from("peek-overlay"))
}
