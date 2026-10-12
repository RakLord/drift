//! Line-level edits to TOML files people also edit by hand.
//!
//! Each edit changes only the `key = value` it needs (a multi-line array
//! counts as one value), so the file's comments, order and layout survive.
//! Callers should re-parse the result before writing it.

/// Set a top-level `key = value` (above the first `[table]` header).
/// Replaces the key if it's there, otherwise adds it after `after` (if that
/// key exists) or at the end of the top-level keys.
pub fn set_top_level(text: &str, key: &str, value: &str, after: Option<&str>) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let end = lines.iter().position(|l| is_header(l)).unwrap_or(lines.len());
    set_in(&mut lines, 0..end, key, value, after);
    lines.join("\n") + "\n"
}

/// Set `key = value` inside `[table]`, adding the key at the end of the
/// table (before any blank lines that separate it from the next), or the
/// table at the end of the file.
pub fn set_in_table(text: &str, table: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let header = format!("[{table}]");
    match lines.iter().position(|l| l.trim() == header || l.trim_start().starts_with(&format!("{header} "))) {
        Some(start) => {
            let end = lines[start + 1..].iter().position(|l| is_header(l)).map_or(lines.len(), |i| start + 1 + i);
            set_in(&mut lines, start + 1..end, key, value, None);
        }
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(header);
            lines.push(format!("{key} = {value}"));
        }
    }
    lines.join("\n") + "\n"
}

/// A TOML array of strings, on one line.
pub fn string_array(items: &[String]) -> String {
    toml::Value::Array(items.iter().map(|s| toml::Value::String(s.clone())).collect()).to_string()
}

fn is_header(line: &str) -> bool {
    line.trim_start().starts_with('[')
}

/// Whether `line` assigns `key`.
fn assigns(line: &str, key: &str) -> bool {
    line.trim_start().strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// Replace or add `key` among `lines[range]`.
fn set_in(lines: &mut Vec<String>, range: std::ops::Range<usize>, key: &str, value: &str, after: Option<&str>) {
    let new_line = format!("{key} = {value}");
    if let Some(i) = range.clone().find(|&i| assigns(&lines[i], key)) {
        let last = value_end(lines, i);
        // Keep a trailing comment (and the spacing before it), if any.
        let comment = trailing_comment(&lines[last]).map(|c| lines[last][c..].to_owned()).unwrap_or_default();
        lines.splice(i..=last, [format!("{new_line}{comment}")]);
        return;
    }
    let anchor = after.and_then(|a| range.clone().find(|&i| assigns(&lines[i], a)));
    let at = match anchor {
        Some(i) => value_end(lines, i) + 1,
        // After the last non-blank line of the range.
        None => range.clone().rev().find(|&i| !lines[i].trim().is_empty()).map_or(range.start, |i| i + 1),
    };
    lines.insert(at, new_line);
}

/// The last line of the value assigned on `lines[start]`: the same line,
/// unless an array or inline table stays open past it.
fn value_end(lines: &[String], start: usize) -> usize {
    let mut depth = 0i32;
    for (i, line) in lines.iter().enumerate().skip(start) {
        let code = &line[..trailing_comment(line).unwrap_or(line.len())];
        let mut quote = None;
        let mut escaped = false;
        for c in code.chars() {
            match quote {
                Some('"') if escaped => escaped = false,
                Some('"') if c == '\\' => escaped = true,
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == '[' || c == '{' => depth += 1,
                None if c == ']' || c == '}' => depth -= 1,
                None => {}
            }
        }
        if depth <= 0 {
            return i;
        }
    }
    lines.len() - 1
}

/// Where a line's trailing comment starts, including the whitespace before
/// it. A `#` inside a quoted string isn't a comment.
pub fn trailing_comment(line: &str) -> Option<usize> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK: &str = "# A comment that must survive.\n\nversion = 1\nname = \"Old\"   # display name\nwidth = 12.0\nverge = 8.0\n\n[[anchors]]\npos = [0.0, 0.0]\nleft = \"soft\"\nright = \"soft\"\nname = \"not top level\"\n";

    #[test]
    fn edits_only_the_top_level_line_and_keeps_comments() {
        let renamed = set_top_level(TRACK, "name", "\"New\"", None);
        assert!(renamed.contains("name = \"New\"   # display name"));
        assert!(renamed.contains("# A comment that must survive."));
        assert!(renamed.contains("name = \"not top level\""), "tables untouched");

        let flagged = set_top_level(TRACK, "test", "false", Some("name"));
        let name_line = flagged.lines().position(|l| l.starts_with("name")).unwrap();
        assert_eq!(flagged.lines().nth(name_line + 1), Some("test = false"));
        let again = set_top_level(&flagged, "test", "true", Some("name"));
        assert_eq!(again.matches("test =").count(), 1);
    }

    #[test]
    fn a_hash_in_a_name_is_not_a_comment() {
        assert_eq!(trailing_comment("name = \"Track #2\""), None);
        assert_eq!(trailing_comment("name = \"Track #2\"  # note"), Some(17));
        assert_eq!(trailing_comment("name = 'say \"hi\" #'"), None);
        let renamed = set_top_level("name = \"Track #2\"\n", "name", "\"Three\"", None);
        assert_eq!(renamed, "name = \"Three\"\n");
    }

    const TRAINING: &str = "seed = 1\n\n[episode]\nfixed = 9   # not this one\n\n[tracks]\nper_generation = 3        # fresh tracks\nfixed = [\"tracks/a.toml\"]   # every generation\ntest_every = 10\n\n[checkpoints]\nevery = 10\n";

    #[test]
    fn edits_a_key_in_its_table_only() {
        let out = set_in_table(TRAINING, "tracks", "fixed", &string_array(&["tracks/b.toml".into(), "tracks/c.toml".into()]));
        assert!(out.contains("fixed = [\"tracks/b.toml\", \"tracks/c.toml\"]   # every generation"));
        assert!(out.contains("fixed = 9   # not this one"));
        assert!(out.contains("per_generation = 3        # fresh tracks"));
        let table: toml::Table = out.parse().unwrap();
        assert_eq!(table["tracks"]["fixed"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn adds_a_missing_key_at_the_end_of_its_table() {
        let out = set_in_table(TRAINING, "tracks", "pool_per_generation", "2");
        let lines: Vec<&str> = out.lines().collect();
        let at = lines.iter().position(|l| *l == "pool_per_generation = 2").unwrap();
        assert_eq!(lines[at - 1], "test_every = 10");
        assert_eq!(lines[at + 1], "", "the blank line before [checkpoints] stays");
        let table: toml::Table = out.parse().unwrap();
        assert_eq!(table["tracks"]["pool_per_generation"].as_integer(), Some(2));
        assert!(table["checkpoints"].get("pool_per_generation").is_none());
    }

    #[test]
    fn replaces_a_multi_line_array() {
        let text = "[tracks]\npool = [\n    \"tracks/a.toml\",  # first\n    \"tracks/b.toml\",\n]   # the pool\ntest_every = 10\n";
        let out = set_in_table(text, "tracks", "pool", &string_array(&["tracks/c.toml".into()]));
        assert_eq!(out, "[tracks]\npool = [\"tracks/c.toml\"]   # the pool\ntest_every = 10\n");
    }

    #[test]
    fn adds_a_missing_table() {
        let out = set_in_table("seed = 1\n", "tracks", "pool", "[]");
        assert_eq!(out, "seed = 1\n\n[tracks]\npool = []\n");
        let table: toml::Table = out.parse().unwrap();
        assert!(table["tracks"]["pool"].as_array().unwrap().is_empty());
    }
}
