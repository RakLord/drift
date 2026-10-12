//! Shared building blocks used across the workspace.
//!
//! Nothing in here knows about cars, networks or rendering.

pub mod config;
pub mod math;
pub mod paths;
pub mod procs;
pub mod rng;
pub mod toml_lines;

pub use math::Vec2;
pub use rng::Rng;
