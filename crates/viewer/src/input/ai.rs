//! A trained genome driving: loads a `.genome` file and runs it exactly as
//! the trainer does (same inputs, decisions every few ticks, rolling start).

use std::path::Path;

use agent::NnDriver;
use agent::genome_file::{self, GenomeHeader};
use nn::Network;
use sim::scoring::Scorer;
use sim::sensors::observation::{Observation, Observer};
use sim::{Controller, Controls, World};

/// Must match training (config/training.toml, [episode]).
const DECISION_EVERY: u64 = 4;
pub const START_SPEED_KMH: f64 = 50.0;

pub struct AiDriver {
    pub header: GenomeHeader,
    driver: NnDriver,
    observer: Observer,
    held: Controls,
    tick: u64,
    /// The last thing it saw, and what it decided (for the inputs panel).
    pub last_observation: Option<Observation>,
    /// Off = you're driving; the AI still watches, so it can take back over.
    pub enabled: bool,
}

impl AiDriver {
    pub fn load(path: &Path) -> Result<Self, String> {
        let saved = genome_file::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let network = Network::new(saved.header.spec.clone(), saved.params);
        Ok(Self {
            header: saved.header,
            driver: NnDriver::new(network),
            observer: Observer::new(0.0, 0),
            held: Controls::default(),
            tick: 0,
            last_observation: None,
            enabled: true,
        })
    }

    pub fn reset(&mut self) {
        self.tick = 0;
        self.held = Controls::default();
        self.last_observation = None;
    }

    /// Controls for this tick.
    pub fn controls(&mut self, world: &World, scorer: &Scorer) -> Controls {
        if self.tick.is_multiple_of(DECISION_EVERY) {
            let observation = self.observer.observe(world, scorer);
            self.held = self.driver.decide(&observation);
            self.last_observation = Some(observation);
        }
        self.tick += 1;
        self.held
    }

    /// What it last decided.
    pub fn decision(&self) -> Controls {
        self.held
    }

    pub fn describe(&self) -> String {
        let h = &self.header;
        format!("{} gen {} island {} fitness {:.0}", h.spec.describe(), h.generation, h.island, h.fitness)
    }
}
