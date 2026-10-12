//! The track library: every `tracks/*.toml`, and small edits to them.
//!
//! Edits change only the line they need (`name`, `test`), so the file's
//! comments and layout survive. Deleting moves a file into
//! `tracks/deleted/`, which nothing reads, so a mistake can be undone.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::toml_lines;
use sim::track::{Track, TrackDef};

use crate::paths::workspace_file;

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub test: bool,
    /// The built track, or why it doesn't build.
    pub track: Result<Arc<Track>, String>,
}

impl Entry {
    /// `tracks/<file>.toml`, as training configs name it.
    pub fn config_path(&self) -> String {
        format!("tracks/{}", self.file_name())
    }

    pub fn file_name(&self) -> String {
        self.path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned())
    }
}

/// Every track in the library, sorted by file name.
pub fn scan() -> Vec<Entry> {
    let Ok(dir) = std::fs::read_dir(workspace_file("tracks")) else { return vec![] };
    let mut paths: Vec<PathBuf> =
        dir.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| match TrackDef::load(&path) {
            Ok(def) => Entry {
                name: def.name.clone(),
                test: def.test,
                track: Track::build(def).map(Arc::new).map_err(|e| e.to_string()),
                path,
            },
            Err(err) => Entry { name: "(unreadable)".into(), test: false, track: Err(err.to_string()), path },
        })
        .collect()
}

/// Whether two paths name the same file.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn edit(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let updated = toml_lines::set_top_level(&text, key, value, Some("name"));
    // Never write something that no longer parses.
    toml::from_str::<TrackDef>(&updated).map_err(|e| format!("edit would break {}: {e}", path.display()))?;
    std::fs::write(path, updated).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn set_test(path: &Path, test: bool) -> Result<(), String> {
    edit(path, "test", if test { "true" } else { "false" })
}

pub fn rename(path: &Path, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a track needs a name".into());
    }
    edit(path, "name", &toml::Value::String(name.to_owned()).to_string())
}

/// Move a track into `tracks/deleted/`. Returns where it went.
pub fn delete(path: &Path) -> Result<PathBuf, String> {
    let bin = workspace_file("tracks").join("deleted");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let target = bin.join(path.file_name().ok_or("not a file")?);
    std::fs::rename(path, &target).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(target)
}

/// Save a generated track into the library folder.
pub fn keep(def: &TrackDef, seed: u64) -> Result<PathBuf, String> {
    let path = workspace_file("tracks").join(format!("generated_{seed}.toml"));
    let header = format!("# Generated from seed {seed}, kept from the viewer.\n\n");
    let body = toml::to_string(def).map_err(|e| e.to_string())?;
    std::fs::write(&path, header + &body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
