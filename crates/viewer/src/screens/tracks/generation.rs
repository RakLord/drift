//! What a run's saved driver drove in its newest generation, from
//! `runs/<run>/tracks.csv`. Generated tracks are rebuilt from their seed with
//! the run's own generator settings. Polled, so it follows a live run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use sim::track::generator::{self, GeneratorParams};
use sim::track::{Track, TrackDef, TrackStats};

use crate::paths::workspace_file;
use crate::render::thumb::Thumb;

const POLL_SECS: f64 = 1.0;

/// One run the saved driver did this generation.
pub struct Drawn {
    /// `tracks/<file>.toml`, or `gen:<seed>`.
    pub id: String,
    /// fixed / pool / generated / check.
    pub role: String,
    pub progress: f64,
    pub laps: f64,
    pub end: String,
    pub track: Result<Arc<Track>, String>,
    pub thumb: Option<Thumb>,
    pub stats: Option<TrackStats>,
}

impl Drawn {
    /// The seed of a generated track.
    pub fn seed(&self) -> Option<u64> {
        self.id.strip_prefix("gen:")?.parse().ok()
    }

    pub fn name(&self) -> String {
        match (&self.track, self.seed()) {
            (Ok(track), None) => track.def.name.clone(),
            (_, Some(seed)) => format!("Generated {seed}"),
            (Err(_), None) => self.id.clone(),
        }
    }
}

pub struct LastGeneration {
    pub run: String,
    csv: PathBuf,
    modified: Option<SystemTime>,
    since_poll: f64,
    pub generation: Option<u32>,
    pub drawn: Vec<Drawn>,
    generator: GeneratorParams,
    /// Built tracks by id, so polling doesn't rebuild them.
    cache: HashMap<String, Result<Arc<Track>, String>>,
}

impl LastGeneration {
    pub fn new(run: &str) -> Self {
        let dir = workspace_file("runs").join(run);
        let generator = common::config::load(dir.join("config/generator.toml"))
            .or_else(|_| common::config::load(workspace_file("config/generator.toml")))
            .unwrap_or_default();
        let mut last = Self {
            run: run.to_owned(),
            csv: dir.join("tracks.csv"),
            modified: None,
            since_poll: 0.0,
            generation: None,
            drawn: vec![],
            generator,
            cache: HashMap::new(),
        };
        last.reload();
        last
    }

    /// Re-read if the file changed. Returns true when it did.
    pub fn poll(&mut self, dt: f64) -> bool {
        self.since_poll += dt;
        if self.since_poll < POLL_SECS {
            return false;
        }
        self.since_poll = 0.0;
        let modified = std::fs::metadata(&self.csv).and_then(|m| m.modified()).ok();
        if modified == self.modified {
            return false;
        }
        self.reload();
        true
    }

    /// The row for a library track this generation, if it was driven.
    pub fn row(&self, id: &str) -> Option<&Drawn> {
        self.drawn.iter().find(|d| d.id == id)
    }

    fn reload(&mut self) {
        self.modified = std::fs::metadata(&self.csv).and_then(|m| m.modified()).ok();
        let text = std::fs::read_to_string(&self.csv).unwrap_or_default();
        let rows = parse(&text);
        let Some(generation) = rows.iter().map(|r| r.generation).max() else {
            (self.generation, self.drawn) = (None, vec![]);
            return;
        };
        let newest: Vec<Row> = rows.into_iter().filter(|r| r.generation == generation && r.role != "test").collect();
        // Drop tracks no longer drawn; keep the rest built.
        self.cache.retain(|id, _| newest.iter().any(|r| r.id == *id));
        self.drawn = newest
            .into_iter()
            .map(|r| {
                let track = self.cache.entry(r.id.clone()).or_insert_with(|| build(&r.id, &self.generator)).clone();
                Drawn {
                    thumb: track.as_ref().ok().map(|t| Thumb::new(t)),
                    stats: track.as_ref().ok().map(|t| TrackStats::of(t)),
                    track,
                    id: r.id,
                    role: r.role,
                    progress: r.progress,
                    laps: r.laps,
                    end: r.end,
                }
            })
            .collect();
        self.generation = Some(generation);
    }
}

struct Row {
    generation: u32,
    id: String,
    role: String,
    progress: f64,
    laps: f64,
    end: String,
}

/// `generation,track,role,length,progress,laps,time,end` rows; a half-written
/// last line is skipped.
fn parse(text: &str) -> Vec<Row> {
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

fn build(id: &str, generator: &GeneratorParams) -> Result<Arc<Track>, String> {
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
