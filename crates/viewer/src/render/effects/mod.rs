//! Cosmetic effects driven by sim state. They read the car but never feed
//! back into the sim, and use their own RNG so the sim stays deterministic.

pub mod skidmarks;
pub mod smoke;

use common::math::smoothstep;
use sim::car::{CarParams, CarState, WheelState};

use super::camera::FollowCamera;
use skidmarks::SkidMarks;
use smoke::Smoke;

pub struct Effects {
    smoke: Smoke,
    skids: SkidMarks,
}

impl Effects {
    pub fn new() -> Self {
        Self { smoke: Smoke::new(), skids: SkidMarks::new() }
    }

    /// Advance by one sim tick. Called from the fixed-step loop, so effects
    /// follow sim time (they freeze when paused and slow down in slow-mo).
    pub fn on_tick(&mut self, state: &CarState, params: &CarParams, dt: f64) {
        let positions = state.wheel_positions(params);
        for (i, (wheel, position)) in state.wheels.iter().zip(positions).enumerate() {
            let intensity = slide_intensity(wheel, params);
            self.smoke.emit(i, position, state, wheel, intensity, dt);
            self.skids.track(i, position, intensity);
        }
        self.smoke.update(dt);
    }

    pub fn clear(&mut self) {
        self.smoke.clear();
        self.skids.clear();
    }

    /// Marks on the road: draw before the car.
    pub fn draw_ground(&self, camera: &FollowCamera) {
        self.skids.draw(camera);
    }

    /// Smoke hangs over the car: draw after it.
    pub fn draw_air(&self) {
        self.smoke.draw();
    }
}

/// 0..1: how hard a tyre is sliding, weighted by how much it is pressed into
/// the road. Drives both smoke and skid marks.
fn slide_intensity(wheel: &WheelState, params: &CarParams) -> f64 {
    let load = (wheel.load / params.tyre.reference_load).min(1.5);
    smoothstep(1.5, 10.0, wheel.slip_speed) * load
}
