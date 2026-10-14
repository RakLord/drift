use std::path::Path;

use common::config::{self, ConfigError};
use evo::{GaConfig, Stage};
use nn::LayerSpec;
use serde::{Deserialize, Serialize};
use sim::episode::EpisodeConfig;

const DEFAULT_TRAINING_TOML: &str = include_str!("../../../config/training.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingConfig {
    pub seed: u64,
    pub generations: u32,
    pub ga: GaConfig,
    pub network: NetworkConfig,
    pub islands: Vec<IslandConfig>,
    pub episode: EpisodeConfig,
    pub tracks: TrackConfig,
    pub checkpoints: CheckpointConfig,
    /// Picking the driver saved as best.genome (off if missing, so older
    /// runs still resume).
    #[serde(default)]
    pub best: BestConfig,
    /// Replays of the saved best (off if missing, so older runs still resume).
    #[serde(default)]
    pub replays: ReplayConfig,
    /// Genomes for the live multi-car viewer (defaults if missing).
    #[serde(default)]
    pub watch: WatchConfig,
    /// What a human pick from the review grid does (defaults if missing).
    #[serde(default)]
    pub review: ReviewConfig,
    pub curriculum: Vec<Stage>,
}

/// Optional starting priors for fresh networks (see config/training.toml).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfig {
    pub output_bias: Vec<f32>,
    pub output_init_scale: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IslandConfig {
    pub hidden: Vec<LayerSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackConfig {
    pub per_generation: u32,
    /// Saved tracks (workspace-relative) driven every generation on top of
    /// the generated ones. They're left out of the test set.
    #[serde(default)]
    pub fixed: Vec<String>,
    /// Saved tracks to draw from: each generation drives `pool_per_generation`
    /// of them, picked from (seed, generation). Left out of the test set,
    /// and the `[best]` check tracks come from here too.
    #[serde(default)]
    pub pool: Vec<String>,
    #[serde(default)]
    pub pool_per_generation: u32,
    pub test_every: u32,
}

impl TrackConfig {
    /// How many pool tracks a generation draws (no more than the pool has).
    pub fn pool_draw(&self) -> usize {
        (self.pool_per_generation as usize).min(self.pool.len())
    }

    /// Every saved track trained on, fixed or pooled.
    pub fn trained_on(&self) -> impl Iterator<Item = &String> {
        self.fixed.iter().chain(&self.pool)
    }
}

/// Before saving best.genome, the top `candidates` genomes by fitness drive
/// `tracks` extra tracks (pool tracks not driven this generation, then
/// generated ones), and the best over all its runs is saved.
/// A genome that topped the generation on a lucky draw doesn't replace a
/// steadier one. 0 (either) = just save the generation's fittest.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BestConfig {
    pub candidates: usize,
    pub tracks: u32,
}

impl BestConfig {
    pub fn enabled(&self) -> bool {
        self.candidates > 1 && self.tracks > 0
    }
}

/// Every `every` generations, the saved best is re-run with recording on
/// each of that generation's tracks, into runs/<run>/replays/. 0 = off.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayConfig {
    pub every: u32,
}

/// Written to runs/<run>/top.genomes every generation for `viewer --watch`:
/// the top `per_island` of each island by fitness, plus the `drifters` best
/// by drift quality not already picked.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchConfig {
    pub per_island: usize,
    pub drifters: usize,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self { per_island: 5, drifters: 10 }
    }
}

/// A genome picked in the viewer's review grid (`agent::picks`) is kept
/// unchanged for `protect_gens` generations, and `flood_fraction` of its
/// island is replaced by mutated children of it straight away.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewConfig {
    pub protect_gens: u32,
    pub flood_fraction: f64,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self { protect_gens: 50, flood_fraction: 0.3 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointConfig {
    pub every: u32,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        toml::from_str(DEFAULT_TRAINING_TOML).expect("config/training.toml is invalid")
    }
}

impl TrainingConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        config::load(path)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn configs_without_the_run11_migration_keys_keep_the_old_rule() {
        let old: String = super::DEFAULT_TRAINING_TOML
            .lines()
            .filter(|l| !l.starts_with("migrants") && !l.starts_with("migration_window"))
            .map(|l| format!("{l}\n"))
            .collect();
        let c: super::TrainingConfig = toml::from_str(&old).unwrap();
        assert_eq!((c.ga.migrants, c.ga.migration_window), (0, 1), "full copies, judged on one generation");
        let new = super::TrainingConfig::default();
        assert_eq!((new.ga.migrants, new.ga.migration_window), (5, 25));
    }

    #[test]
    fn default_config_parses() {
        let c = super::TrainingConfig::default();
        assert!(!c.islands.is_empty() && !c.curriculum.is_empty());
    }
}
