use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Pnpm,
    Npm,
    Yarn,
    Bun,
    Make,
    DockerCompose,
    Cargo,
}

impl Tool {
    pub fn command_prefix(&self) -> &'static str {
        match self {
            Tool::Pnpm => "pnpm ",
            Tool::Npm => "npm run ",
            Tool::Yarn => "yarn ",
            Tool::Bun => "bun run ",
            Tool::Make => "make ",
            Tool::DockerCompose => "docker compose ",
            Tool::Cargo => "cargo ",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tool::Pnpm => "pnpm",
            Tool::Npm => "npm",
            Tool::Yarn => "yarn",
            Tool::Bun => "bun",
            Tool::Make => "make",
            Tool::DockerCompose => "docker compose",
            Tool::Cargo => "cargo",
        }
    }

    /// All command prefixes that should trigger suggestions for this tool.
    pub fn trigger_prefixes(&self) -> &'static [&'static str] {
        match self {
            Tool::Pnpm => &["pnpm ", "pnpm run "],
            Tool::Npm => &["npm run "],
            Tool::Yarn => &["yarn ", "yarn run "],
            Tool::Bun => &["bun run "],
            Tool::Make => &["make "],
            Tool::DockerCompose => &["docker compose ", "docker-compose "],
            Tool::Cargo => &["cargo "],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub name: String,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct ToolScripts {
    pub tool: Tool,
    pub entries: Vec<ScriptEntry>,
}

/// Detect which package manager to use based on lockfile presence.
pub fn detect_package_manager(dir: &Path) -> Option<Tool> {
    if dir.join("pnpm-lock.yaml").exists() {
        Some(Tool::Pnpm)
    } else if dir.join("yarn.lock").exists() {
        Some(Tool::Yarn)
    } else if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        Some(Tool::Bun)
    } else if dir.join("package-lock.json").exists() {
        Some(Tool::Npm)
    } else if dir.join("package.json").exists() {
        // Default to npm if there's a package.json but no lockfile
        Some(Tool::Npm)
    } else {
        None
    }
}

/// Parse scripts from a package.json file.
pub fn parse_package_json_scripts(dir: &Path) -> Result<Vec<ScriptEntry>> {
    let path = dir.join("package.json");
    let content = std::fs::read_to_string(&path)?;
    let parsed: serde_json::Value = serde_json::from_str(&content)?;

    let scripts = parsed
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(name, cmd)| ScriptEntry {
                    name: name.clone(),
                    preview: cmd.as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(scripts)
}

/// Parse targets from a Makefile.
pub fn parse_makefile_targets(dir: &Path) -> Result<Vec<ScriptEntry>> {
    let makefile_names = ["Makefile", "makefile", "GNUmakefile"];
    let path = makefile_names
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.exists());

    let path = match path {
        Some(p) => p,
        None => return Ok(vec![]),
    };

    let content = std::fs::read_to_string(&path)?;
    let mut targets = Vec::new();

    for line in content.lines() {
        // Match lines like "target-name:" but skip variable assignments, pattern rules, and special targets
        if let Some(target) = line.strip_suffix(':') {
            let target = target.trim();
            if target.is_empty()
                || target.starts_with('.')
                || target.starts_with('\t')
                || target.contains('=')
                || target.contains('%')
                || target.contains('$')
            {
                continue;
            }
            // Handle multiple targets on one line (e.g., "clean build:")
            for t in target.split_whitespace() {
                targets.push(ScriptEntry {
                    name: t.to_string(),
                    preview: String::new(),
                });
            }
        } else if let Some((target_part, _deps)) = line.split_once(':') {
            let target_part = target_part.trim();
            if target_part.is_empty()
                || target_part.starts_with('.')
                || target_part.starts_with('\t')
                || target_part.contains('=')
                || target_part.contains('%')
                || target_part.contains('$')
            {
                continue;
            }
            for t in target_part.split_whitespace() {
                targets.push(ScriptEntry {
                    name: t.to_string(),
                    preview: String::new(),
                });
            }
        }
    }

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    targets.retain(|t| seen.insert(t.name.clone()));

    Ok(targets)
}

/// Parse services from a docker-compose file.
pub fn parse_compose_services(dir: &Path) -> Result<Vec<ScriptEntry>> {
    let compose_names = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];
    let path = compose_names
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.exists());

    let path = match path {
        Some(p) => p,
        None => return Ok(vec![]),
    };

    let content = std::fs::read_to_string(&path)?;

    // Simple YAML parsing: find lines under "services:" that are top-level keys
    let mut services = Vec::new();
    let mut in_services = false;

    for line in content.lines() {
        if line.trim_start() == "services:" || line.trim_start().starts_with("services:") {
            in_services = true;
            continue;
        }

        if in_services {
            // A non-indented line means we left the services block
            if !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty() {
                in_services = false;
                continue;
            }

            // Service names are indented by exactly 2 spaces (or 1 tab)
            let trimmed = line.trim_end();
            if trimmed.ends_with(':') {
                let indent = line.len() - line.trim_start().len();
                if indent == 2 || (indent == 1 && line.starts_with('\t')) {
                    let name = trimmed.trim().trim_end_matches(':');
                    if !name.is_empty() {
                        services.push(ScriptEntry {
                            name: name.to_string(),
                            preview: String::new(),
                        });
                    }
                }
            }
        }
    }

    Ok(services)
}

