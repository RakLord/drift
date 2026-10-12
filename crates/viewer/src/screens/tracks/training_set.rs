//! A training set: the `[tracks]` table of a training config. Defaults for
//! new runs (`config/training.toml`), a run's own config (used on resume), or
//! a file the launcher hands over (`--set`). Edits change only `[tracks]`
//! lines, so the file's comments survive.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use common::toml_lines;

use crate::paths::workspace_file;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Defaults,
    Run(String),
    /// A file someone else will read back (the launcher's pick).
    File(PathBuf),
}

/// What a library track does in a training set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Driven every generation.
    Fixed,
    /// Drawn from, a few each generation.
    Pool,
    Off,
}

pub struct TrainingSet {
    pub source: Source,
    pub path: PathBuf,
    pub fixed: Vec<String>,
    pub pool: Vec<String>,
    /// `pool_per_generation`.
    pub pooled: u32,
    /// `per_generation`: fresh generated tracks.
    pub generated: u32,
}

impl Source {
    pub fn path(&self) -> PathBuf {
        match self {
            Self::Defaults => workspace_file("config/training.toml"),
            Self::Run(run) => workspace_file("runs").join(run).join("config/training.toml"),
            Self::File(path) => path.clone(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Defaults => "Defaults (new runs)".into(),
            Self::Run(run) => format!("{run} (used on resume)"),
            Self::File(_) => "This run (from the launcher)".into(),
        }
    }

    pub fn run(&self) -> Option<&str> {
        match self {
            Self::Run(run) => Some(run),
            _ => None,
        }
    }
}

/// Defaults, then every run with a config, newest first.
pub fn sources() -> Vec<Source> {
    let mut runs: Vec<(SystemTime, String)> = std::fs::read_dir(workspace_file("runs"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let config = e.path().join("config/training.toml");
            let modified = std::fs::metadata(e.path()).and_then(|m| m.modified()).ok()?;
            config.exists().then(|| (modified, e.file_name().to_string_lossy().into_owned()))
        })
        .collect();
    runs.sort_by(|a, b| b.cmp(a));
    std::iter::once(Source::Defaults).chain(runs.into_iter().map(|(_, run)| Source::Run(run))).collect()
}

impl TrainingSet {
    pub fn load(source: Source) -> Result<Self, String> {
        let path = source.path();
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { source, path, ..Self::from_table(&table) })
    }

    /// `track` is a workspace-relative path, `tracks/<file>.toml`.
    pub fn role(&self, track: &str) -> Role {
        if self.fixed.iter().any(|p| same_track(p, track)) {
            Role::Fixed
        } else if self.pool.iter().any(|p| same_track(p, track)) {
            Role::Pool
        } else {
            Role::Off
        }
    }

    pub fn trained_on(&self, track: &str) -> bool {
        self.role(track) != Role::Off
    }

    /// Pool tracks a generation actually draws.
    pub fn pool_draw(&self) -> u32 {
        self.pooled.min(self.pool.len() as u32)
    }

    /// Give `track` a role, and save. A track is never both fixed and pool.
    pub fn set_role(&mut self, track: &str, role: Role) -> Result<(), String> {
        let (fixed, pool) = (self.fixed.clone(), self.pool.clone());
        self.fixed.retain(|p| !same_track(p, track));
        self.pool.retain(|p| !same_track(p, track));
        match role {
            Role::Fixed => self.fixed.push(track.to_owned()),
            Role::Pool => self.pool.push(track.to_owned()),
            Role::Off => {}
        }
        // New pool, nothing drawn yet: draw one, so the pool is used.
        if role == Role::Pool && self.pooled == 0 {
            self.pooled = 1;
        }
        let result = self.save();
        if result.is_err() {
            (self.fixed, self.pool) = (fixed, pool);
        }
        result
    }

    pub fn set_pooled(&mut self, pooled: u32) -> Result<(), String> {
        let before = std::mem::replace(&mut self.pooled, pooled);
        self.save().inspect_err(|_| self.pooled = before)
    }

    pub fn set_generated(&mut self, generated: u32) -> Result<(), String> {
        let before = std::mem::replace(&mut self.generated, generated);
        self.save().inspect_err(|_| self.generated = before)
    }

    /// Whether a generation would have anything to drive.
    pub fn has_tracks(&self) -> bool {
        !self.fixed.is_empty() || self.pool_draw() > 0 || self.generated > 0
    }

    /// "1 fixed · 2 of 5 pool · 3 generated per generation".
    pub fn summary(&self) -> String {
        let mut parts = vec![];
        if !self.fixed.is_empty() {
            parts.push(format!("{} fixed", self.fixed.len()));
        }
        if !self.pool.is_empty() {
            parts.push(format!("{} of {} pool", self.pool_draw(), self.pool.len()));
        }
        if self.generated > 0 {
            parts.push(format!("{} generated", self.generated));
        }
        if parts.is_empty() { "nothing".into() } else { parts.join(" · ") + " per generation" }
    }

