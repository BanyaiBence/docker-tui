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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn parses_all_fields() {
        let cfg = Config::parse(
            r#"
            host = "tcp://10.0.0.5:2375"
            refresh_secs = 5
            shell = "/bin/zsh"
            log_tail = 100
            "#,
        )
        .unwrap();
        assert_eq!(cfg.host.as_deref(), Some("tcp://10.0.0.5:2375"));
        assert_eq!(cfg.refresh_secs, 5);
        assert_eq!(cfg.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(cfg.log_tail, 100);
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let cfg = Config::parse("refresh_secs = 10").unwrap();
        assert_eq!(cfg.refresh_secs, 10);
        assert_eq!(cfg.log_tail, 500);
    }

    #[test]
    fn unknown_keys_and_bad_types_are_errors() {
        assert!(
            Config::parse("refresh = 1")
                .unwrap_err()
                .contains("refresh")
        );
        assert!(Config::parse("refresh_secs = \"x\"").is_err());
    }

    #[test]
    fn missing_file_is_default_but_unreadable_toml_is_error() {
        let dir = std::env::temp_dir().join(format!("dtui-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            Config::load(&dir.join("nope.toml")).unwrap(),
            Config::default()
        );
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, "this is not toml").unwrap();
        let err = Config::load(&bad).unwrap_err();
        assert!(err.contains("bad.toml"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refresh_ticks_rounds_and_is_never_zero() {
        let cfg = Config {
            refresh_secs: 2,
            ..Default::default()
        };
        assert_eq!(cfg.refresh_ticks(250), 8);
        let cfg = Config {
            refresh_secs: 0,
            ..Default::default()
        };
        assert_eq!(cfg.refresh_ticks(250), 1);
    }

    #[test]
    fn host_precedence() {
        let cfg = Config {
            host: Some("cfg".into()),
            ..Default::default()
        };
        assert_eq!(
            cfg.resolve_host(Some("flag".into()), Some("env".into()))
                .as_deref(),
            Some("flag")
        );
        // DOCKER_HOST is honoured by bollard's defaults, so the config steps aside.
        assert_eq!(cfg.resolve_host(None, Some("env".into())), None);
        assert_eq!(cfg.resolve_host(None, None).as_deref(), Some("cfg"));
        assert_eq!(Config::default().resolve_host(None, None), None);
    }
}
