//! Human picks: drivers chosen in the viewer's review grid, sent to the
//! run's trainer to be boosted (`[review]` in training.toml).
//!
//! Each send is one file in `runs/<run>/picks/inbox/`, in the top.genomes
//! format (the entries plus their genomes, so a pick still works when its
//! genome has been bred out by the time the trainer reads it). The trainer
//! reads the inbox at the end of each generation and moves each file to
//! `picks/applied/g<generation>_<name>`, the record of what was boosted
//! when. Files it can't read go to `picks/rejected/`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::top_file::{self, TopFileError, TopGenomes};

const EXTENSION: &str = "genomes";

pub fn inbox(run_dir: &Path) -> PathBuf {
    run_dir.join("picks").join("inbox")
}

pub fn applied(run_dir: &Path) -> PathBuf {
    run_dir.join("picks").join("applied")
}

pub fn rejected(run_dir: &Path) -> PathBuf {
    run_dir.join("picks").join("rejected")
}

/// `top` cut down to the entries (and genomes) with these ids, in `ids`
/// order. Ids it doesn't hold are skipped.
pub fn subset(top: &TopGenomes, ids: &[u64]) -> TopGenomes {
    let mut out = TopGenomes { header: top.header.clone(), genomes: vec![] };
    out.header.entries.clear();
    for &id in ids {
        if let Some(i) = top.header.entries.iter().position(|e| e.id == id) {
            out.header.entries.push(top.header.entries[i].clone());
            out.genomes.push(top.genomes[i].clone());
        }
    }
    out
}

/// Drop `picks` into the run's inbox. Returns the file's name, to follow
/// with [`state`].
pub fn send(run_dir: &Path, picks: &TopGenomes) -> io::Result<String> {
    let dir = inbox(run_dir);
    std::fs::create_dir_all(&dir)?;
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    // Names sort in send order; two sends in the same millisecond still get
    // their own file.
    let name = (0..)
        .map(|n: u32| format!("{ms:013}-{n:03}.{EXTENSION}"))
        .find(|name| !dir.join(name).exists())
        .expect("a free name");
    top_file::save(dir.join(&name), picks)?;
    Ok(name)
}

/// Every file waiting in the inbox, oldest first, with what it holds.
pub fn waiting(run_dir: &Path) -> Vec<(PathBuf, Result<TopGenomes, TopFileError>)> {
    let Ok(dir) = std::fs::read_dir(inbox(run_dir)) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> =
        dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == EXTENSION)).collect();
    files.sort();
    files.into_iter().map(|p| (p.clone(), top_file::load(&p))).collect()
}

/// Move an inbox file to `applied/`, tagged with the generation it was
/// applied in.
pub fn mark_applied(run_dir: &Path, file: &Path, generation: u32) -> io::Result<()> {
    move_into(&applied(run_dir), file, &format!("g{generation:05}_"))
}

/// Move an unreadable inbox file out of the way.
pub fn mark_rejected(run_dir: &Path, file: &Path) -> io::Result<()> {
    move_into(&rejected(run_dir), file, "")
}

fn move_into(dir: &Path, file: &Path, prefix: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let name = file.file_name().ok_or_else(|| io::Error::other("no file name"))?.to_string_lossy();
    std::fs::rename(file, dir.join(format!("{prefix}{name}")))
}

/// Where a sent file is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Waiting,
    /// Applied in this generation.
    Applied(u32),
    Rejected,
    /// Not found anywhere (the run folder changed).
    Gone,
}

/// The state of the file `send` named `name`.
pub fn state(run_dir: &Path, name: &str) -> State {
    if inbox(run_dir).join(name).exists() {
        return State::Waiting;
    }
    let suffix = format!("_{name}");
    let applied_in = std::fs::read_dir(applied(run_dir)).into_iter().flatten().flatten().find_map(|e| {
        let file = e.file_name().to_string_lossy().into_owned();
        file.strip_suffix(&suffix)?.strip_prefix('g')?.parse().ok()
    });
    match applied_in {
        Some(generation) => State::Applied(generation),
        None if rejected(run_dir).join(name).exists() => State::Rejected,
        None => State::Gone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::top_file::tests::top;

    #[test]
    fn a_send_waits_then_moves_to_applied() {
        let run = std::env::temp_dir().join(format!("drift-picks-{}", std::process::id()));
        std::fs::remove_dir_all(&run).ok();
        let all = top();
        let picks = subset(&all, &[102, 100, 555]);
        assert_eq!(picks.header.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [102, 100]);
        assert_eq!(picks.genomes, [all.genomes[2].clone(), all.genomes[0].clone()]);

        let first = send(&run, &picks).unwrap();
        let second = send(&run, &subset(&all, &[101])).unwrap();
        assert_ne!(first, second);
        assert_eq!(state(&run, &first), State::Waiting);

        let waiting = waiting(&run);
        assert_eq!(waiting.len(), 2, "no temporary files listed");
        let (path, read) = &waiting[0];
        assert_eq!(read.as_ref().unwrap().header.entries, picks.header.entries);
        mark_applied(&run, path, 42).unwrap();
        assert_eq!(state(&run, &first), State::Applied(42));
        mark_rejected(&run, &waiting[1].0).unwrap();
        assert_eq!(state(&run, &second), State::Rejected);
        assert!(super::waiting(&run).is_empty());
        assert_eq!(state(&run, "nope.genomes"), State::Gone);
        std::fs::remove_dir_all(&run).ok();
    }
}
