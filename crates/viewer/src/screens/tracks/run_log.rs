//! Reading a run's `runs/<run>/tracks.csv`, shared by the "this generation"
//! strip and the Results tab: its rows, a change watch for following a live
//! run, and rebuilding the tracks it names (generated ones from their seed
//! with the run's own generator settings).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use sim::track::generator::{self, GeneratorParams};
use sim::track::{Track, TrackDef};

use crate::paths::workspace_file;

const POLL_SECS: f64 = 1.0;

/// The run's directory.
pub fn run_dir(run: &str) -> PathBuf {
    workspace_file("runs").join(run)
}

/// The run's snapshot `generator.toml`, falling back to the defaults.
pub fn generator(run: &str) -> GeneratorParams {
    common::config::load(run_dir(run).join("config/generator.toml"))
        .or_else(|_| common::config::load(workspace_file("config/generator.toml")))
        .unwrap_or_default()
}

/// Polls a file's modified time, once a second.
pub struct Watch {
    path: PathBuf,
    modified: Option<SystemTime>,
    since_poll: f64,
}

impl Watch {
    pub fn new(path: PathBuf) -> Self {
        Self { path, modified: None, since_poll: 0.0 }
    }

    /// The file's text, noting its modified time.
    pub fn read(&mut self) -> String {
        self.modified = modified(&self.path);
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    /// Whether the file changed since the last `read`.
    pub fn changed(&mut self, dt: f64) -> bool {
        self.since_poll += dt;
        if self.since_poll < POLL_SECS {
            return false;
        }
        self.since_poll = 0.0;
        modified(&self.path) != self.modified
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// One run of the saved driver.
#[derive(Clone, Debug)]
pub struct Row {
    pub generation: u32,
    /// `tracks/<file>.toml`, or `gen:<seed>`.
    pub id: String,
    /// fixed / pool / generated / check / test.
    pub role: String,
    pub progress: f64,
    pub laps: f64,
    pub end: String,
}

impl Row {
    pub fn generated(&self) -> bool {
        self.id.starts_with("gen:")
    }
}

/// `generation,track,role,length,progress,laps,time,end` rows; a half-written
/// last line is skipped.
pub fn parse(text: &str) -> Vec<Row> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() != 8 {
                return None;
            }
            Some(Row {
                generation: f[0].parse().ok()?,
                id: f[1].to_owned(),
                role: f[2].to_owned(),
                progress: f[4].parse().ok()?,
                laps: f[5].parse().ok()?,
                end: f[7].to_owned(),
            })
        })
        .collect()
}

/// Build the track a row names.
pub fn build(id: &str, generator: &GeneratorParams) -> Result<Arc<Track>, String> {
    let def = match id.strip_prefix("gen:") {
        Some(seed) => {
            let seed: u64 = seed.parse().map_err(|_| format!("bad seed in {id}"))?;
            generator::generate(generator, seed).map_err(|f| f.lines(1).join("; "))?
        }
        None => TrackDef::load(workspace_file(id)).map_err(|e| e.to_string())?,
    };
    Track::build(def).map(Arc::new).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_rows_and_skips_a_half_written_one() {
        let text = "generation,track,role,length,progress,laps,time,end\n3,gen:42,generated,900,90.0,0.100,5.00,crashed\n3,tracks/a.toml,fix";
        let rows = parse(text);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].generation, rows[0].id.as_str(), rows[0].end.as_str()), (3, "gen:42", "crashed"));
    }
}
