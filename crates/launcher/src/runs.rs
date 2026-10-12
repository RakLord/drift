//! Training runs on disk (`runs/<name>/`) and whether a trainer is working
//! on them right now.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::paths;
use crate::tracks::TrackChoice;

pub struct Run {
    pub name: String,
    pub dir: PathBuf,
    /// Last generation written to metrics.csv.
    pub generation: Option<u32>,
    pub stage: Option<String>,
    pub best: Option<f64>,
    /// Generation the run trains up to (its saved training.toml).
    pub target: Option<u32>,
    /// What it trains on (its saved training.toml).
    pub tracks: TrackChoice,
    pub last_checkpoint: Option<u32>,
    /// Trainer process working on this run, if any.
    pub pid: Option<u32>,
    pub modified: SystemTime,
}

impl Run {
    pub fn running(&self) -> bool {
        self.pid.is_some()
    }

    pub fn finished(&self) -> bool {
        matches!((self.generation, self.target), (Some(g), Some(t)) if g + 1 >= t)
    }

    pub fn log(&self) -> PathBuf {
        self.dir.join("train.log")
    }

    pub fn status(&self) -> &'static str {
        if self.running() {
            "● training"
        } else if self.finished() {
            "✓ finished"
        } else {
            "○ stopped"
        }
    }
}

/// One line per run for menus and the header.
impl fmt::Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let generation = match (self.generation, self.target) {
            (Some(g), Some(t)) => format!("gen {g}/{t}"),
            (Some(g), None) => format!("gen {g}"),
            _ => "no gens yet".into(),
        };
        let best = self.best.map_or(String::new(), |b| format!("best {b:.0}"));
        write!(
            f,
            "{:<12} {:<11} {:<14} {:<7} {:<10} {}",
            self.name,
            self.status(),
            generation,
            self.stage.as_deref().unwrap_or(""),
            best,
            self.tracks.summary()
        )
    }
}

/// Every run, most recently active first.
pub fn all() -> Vec<Run> {
    let trainers = trainer_processes();
    let Ok(entries) = std::fs::read_dir(paths::root().join("runs")) else { return vec![] };
    let mut runs: Vec<Run> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| load(&e.path(), &trainers))
        .collect();
    runs.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    runs
}

fn load(dir: &Path, trainers: &[(u32, String)]) -> Run {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let config = std::fs::read_to_string(dir.join("config/training.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let tracks = config.get("tracks").and_then(|t| t.as_table());
    let metrics = dir.join("metrics.csv");
    let (generation, stage, best) = last_metrics(&metrics);
    let modified = [metrics.as_path(), dir]
        .iter()
        .filter_map(|p| p.metadata().and_then(|m| m.modified()).ok())
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    Run {
        generation,
        stage,
        best,
        target: config.get("generations").and_then(|g| g.as_integer()).map(|g| g as u32),
        tracks: TrackChoice::from_table(tracks),
        last_checkpoint: last_checkpoint(dir),
        pid: trainers.iter().find(|(_, run)| *run == name).map(|(pid, _)| *pid),
        modified,
        dir: dir.to_path_buf(),
        name,
    }
}

/// Generation, stage and best fitness from the last row of metrics.csv.
fn last_metrics(path: &Path) -> (Option<u32>, Option<String>, Option<f64>) {
    let Ok(text) = std::fs::read_to_string(path) else { return (None, None, None) };
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return (None, None, None) };
    let Some(last) = lines.rev().find(|l| !l.trim().is_empty()) else { return (None, None, None) };
    let column = |name: &str| {
        let i = header.split(',').position(|h| h == name)?;
        last.split(',').nth(i)
    };
    (
        column("generation").and_then(|g| g.parse().ok()),
        column("stage").map(String::from),
        column("best").and_then(|b| b.parse().ok()),
    )
}

/// Newest `checkpoints/gen_NNNNN.pop`.
fn last_checkpoint(dir: &Path) -> Option<u32> {
    std::fs::read_dir(dir.join("checkpoints"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.strip_prefix("gen_")?.strip_suffix(".pop")?.parse().ok())
        .max()
}

/// (pid, run name) for every `train --name <run>` process, however it was
/// started (Linux `/proc`).
fn trainer_processes() -> Vec<(u32, String)> {
    let Ok(entries) = std::fs::read_dir("/proc") else { return vec![] };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let cmdline = std::fs::read(e.path().join("cmdline")).ok()?;
            let args: Vec<String> = cmdline.split(|&b| b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
            if Path::new(args.first()?).file_name()? != "train" {
                return None;
            }
            let name = args.iter().position(|a| a == "--name").and_then(|i| args.get(i + 1))?;
            Some((pid, name.clone()))
        })
        .collect()
}

/// The next free `runN` name.
pub fn next_name(runs: &[Run]) -> String {
    let n = runs.iter().filter_map(|r| r.name.strip_prefix("run")?.parse::<u32>().ok()).max().map_or(1, |n| n + 1);
    format!("run{n}")
}
