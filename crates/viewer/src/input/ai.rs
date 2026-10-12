//! A trained genome driving: runs it exactly as the trainer does (same
//! inputs, decisions every few ticks, rolling start, same end-of-run rules,
//! and the run's own car/scoring settings when it came from a training run).
//!
//! In follow mode it watches the genome file: when training writes a new
//! best, it's picked up and swapped in at the start of the next run.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use agent::NnDriver;
use agent::genome_file::{self, GenomeHeader, SavedGenome};
use nn::Network;
use serde::Deserialize;
use sim::car::CarParams;
use sim::episode::{EndReason, EpisodeConfig, TICK, Termination};
use sim::scoring::{ScoreEvent, Scorer, ScoringParams};
use sim::sensors::observation::{Observation, Observer};
use sim::{Controller, Controls, World};

const POLL_SECS: f64 = 0.5;

/// Settings of the run a genome came from (`<run>/config/*.toml`).
pub struct RunSettings {
    pub episode: EpisodeConfig,
    pub car: Option<CarParams>,
    pub scoring: Option<ScoringParams>,
}

#[derive(Deserialize)]
struct TrainingFile {
    episode: EpisodeConfig,
}

impl RunSettings {
    fn for_genome(path: &Path) -> Self {
        let dir = path.parent().unwrap_or(Path::new(".")).join("config");
        let episode = common::config::load::<TrainingFile>(dir.join("training.toml")).map_or_else(
            |_| EpisodeConfig {
                max_secs: 60.0,
                start_speed_kmh: 50.0,
                decision_every: 4,
                sensor_noise: 0.0,
                no_progress_secs: 4.0,
                max_backwards: 20.0,
                max_hard_hits: 3,
                end_on_spin: true,
            },
            |f| f.episode,
        );
        Self {
            episode,
            car: common::config::load(dir.join("car.toml")).ok(),
            scoring: common::config::load(dir.join("scoring.toml")).ok(),
        }
    }
}

pub struct AiDriver {
    pub path: PathBuf,
    pub follow: bool,
    pub run: RunSettings,
    /// The genome driving now (none while waiting for training to write one).
    current: Option<(GenomeHeader, NnDriver)>,
    /// A newer genome, waiting for the next run.
    pending: Option<SavedGenome>,
    modified: Option<SystemTime>,
    poll_timer: f64,
    observer: Observer,
    held: Controls,
    tick: u64,
    termination: Termination,
    /// The last thing it saw (for the inputs panel).
    pub last_observation: Option<Observation>,
    /// Off = you're driving; Tab hands back.
    pub enabled: bool,
}

impl AiDriver {
    /// Load a genome. In follow mode a missing file is fine: it waits for
    /// training to write one.
    pub fn new(path: PathBuf, follow: bool) -> Result<Self, String> {
        let run = RunSettings::for_genome(&path);
        let mut ai = Self {
            path,
            follow,
            run,
            current: None,
            pending: None,
            modified: None,
            poll_timer: 0.0,
            observer: Observer::new(0.0, 0),
            held: Controls::default(),
            tick: 0,
            termination: Termination::default(),
            last_observation: None,
            enabled: true,
        };
        match ai.read() {
            Ok(saved) => ai.install(saved),
            Err(err) if !follow => return Err(err),
            Err(_) => {}
        }
        Ok(ai)
    }

