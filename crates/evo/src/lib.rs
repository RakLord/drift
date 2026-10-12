//! Genetic algorithm for driver networks.
//!
//! Islands of genomes (flat network parameters), each island with its own
//! network shape. Each generation: keep the elites, breed the rest by
//! tournament selection, neuron-level crossover and self-adaptive mutation,
//! and drop in a few random immigrants. Every so often the weakest island
//! is replaced by a copy of the strongest, shape and all.

pub mod config;
pub mod crossover;
pub mod diversity;
pub mod fitness;
pub mod genome;
pub mod island;
pub mod mutation;
pub mod selection;

pub use config::GaConfig;
pub use fitness::{Curriculum, Stage};
pub use genome::Genome;
pub use island::{Ids, Island, migrate};
