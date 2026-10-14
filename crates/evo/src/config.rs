use serde::{Deserialize, Serialize};

/// How breeding picks parents and elites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    /// By the scalar fitness alone.
    Scalar,
    /// By Pareto rank over (driving, drift quality), so specialists at
    /// either end survive alongside all-rounders.
    Pareto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaConfig {
    /// Genomes per island.
    pub population: usize,
    /// Share of each island copied unchanged into the next generation.
    pub elite_fraction: f64,
    pub tournament_size: usize,
    /// Chance a child comes from crossover rather than one parent.
    pub crossover_rate: f64,
    /// Chance each parameter is mutated.
    pub mutation_rate: f64,
    /// Mutation step size: starting value, limits, and how fast it adapts.
    pub sigma_start: f64,
    pub sigma_min: f64,
    pub sigma_max: f64,
    pub sigma_adapt: f64,
    /// Chance a parameter is thrown away and re-drawn.
    pub reset_rate: f64,
    /// Fresh random genomes per island per generation.
    pub immigrants: usize,
    /// Every this many generations, the weakest island may be replaced...
    pub migration_every: u32,
    /// ...if its best fitness is below this share of the best island's.
    /// Only for islands of a different shape when `migrants` > 0.
    pub migration_gap: f64,
    /// When the weakest island has the strongest's shape, this many of the
    /// strongest's top genomes replace its worst instead of a full copy (no
    /// gap needed). 0 = always a full copy, as before run11.
    #[serde(default)]
    pub migrants: usize,
    /// Islands are compared on their best fitness averaged over this many
    /// recent generations (1 = this generation only, as before run11).
    #[serde(default = "one")]
    pub migration_window: u32,
    pub selection: Selection,
    /// On a curriculum stage change: every sigma resets to `sigma_start`,
    /// and this share of each island is replaced by copies of its best with
    /// every parameter nudged by `reseed_sigma`.
    pub reseed_fraction: f64,
    pub reseed_sigma: f64,
}

fn one() -> u32 {
    1
}
