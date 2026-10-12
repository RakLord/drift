//! The track library: every `tracks/*.toml`, and small edits to them.
//!
//! Edits change only the line they need (`name`, `test`), so the file's
//! comments and layout survive. Deleting moves a file into
//! `tracks/deleted/`, which nothing reads, so a mistake can be undone.

use std::path::{Path, PathBuf};

use sim::track::{Track, TrackDef};

use crate::paths::workspace_file;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub test: bool,
    /// Length in metres, or why the track doesn't build.
    pub length: Result<f64, String>,
}

impl Entry {
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
                length: Track::build(def).map(|t| t.length).map_err(|e| e.to_string()),
                path,
            },
            Err(err) => Entry { name: "(unreadable)".into(), test: false, length: Err(err.to_string()), path },
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

/// Set a top-level `key = value` in a track file, keeping everything else.
/// Replaces the line if the key is there, otherwise adds it after `name`.
fn set_top_level(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    // Top-level keys come before the first [table] / [[array]] header.
    let top_end = lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(lines.len());
    let is_key = |l: &str| {
        let t = l.trim_start();
        t.strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    let new_line = format!("{key} = {value}");
    if let Some(i) = lines[..top_end].iter().position(|l| is_key(l)) {
        // Keep a trailing comment (and the spacing before it), if any.
        let comment = trailing_comment(&lines[i]).map(|c| lines[i][c..].to_owned()).unwrap_or_default();
        lines[i] = format!("{new_line}{comment}");
    } else {
        let after = lines[..top_end].iter().position(|l| {
            let t = l.trim_start();
            t.starts_with("name") && t[4..].trim_start().starts_with('=')
        });
        lines.insert(after.map_or(top_end, |i| i + 1), new_line);
    }
    lines.join("\n") + "\n"
}

/// Where a line's trailing comment starts, including the whitespace before
/// it. A `#` inside a quoted string isn't a comment.
fn trailing_comment(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return Some(line[..i].trim_end().len()),
            None => {}
        }
    }
    None
}

fn edit(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let updated = set_top_level(&text, key, value);
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

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK: &str = "# A comment that must survive.\n\nversion = 1\nname = \"Old\"   # display name\nwidth = 12.0\nverge = 8.0\n\n[[anchors]]\npos = [0.0, 0.0]\nleft = \"soft\"\nright = \"soft\"\nname = \"not top level\"\n";

    #[test]
    fn edits_only_the_top_level_line_and_keeps_comments() {
        let renamed = set_top_level(TRACK, "name", "\"New\"");
        assert!(renamed.contains("name = \"New\"   # display name"));
        assert!(renamed.contains("# A comment that must survive."));
        assert!(renamed.contains("name = \"not top level\""), "tables untouched");

        let flagged = set_top_level(TRACK, "test", "false");
        let name_line = flagged.lines().position(|l| l.starts_with("name")).unwrap();
        assert_eq!(flagged.lines().nth(name_line + 1), Some("test = false"));
        let again = set_top_level(&flagged, "test", "true");
        assert_eq!(again.matches("test =").count(), 1);
    }

    #[test]
    fn a_hash_in_a_name_is_not_a_comment() {
        assert_eq!(trailing_comment("name = \"Track #2\""), None);
        assert_eq!(trailing_comment("name = \"Track #2\"  # note"), Some(17));
        assert_eq!(trailing_comment("name = 'say \"hi\" #'"), None);
        let renamed = set_top_level("name = \"Track #2\"\n", "name", "\"Three\"");
        assert_eq!(renamed, "name = \"Three\"\n");
    }
}
