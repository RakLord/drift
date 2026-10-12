//! The drift simulation: car physics, track, sensors and scoring.
//!
//! No graphics and no neural networks live here. Given the same inputs the sim
//! always produces the same outputs.

pub mod car;
pub mod controller;
pub mod controls;
pub mod episode;
pub mod frame;
pub mod recording;
pub mod scoring;
pub mod sensors;
pub mod track;
pub mod world;

pub use car::Car;
pub use controller::Controller;
pub use controls::Controls;
pub use world::World;

pub const GRAVITY: f64 = 9.81;