    fn read(&mut self) -> Result<SavedGenome, String> {
        self.modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        genome_file::load(&self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    fn install(&mut self, saved: SavedGenome) {
        let network = Network::new(saved.header.spec.clone(), saved.params);
        self.current = Some((saved.header, NnDriver::new(network)));
    }

    /// Follow mode: check for a newer genome. Returns a message when one
    /// arrives.
    pub fn poll(&mut self, dt: f64) -> Option<String> {
        self.poll_timer += dt;
        if !self.follow || self.poll_timer < POLL_SECS {
            return None;
        }
        self.poll_timer = 0.0;
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified == self.modified {
            return None;
        }
        // Training may be mid-write; if so, try again next poll.
        let saved = self.read().ok()?;
        let generation = saved.header.generation;
        if self.current.is_none() {
            self.install(saved);
            return Some(format!("watching gen {generation}"));
        }
        self.pending = Some(saved);
        Some(format!("gen {generation} ready: starts next run (B: now)"))
    }

    /// Swap in the newest genome, if one is waiting. Returns its generation.
    pub fn take_pending(&mut self) -> Option<u32> {
        let saved = self.pending.take()?;
        let generation = saved.header.generation;
        self.install(saved);
        Some(generation)
    }

    /// Re-read the file now (B), even if it hasn't changed.
    pub fn reload_now(&mut self) -> Result<u32, String> {
        let saved = self.read()?;
        let generation = saved.header.generation;
        self.pending = None;
        self.install(saved);
        Ok(generation)
    }

    /// Start a new run.
    pub fn reset(&mut self) {
        self.tick = 0;
        self.held = Controls::default();
        self.termination = Termination::default();
        self.last_observation = None;
    }

    pub fn start_speed(&self) -> f64 {
        self.run.episode.start_speed_kmh / 3.6
    }

    /// Controls for this tick.
    pub fn controls(&mut self, world: &World, scorer: &Scorer) -> Controls {
        let Some((_, driver)) = &mut self.current else { return Controls::default() };
        let every = u64::from(self.run.episode.decision_every.max(1));
        if self.tick.is_multiple_of(every) {
            let observation = self.observer.observe(world, scorer);
            self.held = driver.decide(&observation);
            self.last_observation = Some(observation);
        }
        self.tick += 1;
        self.held
    }

    /// Apply the training end-of-run rules to this tick.
    pub fn check_end(&mut self, progress: f64, events: &[ScoreEvent]) -> Option<EndReason> {
        self.current.as_ref()?;
        let time = self.tick as f64 * TICK;
        self.termination.update(&self.run.episode, progress, events, time, TICK)
    }

    pub fn decision(&self) -> Controls {
        self.held
    }

    pub fn status(&self) -> String {
        let Some((h, _)) = &self.current else {
            return format!("AI: waiting for {}", self.path.display());
        };
        let following = if self.follow { "  following" } else { "" };
        let next = self.pending.as_ref().map_or(String::new(), |p| format!("  (next: gen {})", p.header.generation));
        format!("AI gen {} {} fitness {:.0}{following}{next}", h.generation, h.spec.describe(), h.fitness)
    }

    pub fn generation(&self) -> Option<u32> {
        self.current.as_ref().map(|(h, _)| h.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nn::{Activation, LayerSpec};

    fn write_genome(path: &Path, generation: u32) {
        let spec = agent::driver_spec(vec![LayerSpec { size: 4, activation: Activation::Tanh }]);
        let params = spec.random_params(&mut common::Rng::new(u64::from(generation)));
        let saved = SavedGenome { header: GenomeHeader::new(spec, generation, 0, 1.0, "test"), params };
        genome_file::save(path, &saved).unwrap();
    }

    #[test]
    fn follow_waits_then_queues_new_genomes_for_the_next_run() {
        let dir = std::env::temp_dir().join(format!("drift-follow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("best.genome");

        // No genome yet: follow mode waits instead of failing.
        let mut ai = AiDriver::new(path.clone(), true).unwrap();
        assert!(ai.generation().is_none());
        assert!(ai.status().contains("waiting"));

        // Training writes one: it's picked up straight away.
        write_genome(&path, 3);
        assert!(ai.poll(1.0).is_some());
        assert_eq!(ai.generation(), Some(3));

        // A newer one waits for the next run.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_genome(&path, 9);
        assert!(ai.poll(1.0).unwrap().contains("gen 9"));
        assert_eq!(ai.generation(), Some(3));
        assert_eq!(ai.take_pending(), Some(9));
        assert_eq!(ai.generation(), Some(9));

        // Nothing new: nothing happens.
        assert!(ai.poll(1.0).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plain_genome_must_exist() {
        assert!(AiDriver::new(PathBuf::from("/nonexistent/best.genome"), false).is_err());
    }
}
