use std::io::Write;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};

pub struct OverlayProcess {
    child: Child,
}

impl OverlayProcess {
    pub fn spawn() -> Result<Self> {
        let overlay_bin = find_overlay_binary()?;

        let child = Command::new(&overlay_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn overlay: {}", overlay_bin.display()))?;

        Ok(Self { child })
    }

    /// Show the overlay at a specific screen position (CG coordinates: top-left origin).
    /// If pos is None, the overlay will use its own fallback positioning.
    pub fn show(&mut self, items: &[(String, String)], selected: usize, pos: Option<(i32, i32)>) {
        let items_json: Vec<serde_json::Value> = items
            .iter()
            .map(|(name, preview)| {
                serde_json::json!({
                    "name": name,
                    "preview": preview,
                })
            })
            .collect();

        let mut cmd = serde_json::json!({
            "action": "show",
            "items": items_json,
            "selected": selected,
        });

        if let Some((x, y)) = pos {
            cmd["screenX"] = serde_json::json!(x);
            cmd["screenY"] = serde_json::json!(y);
        }

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
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap();
        let overlay = dir.join("peek-overlay");
        if overlay.exists() {
            return Ok(overlay);
        }
    }
    Ok(std::path::PathBuf::from("peek-overlay"))
}
