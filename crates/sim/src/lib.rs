//! The drift simulation: car physics, track, sensors and scoring.
//!
//! No graphics and no neural networks live here. Given the same inputs the sim
//! always produces the same outputs.

pub mod car;
pub mod controls;

pub use car::Car;
pub use controls::Controls;

pub const GRAVITY: f64 = 9.81;
