//! Saved tracks (`tracks/*.toml`).

use std::fmt;
use std::path::Path;

use crate::paths;

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

/// Every saved track, test_loop first, then by name.
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
    tracks.sort_by_key(|t| (t.stem != "test_loop", t.stem.clone()));
    tracks
}
