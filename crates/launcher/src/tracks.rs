//! Saved tracks (`tracks/*.toml`).

use std::fmt;
use std::path::Path;

use crate::paths;

#[derive(Clone)]
pub struct SavedTrack {
    /// Workspace-relative path, as the trainer and viewer take it.
    pub path: String,
    pub stem: String,
    pub name: String,
    /// Part of the trainer's held-out test set (unless trained on).
    pub test: bool,
}

impl fmt::Display for SavedTrack {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:<14} {}", self.stem, self.name)
    }
}

/// `tracks/generated_1.toml` → `generated_1`.
pub fn stem(path: &str) -> String {
    Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Every saved track, by file name.
pub fn all() -> Vec<SavedTrack> {
    let Ok(entries) = std::fs::read_dir(paths::root().join("tracks")) else { return vec![] };
    let mut tracks: Vec<SavedTrack> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| {
            let table = std::fs::read_to_string(&p).ok()?.parse::<toml::Table>().ok()?;
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            Some(SavedTrack {
                path: format!("tracks/{stem}.toml"),
                name: table.get("name").and_then(|n| n.as_str()).unwrap_or(&stem).to_string(),
                test: table.get("test").and_then(|t| t.as_bool()).unwrap_or(true),
                stem,
            })
        })
        .collect();
    tracks.sort_by_key(|t| t.stem.clone());
    tracks
}

/// What a run trains on: the trainer's `[tracks]` choices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackChoice {
    /// Saved tracks driven every generation.
    pub fixed: Vec<String>,
    /// Saved tracks to draw `pooled` of each generation.
    pub pool: Vec<String>,
    pub pooled: u32,
    /// Fresh generated tracks each generation.
    pub generated: u32,
}

impl TrackChoice {
    /// From a training.toml's `[tracks]` table (missing keys as the trainer
    /// reads them).
    pub fn from_table(tracks: Option<&toml::Table>) -> Self {
        let paths = |key: &str| {
            tracks
                .and_then(|t| t.get(key))
                .and_then(|f| f.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        let int = |key: &str, default: u32| tracks.and_then(|t| t.get(key)).and_then(|v| v.as_integer()).map_or(default, |n| n as u32);
        Self { fixed: paths("fixed"), pool: paths("pool"), pooled: int("pool_per_generation", 0), generated: int("per_generation", 3) }
    }

    /// Pool tracks a generation actually draws.
    pub fn pool_draw(&self) -> usize {
        (self.pooled as usize).min(self.pool.len())
    }

    pub fn uses(&self, path: &str) -> bool {
        self.fixed.iter().chain(&self.pool).any(|p| p == path)
    }

    /// "generated_1 + 2 of 5 pool + 3 generated", "3 generated", ...
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self.fixed.iter().map(|f| stem(f)).collect();
        if self.pool_draw() > 0 {
            parts.push(format!("{} of {} pool", self.pool_draw(), self.pool.len()));
        }
        if self.generated > 0 {
            parts.push(format!("{} generated", self.generated));
        }
        if parts.is_empty() { "?".into() } else { parts.join(" + ") }
    }

    /// As a training config's `[tracks]` table (for the viewer to edit).
    pub fn to_toml(&self) -> String {
        let list = |paths: &[String]| toml::Value::Array(paths.iter().map(|p| toml::Value::String(p.clone())).collect());
        format!(
            "[tracks]\nper_generation = {}\nfixed = {}\npool = {}\npool_per_generation = {}\n",
            self.generated,
            list(&self.fixed),
            list(&self.pool),
            self.pooled
        )
    }

    /// Whether a generation would have anything to drive.
    pub fn has_tracks(&self) -> bool {
        !self.fixed.is_empty() || self.pool_draw() > 0 || self.generated > 0
    }

    /// The trainer flags that set exactly this choice.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![];
        if self.fixed.is_empty() {
            args.push("--no-fixed".into());
        }
        for path in &self.fixed {
            args.extend(["--track".into(), path.clone()]);
        }
        if self.pool.is_empty() {
            args.push("--no-pool".into());
        }
        for path in &self.pool {
            args.extend(["--pool".into(), path.clone()]);
        }
        args.extend(["--pooled".into(), self.pooled.to_string(), "--generated".into(), self.generated.to_string()]);
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_choice_reads_summarises_and_becomes_trainer_flags() {
        let table: toml::Table = toml::from_str(
            "per_generation = 3\nfixed = [\"tracks/a.toml\"]\npool = [\"tracks/b.toml\", \"tracks/c.toml\"]\npool_per_generation = 5\n",
        )
        .unwrap();
        let choice = TrackChoice::from_table(Some(&table));
        assert_eq!(choice.summary(), "a + 2 of 2 pool + 3 generated");
        assert!(choice.uses("tracks/c.toml") && !choice.uses("tracks/d.toml"));
        assert_eq!(
            choice.args().join(" "),
            "--track tracks/a.toml --pool tracks/b.toml --pool tracks/c.toml --pooled 5 --generated 3"
        );
        let table: toml::Table = toml::from_str(&choice.to_toml()).unwrap();
        assert_eq!(TrackChoice::from_table(table.get("tracks").and_then(|t| t.as_table())), choice);
        let none = TrackChoice { generated: 2, ..TrackChoice::default() };
        assert_eq!(none.args().join(" "), "--no-fixed --no-pool --pooled 0 --generated 2");
    }
}
