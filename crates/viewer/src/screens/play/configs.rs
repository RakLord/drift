//! The config and track files the play screen watches and hot-reloads.

use common::config::WatchedConfig;
use serde::de::DeserializeOwned;
use sim::car::CarParams;
use sim::scoring::ScoringParams;
use sim::track::TrackDef;
use sim::track::generator::GeneratorParams;

use crate::paths::workspace_file;

const POLL_SECS: f64 = 0.5;

pub const LIBRARY_TRACK: &str = "tracks/test_loop.toml";
const BUILT_IN_TRACK: &str = include_str!("../../../../../tracks/test_loop.toml");

/// Something that changed on disk.
pub enum Reload {
    Car(CarParams),
    Scoring(ScoringParams),
    /// Read the new values with `library_track()` / `generator()`.
    LibraryTrack,
    Generator,
    Rejected(String),
}

pub struct Configs {
    car: Watched<CarParams>,
    scoring: Watched<ScoringParams>,
    track: Watched<TrackDef>,
    generator: Watched<GeneratorParams>,
    timer: f64,
}

/// A watched file, or the built-in value if it couldn't be loaded.
struct Watched<T> {
    name: &'static str,
    file: Option<WatchedConfig<T>>,
    fallback: T,
}

impl<T: DeserializeOwned + Clone> Watched<T> {
    fn load(name: &'static str, path: &str, fallback: impl FnOnce() -> T, warnings: &mut Vec<String>) -> Self {
        let file = WatchedConfig::load(workspace_file(path))
            .map_err(|err| warnings.push(format!("using built-in {name}: {err}")))
            .ok();
        Self { name, file, fallback: fallback() }
    }

    fn get(&self) -> T {
        self.file.as_ref().map_or_else(|| self.fallback.clone(), |f| f.get().clone())
    }

    fn poll(&mut self, wrap: impl FnOnce(T) -> Reload) -> Option<Reload> {
        match self.file.as_mut()?.poll()? {
            Ok(()) => Some(wrap(self.get())),
            Err(err) => Some(Reload::Rejected(format!("{} rejected: {err}", self.name))),
        }
    }
}

impl Configs {
    /// Load everything, recording a warning for anything that fell back to
    /// its built-in default.
    pub fn load(warnings: &mut Vec<String>) -> Self {
        Self {
            car: Watched::load("car", "config/car.toml", CarParams::default, warnings),
            scoring: Watched::load("scoring", "config/scoring.toml", ScoringParams::default, warnings),
            track: Watched::load("track", LIBRARY_TRACK, built_in_track, warnings),
            generator: Watched::load("generator", "config/generator.toml", GeneratorParams::default, warnings),
            timer: 0.0,
        }
    }

    pub fn car(&self) -> CarParams {
        self.car.get()
    }

    pub fn scoring(&self) -> ScoringParams {
        self.scoring.get()
    }

    pub fn library_track(&self) -> TrackDef {
        self.track.get()
    }

    pub fn generator(&self) -> GeneratorParams {
        self.generator.get()
    }

    /// Check the files (every half second) and report what changed.
    pub fn poll(&mut self, dt: f64) -> Vec<Reload> {
        self.timer += dt;
        if self.timer < POLL_SECS {
            return vec![];
        }
        self.timer = 0.0;
        [
            self.car.poll(Reload::Car),
            self.scoring.poll(Reload::Scoring),
            self.track.poll(|_| Reload::LibraryTrack),
            self.generator.poll(|_| Reload::Generator),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

fn built_in_track() -> TrackDef {
    toml::from_str(BUILT_IN_TRACK).expect("built-in track is invalid")
}
