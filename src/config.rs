//! User configuration from `~/.config/docker-tui/config.toml`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Docker daemon address; `--host` and `DOCKER_HOST` take precedence.
    pub host: Option<String>,
    /// Seconds between container/image list refreshes.
    pub refresh_secs: u64,
    /// Shell to try before the built-in candidates.
    pub shell: Option<String>,
    /// Log lines loaded when opening a container's logs.
    pub log_tail: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: None,
            refresh_secs: 2,
            shell: None,
            log_tail: 500,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Load from `path`; a missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "docker-tui")
            .map(|d| d.config_dir().join("config.toml"))
    }

    /// Refresh interval in UI ticks of `tick_ms`, at least one.
    pub fn refresh_ticks(&self, tick_ms: u64) -> u64 {
        (self.refresh_secs * 1000 / tick_ms.max(1)).max(1)
    }

    /// Daemon address: the CLI flag, then `DOCKER_HOST`, then the config file.
    pub fn resolve_host(&self, flag: Option<String>, env: Option<String>) -> Option<String> {
        match (flag, env) {
            (Some(flag), _) => Some(flag),
            (None, Some(_)) => None,
            (None, None) => self.host.clone(),
        }
    }
}
