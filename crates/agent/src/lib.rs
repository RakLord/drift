//! Neural network drivers: the bridge between `nn` and `sim`.

pub mod driver;
pub mod genome_file;
pub mod replay_file;
pub mod top_file;

pub use driver::{NnDriver, driver_spec};
pub use genome_file::{GenomeHeader, SavedGenome};