/// Get standard cargo subcommands when Cargo.toml is present.
pub fn parse_cargo_commands(dir: &Path) -> Result<Vec<ScriptEntry>> {
    if !dir.join("Cargo.toml").exists() {
        return Ok(vec![]);
    }

    let commands: Vec<(&str, &str)> = vec![
        ("build", "Compile the current package"),
        ("run", "Run a binary or example"),
        ("test", "Run the tests"),
        ("check", "Check for errors without building"),
        ("clippy", "Run clippy lints"),
        ("fmt", "Format the code"),
        ("bench", "Run benchmarks"),
        ("doc", "Build documentation"),
        ("clean", "Remove build artifacts"),
        ("update", "Update dependencies"),
    ];

    Ok(commands
        .into_iter()
        .map(|(name, preview)| ScriptEntry {
            name: name.to_string(),
            preview: preview.to_string(),
        })
        .collect())
}

/// Scan a directory and return all detected tools with their scripts/targets.
pub fn scan_directory(dir: &Path) -> Vec<ToolScripts> {
    let mut results = Vec::new();

    // Package manager scripts — register for ALL JS package managers so
    // suggestions work regardless of which lockfile is present.
    if dir.join("package.json").exists() {
        if let Ok(entries) = parse_package_json_scripts(dir) {
            if !entries.is_empty() {
                for tool in [Tool::Pnpm, Tool::Npm, Tool::Yarn, Tool::Bun] {
                    results.push(ToolScripts {
                        tool,
                        entries: entries.clone(),
                    });
                }
            }
        }
    }

    // Makefile targets
    if let Ok(entries) = parse_makefile_targets(dir) {
        if !entries.is_empty() {
            results.push(ToolScripts {
                tool: Tool::Make,
                entries,
            });
        }
    }

    // Docker Compose services
    if let Ok(entries) = parse_compose_services(dir) {
        if !entries.is_empty() {
            results.push(ToolScripts {
                tool: Tool::DockerCompose,
                entries,
            });
        }
    }

    // Cargo commands
    if let Ok(entries) = parse_cargo_commands(dir) {
        if !entries.is_empty() {
            results.push(ToolScripts {
                tool: Tool::Cargo,
                entries,
            });
        }
    }

    results
}

/// Given a command line, determine which tool is being invoked and extract the filter text.
pub fn match_tool_prefix(line: &str) -> Option<(Tool, &str)> {
    let all_tools = [
        Tool::Pnpm,
        Tool::Npm,
        Tool::Yarn,
        Tool::Bun,
        Tool::Make,
        Tool::DockerCompose,
        Tool::Cargo,
    ];

    // Try longest prefixes first to avoid partial matches
    let mut best_match: Option<(Tool, &str)> = None;
    let mut best_len = 0;

    for tool in &all_tools {
        for prefix in tool.trigger_prefixes() {
            if line.starts_with(prefix) && prefix.len() > best_len {
                best_match = Some((*tool, &line[prefix.len()..]));
                best_len = prefix.len();
            }
        }
    }

    best_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_tool_prefix() {
        assert_eq!(
            match_tool_prefix("pnpm dev").map(|(t, f)| (t, f)),
            Some((Tool::Pnpm, "dev"))
        );
        assert_eq!(
            match_tool_prefix("pnpm run build").map(|(t, f)| (t, f)),
            Some((Tool::Pnpm, "build"))
        );
        assert_eq!(
            match_tool_prefix("npm run test").map(|(t, f)| (t, f)),
            Some((Tool::Npm, "test"))
        );
        assert_eq!(
            match_tool_prefix("make ").map(|(t, f)| (t, f)),
            Some((Tool::Make, ""))
        );
        assert_eq!(
            match_tool_prefix("docker compose up").map(|(t, f)| (t, f)),
            Some((Tool::DockerCompose, "up"))
        );
        assert_eq!(
            match_tool_prefix("docker-compose up").map(|(t, f)| (t, f)),
            Some((Tool::DockerCompose, "up"))
        );
        assert_eq!(
            match_tool_prefix("cargo t").map(|(t, f)| (t, f)),
            Some((Tool::Cargo, "t"))
        );
        assert!(match_tool_prefix("echo hello").is_none());
    }
}