    fn save(&self) -> Result<(), String> {
        let text = std::fs::read_to_string(&self.path).map_err(|e| format!("{}: {e}", self.path.display()))?;
        let updated = edit(&text, self);
        // Never write something that doesn't read back as intended.
        let table: toml::Table =
            updated.parse().map_err(|e| format!("edit would break {}: {e}", self.path.display()))?;
        let reread = TrainingSet::from_table(&table);
        if (reread.fixed, reread.pool, reread.pooled, reread.generated)
            != (self.fixed.clone(), self.pool.clone(), self.pooled, self.generated)
        {
            return Err(format!("edit didn't read back as intended; {} left as it was", self.path.display()));
        }
        std::fs::write(&self.path, updated).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// The `[tracks]` values of a parsed config, missing keys as the
    /// trainer reads them.
    fn from_table(table: &toml::Table) -> Self {
        let tracks = table.get("tracks").and_then(|t| t.as_table());
        let paths = |key: &str| {
            tracks
                .and_then(|t| t.get(key))
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        let int = |key: &str, default: u32| {
            tracks.and_then(|t| t.get(key)).and_then(|v| v.as_integer()).map_or(default, |n| n.max(0) as u32)
        };
        Self {
            source: Source::Defaults,
            path: PathBuf::new(),
            fixed: paths("fixed"),
            pool: paths("pool"),
            pooled: int("pool_per_generation", 0),
            generated: int("per_generation", 3),
        }
    }
}

/// `text` with its `[tracks]` lines set to `set`'s values.
fn edit(text: &str, set: &TrainingSet) -> String {
    let text = toml_lines::set_in_table(text, "tracks", "per_generation", &set.generated.to_string());
    let text = toml_lines::set_in_table(&text, "tracks", "fixed", &toml_lines::string_array(&set.fixed));
    let text = toml_lines::set_in_table(&text, "tracks", "pool", &toml_lines::string_array(&set.pool));
    toml_lines::set_in_table(&text, "tracks", "pool_per_generation", &set.pooled.to_string())
}

/// Whether two config paths name the same track file (`tracks/a.toml` and
/// `./tracks/a.toml` do).
fn same_track(a: &str, b: &str) -> bool {
    let name = |p: &str| Path::new(p).file_name().map(|f| f.to_owned());
    a == b || (name(a).is_some() && name(a) == name(b) && Path::new(a).parent().map(clean) == Path::new(b).parent().map(clean))
}

fn clean(dir: &Path) -> PathBuf {
    dir.components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: &str = "seed = 1\n\n[tracks]\nper_generation = 3        # fresh generated tracks\nfixed = [\"tracks/generated_1.toml\"]   # every generation\npool = []                 # drawn from\npool_per_generation = 0   # drawn each generation\ntest_every = 10\n\n[checkpoints]\nevery = 10\n";

    fn set(text: &str) -> TrainingSet {
        TrainingSet::from_table(&text.parse().unwrap())
    }

    #[test]
    fn moving_a_track_between_roles_edits_only_tracks_lines() {
        let mut s = set(DEFAULTS);
        assert_eq!(s.role("tracks/generated_1.toml"), Role::Fixed);
        s.fixed.retain(|p| p != "tracks/generated_1.toml");
        s.pool = vec!["tracks/generated_1.toml".into(), "tracks/generated_9.toml".into()];
        s.pooled = 1;
        let out = edit(DEFAULTS, &s);
        assert!(out.contains("fixed = []   # every generation"));
        assert!(out.contains("pool = [\"tracks/generated_1.toml\", \"tracks/generated_9.toml\"]                 # drawn from"));
        assert!(out.contains("pool_per_generation = 1   # drawn each generation"));
        assert!(out.contains("test_every = 10\n\n[checkpoints]\nevery = 10"));
        let again = set(&out);
        assert_eq!(again.role("./tracks/generated_9.toml"), Role::Pool);
        assert_eq!(again.role("tracks/generated_1.toml"), Role::Pool);
        assert_eq!(again.role("tracks/other.toml"), Role::Off);
    }

    #[test]
    fn a_run_snapshot_without_pool_keys_gets_them() {
        let snapshot = "seed = 1\n\n[tracks]\nper_generation = 3\nfixed = [\"tracks/generated_1.toml\"]\ntest_every = 10\n\n[checkpoints]\nevery = 10\n";
        let mut s = set(snapshot);
        s.pool = vec!["tracks/a.toml".into()];
        s.pooled = 1;
        let out = edit(snapshot, &s);
        let table: toml::Table = out.parse().unwrap();
        assert_eq!(table["tracks"]["pool_per_generation"].as_integer(), Some(1));
        assert_eq!(table["tracks"]["test_every"].as_integer(), Some(10));
        assert!(table["checkpoints"].get("pool").is_none());
    }

    #[test]
    fn roles_save_to_the_file_and_read_back() {
        let path = std::env::temp_dir().join(format!("drift-training-set-{}.toml", std::process::id()));
        std::fs::write(&path, DEFAULTS).unwrap();
        let mut s = TrainingSet::load(Source::File(path.clone())).unwrap();
        s.set_role("tracks/generated_9.toml", Role::Pool).unwrap();
        s.set_role("tracks/generated_1.toml", Role::Pool).unwrap();
        s.set_generated(0).unwrap();
        let back = TrainingSet::load(Source::File(path.clone())).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(back.fixed.is_empty());
        assert_eq!(back.pool, ["tracks/generated_9.toml", "tracks/generated_1.toml"]);
        assert_eq!((back.pooled, back.generated), (1, 0), "a new pool draws one");
        assert!(text.contains("per_generation = 0        # fresh generated tracks"));
        assert!(text.ends_with("[checkpoints]\nevery = 10\n"));
    }
}
