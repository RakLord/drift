//! What a run's saved driver drove in its newest generation, from
//! `runs/<run>/tracks.csv`. Generated tracks are rebuilt from their seed with
//! the run's own generator settings. Polled, so it follows a live run.

use std::collections::HashMap;
use std::sync::Arc;

use sim::track::generator::GeneratorParams;
use sim::track::{Track, TrackStats};

use super::run_log::{self, Row, Watch};
use crate::render::thumb::Thumb;

/// One run of the saved driver, with its track built.
pub struct Drawn {
    pub generation: u32,
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
    pub fn new(row: Row, track: Result<Arc<Track>, String>) -> Self {
        Self {
            thumb: track.as_ref().ok().map(|t| Thumb::new(t)),
            stats: track.as_ref().ok().map(|t| TrackStats::of(t)),
            track,
            generation: row.generation,
            id: row.id,
            role: row.role,
            progress: row.progress,
            laps: row.laps,
            end: row.end,
        }
    }

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
    watch: Watch,
    pub generation: Option<u32>,
    pub drawn: Vec<Drawn>,
    generator: GeneratorParams,
    /// Built tracks by id, so polling doesn't rebuild them.
    cache: HashMap<String, Result<Arc<Track>, String>>,
}

impl LastGeneration {
    pub fn new(run: &str) -> Self {
        let mut last = Self {
            run: run.to_owned(),
            watch: Watch::new(run_log::run_dir(run).join("tracks.csv")),
            generation: None,
            drawn: vec![],
            generator: run_log::generator(run),
            cache: HashMap::new(),
        };
        last.reload();
        last
    }

    /// Re-read if the file changed. Returns true when it did.
    pub fn poll(&mut self, dt: f64) -> bool {
        if !self.watch.changed(dt) {
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
        let rows = run_log::parse(&self.watch.read());
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
                let track = self.cache.entry(r.id.clone()).or_insert_with(|| run_log::build(&r.id, &self.generator)).clone();
                Drawn::new(r, track)
            })
            .collect();
        self.generation = Some(generation);
    }
}
