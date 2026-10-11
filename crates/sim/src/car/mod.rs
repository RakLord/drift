//! Car physics: a 4-wheel model with combined-slip tyres, weight transfer,
//! an engine and clutch, a welded diff, and a rate-limited steering rack.
//!
//! Frames: world is +x right, +y up, angles counter-clockwise. The car frame
//! is +x forward, +y left.

pub mod drivetrain;
pub mod engine;
pub mod params;
pub mod state;
pub mod steering;
mod step;
pub mod suspension;
pub mod tyre;
pub mod wheel;

use common::Vec2;

pub use params::CarParams;
pub use state::{CarState, WheelState};

use crate::Controls;

#[derive(Clone, Debug)]
pub struct Car {
    pub params: CarParams,
    pub state: CarState,
}

impl Car {
    pub fn new(params: CarParams, position: Vec2, heading: f64) -> Self {
        let state = CarState::at_rest(&params, position, heading);
        Self { params, state }
    }

    /// Advance by one sim tick of `dt` seconds, split into the configured
    /// number of substeps.
    pub fn step(&mut self, controls: Controls, dt: f64) {
        let controls = controls.clamped();
        self.state.controls = controls;
        let substeps = self.params.substeps.max(1);
        let h = dt / f64::from(substeps);
        for _ in 0..substeps {
            step::substep(&self.params, &mut self.state, &controls, h);
        }
    }
}
