//! Shared building blocks used across the workspace.
//!
//! Nothing in here knows about cars, networks or rendering.

pub mod config;
pub mod math;
pub mod paths;
pub mod rng;

pub use math::Vec2;
pub use rng::Rng;
