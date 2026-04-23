use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub trigger: Trigger,
    pub max_suggestions: usize,
    pub frecency: FrecencyConfig,
    pub tools: ToolsConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    #[default]
    Auto,
    Tab,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FrecencyConfig {
    pub recency_half_life_days: f64,
    pub frequency_weight: f64,
    pub recency_weight: f64,
}

impl Default for FrecencyConfig {
    fn default() -> Self {
        Self {
            recency_half_life_days: 7.0,
            frequency_weight: 1.0,
            recency_weight: 2.0,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub pnpm: bool,
    pub npm: bool,
    pub yarn: bool,
    pub bun: bool,
    pub make: bool,
    pub docker_compose: bool,
    pub cargo: bool,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            pnpm: true,
            npm: true,
            yarn: true,
            bun: true,
            make: true,
            docker_compose: true,
            cargo: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            trigger: Trigger::Auto,
            max_suggestions: 8,
            frecency: FrecencyConfig::default(),
            tools: ToolsConfig::default(),
        }
    }
}

/// Returns the peek data directory (~/.peek).
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".peek")
}

/// Returns the path to the socket file.
pub fn socket_path() -> PathBuf {
    data_dir().join("peek.sock")
}

/// Returns the path to the PID file.
pub fn pid_path() -> PathBuf {
    data_dir().join("peekd.pid")
}

/// Returns the path to the log directory.
pub fn log_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Load configuration from ~/.peek/config.toml, falling back to defaults.
pub fn load_config() -> Result<Config> {
    let config_path = data_dir().join("config.toml");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    } else {
        Ok(Config::default())
    }
}

impl Config {
    pub fn is_tool_enabled(&self, tool: peek_core::tools::Tool) -> bool {
        use peek_core::tools::Tool;
        match tool {
            Tool::Pnpm => self.tools.pnpm,
            Tool::Npm => self.tools.npm,
            Tool::Yarn => self.tools.yarn,
            Tool::Bun => self.tools.bun,
            Tool::Make => self.tools.make,
            Tool::DockerCompose => self.tools.docker_compose,
            Tool::Cargo => self.tools.cargo,
        }
    }
}
