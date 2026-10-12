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
    pub test_every: u32,
}

/// Before saving best.genome, the top `candidates` genomes by fitness drive
/// `tracks` extra generated tracks, and the best over all its runs is saved.
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
    fn default_config_parses() {
        let c = super::TrainingConfig::default();
        assert!(!c.islands.is_empty() && !c.curriculum.is_empty());
    }
}
