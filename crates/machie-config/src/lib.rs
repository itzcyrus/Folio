//! Machie configuration: versioned TOML with layered discovery.
//!
//! Precedence (highest wins):
//!   1. `$PWD/machie.toml`          (project-local)
//!   2. `$MACHIE_CONFIG`            (explicit override path)
//!   3. platform user config dir    (`~/.config/machie/config.toml` on Linux)
//! Layers are merged table-by-table; unknown keys are rejected so typos fail loud.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("{path}: unsupported config_version {found} (this build understands version {CONFIG_VERSION})")]
    Version { path: String, found: u32 },
    #[error("{path}: {message}")]
    Validation { path: String, message: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PathsConfig {
    /// Override the data directory (defaults to the platform data dir under `machie`).
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LogConfig {
    /// One of: off | error | warn | info | debug | trace
    pub level: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Schema version of the config file itself. Currently must be 1.
    pub config_version: Option<u32>,
    pub paths: PathsConfig,
    pub log: LogConfig,
}

impl Config {
    /// Load from a single TOML file.
    pub fn load_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&text, &path.display().to_string())
    }

    /// Parse + validate from a string, with `origin` used in error messages.
    pub fn from_toml_str(text: &str, origin: &str) -> Result<Self, ConfigError> {
        let cfg: Config =
            toml::from_str(text).map_err(|source| ConfigError::Parse {
                path: origin.to_string(),
                source,
            })?;
        cfg.validate(origin)?;
        Ok(cfg)
    }

    /// Merge another layer on top of `self`. `other` values win when present.
    pub fn merge_over(&mut self, other: Config) {
        if other.config_version.is_some() {
            self.config_version = other.config_version;
        }
        if other.paths.data_dir.is_some() {
            self.paths.data_dir = other.paths.data_dir;
        }
        if other.log.level.is_some() {
            self.log.level = other.log.level;
        }
    }

    pub fn validate(&self, origin: &str) -> Result<(), ConfigError> {
        if let Some(v) = self.config_version {
            if v != CONFIG_VERSION {
                return Err(ConfigError::Version {
                    path: origin.to_string(),
                    found: v,
                });
            }
        }
        if let Some(level) = &self.log.level {
            const OK: &[&str] = &["off", "error", "warn", "info", "debug", "trace"];
            if !OK.contains(&level.as_str()) {
                return Err(ConfigError::Validation {
                    path: origin.to_string(),
                    message: format!(
                        "log.level must be one of {}; found \"{level}\"",
                        OK.join(", ")
                    ),
                });
            }
        }
        Ok(())
    }

    /// Effective data directory: config override, else platform default `<data>/machie`.
    pub fn data_dir(&self) -> PathBuf {
        if let Some(dir) = &self.paths.data_dir {
            return PathBuf::from(dir);
        }
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("machie")
    }

    /// Discover and merge all layers. Missing files are simply skipped.
    pub fn discover() -> Result<Self, ConfigError> {
        let mut layers: Vec<PathBuf> = Vec::new();

        if let Ok(env_path) = std::env::var("MACHIE_CONFIG") {
            layers.push(PathBuf::from(env_path));
        }
        if let Some(user) = user_config_path() {
            layers.push(user);
        }
        layers.push(PathBuf::from("machie.toml")); // project-local, highest precedence

        let mut merged = Config::default();
        for path in layers {
            if path.is_file() {
                merged.merge_over(Self::load_file(&path)?);
            }
        }
        Ok(merged)
    }

    /// The default location for `machie init` to write a starter config.
    pub fn default_user_config_path() -> PathBuf {
        user_config_path().unwrap_or_else(|| PathBuf::from("machie.toml"))
    }

    /// Starter template written by `machie init`.
    pub const STARTER_TOML: &'static str = r#"# Machie user configuration
# Layers: <data dir handled automatically>; project-local ./machie.toml overrides this file.
config_version = 1

[paths]
# data_dir = "~/Library/Application Support/machie"   # uncomment to relocate the database

[log]
level = "info"          # off | error | warn | info | debug | trace
"#;
}

fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("machie").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_and_validate() {
        let cfg = Config::from_toml_str("", "test").unwrap();
        assert_eq!(cfg.log.level, None);
        cfg.validate("test").unwrap();
    }

    #[test]
    fn parses_full_starter() {
        let cfg = Config::from_toml_str(Config::STARTER_TOML, "starter").unwrap();
        assert_eq!(cfg.config_version, Some(1));
        assert_eq!(cfg.log.level.as_deref(), Some("info"));
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = Config::from_toml_str("[paths]\ndata_directory = \"x\"\n", "test").unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
    }

    #[test]
    fn rejects_future_config_version() {
        let err = Config::from_toml_str("config_version = 99\n", "test.toml").unwrap_err();
        assert!(matches!(err, ConfigError::Version { found: 99, .. }), "{err}");
    }

    #[test]
    fn rejects_bad_log_level() {
        let err = Config::from_toml_str("[log]\nlevel = \"verbose\"\n", "t").unwrap_err();
        assert!(matches!(err, ConfigError::Validation { .. }), "{err}");
    }

    #[test]
    fn merge_over_prefers_higher_layer() {
        let mut base = Config::from_toml_str("[log]\nlevel=\"info\"\n[paths]\ndata_dir=\"/a\"\n", "b").unwrap();
        let top = Config::from_toml_str("[log]\nlevel=\"debug\"\n", "t").unwrap();
        base.merge_over(top);
        assert_eq!(base.log.level.as_deref(), Some("debug"));
        assert_eq!(base.paths.data_dir.as_deref(), Some("/a"));
    }

    #[test]
    fn loads_from_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machie.toml");
        std::fs::write(&path, "[log]\nlevel = \"warn\"\n").unwrap();
        let cfg = Config::load_file(&path).unwrap();
        assert_eq!(cfg.log.level.as_deref(), Some("warn"));
    }
}
