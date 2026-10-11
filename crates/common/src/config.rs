//! Loading TOML config files, plus a cheap hot-reload watcher for the viewer.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::de::DeserializeOwned;

#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: std::io::Error },
    Parse { path: PathBuf, source: toml::de::Error },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "reading {}: {source}", path.display()),
            Self::Parse { path, source } => write!(f, "parsing {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn load<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, ConfigError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .map_err(|source| ConfigError::Io { path: path.to_owned(), source })?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse { path: path.to_owned(), source })
}

/// Holds a config value and reloads it when the file's modification time changes.
///
/// A failed reload keeps the previous value, so a half-saved file in an editor
/// never crashes the viewer.
pub struct WatchedConfig<T> {
    path: PathBuf,
    modified: Option<SystemTime>,
    value: T,
}

impl<T: DeserializeOwned> WatchedConfig<T> {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        let value = load(&path)?;
        let modified = modified_time(&path);
        Ok(Self { path, modified, value })
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    /// Check the file and reload if it changed. Returns `Some(Ok(()))` on a
    /// successful reload, `Some(Err(_))` if the new contents were rejected, and
    /// `None` if nothing changed.
    pub fn poll(&mut self) -> Option<Result<(), ConfigError>> {
        let modified = modified_time(&self.path);
        if modified == self.modified {
            return None;
        }
        self.modified = modified;
        Some(load(&self.path).map(|value| self.value = value))
    }
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Example {
        mass: f64,
        name: String,
    }

    #[test]
    fn load_reports_parse_errors_with_path() {
        let dir = std::env::temp_dir().join(format!("drift-config-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.toml");
        let bad = dir.join("bad.toml");
        fs::write(&good, "mass = 1250.0\nname = \"s15\"\n").unwrap();
        fs::write(&bad, "mass = \"heavy\"\n").unwrap();

        let ok: Example = load(&good).unwrap();
        assert_eq!(ok, Example { mass: 1250.0, name: "s15".into() });

        let err = load::<Example>(&bad).unwrap_err();
        assert!(err.to_string().contains("bad.toml"));

        fs::remove_dir_all(&dir).ok();
    }
}
