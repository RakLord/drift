//! Cosmetic effects driven by sim state. They read the car but never feed
//! back into the sim, and use their own RNG so the sim stays deterministic.

pub mod damage;
pub mod skidmarks;
pub mod smoke;
pub mod sparks;

use common::math::smoothstep;
use sim::car::{CarParams, CarState, WheelState};
use sim::world::Contact;

use super::camera::FollowCamera;
use damage::Damage;
use skidmarks::SkidMarks;
use smoke::Smoke;
use sparks::Sparks;

pub struct Effects {
    smoke: Smoke,
    skids: SkidMarks,
    sparks: Sparks,
    pub damage: Damage,
}

impl Effects {
    pub fn new() -> Self {
        Self { smoke: Smoke::new(), skids: SkidMarks::new(), sparks: Sparks::new(), damage: Damage::new() }
    }

    /// Advance by one sim tick. Called from the fixed-step loop, so effects
    /// follow sim time (they freeze when paused and slow down in slow-mo).
    pub fn on_tick(&mut self, state: &CarState, params: &CarParams, contacts: &[Contact], dt: f64) {
        let positions = state.wheel_positions(params);
        for (i, (wheel, position)) in state.wheels.iter().zip(positions).enumerate() {
            let intensity = slide_intensity(wheel, params);
            self.smoke.emit(i, position, state, wheel, intensity, dt);
            self.skids.track(i, position, intensity);
        }
        for contact in contacts {
            self.sparks.emit(contact);
            self.damage.on_contact(contact, state, params);
        }
        self.smoke.update(dt);
        self.sparks.update(dt);
        self.damage.update(dt);
    }

    pub fn clear(&mut self) {
        self.smoke.clear();
        self.skids.clear();
        self.sparks.clear();
        self.damage.clear();
    }

    /// Marks and debris on the road: draw before the car.
    pub fn draw_ground(&self, camera: &FollowCamera, params: &CarParams) {
        self.skids.draw(camera);
        self.damage.draw_debris(params);
    }

    /// Smoke and sparks over the car: draw after it.
    pub fn draw_air(&self) {
        self.smoke.draw();
        self.sparks.draw();
    }
}

/// 0..1: how hard a tyre is sliding, weighted by how much it is pressed into
/// the road. Drives both smoke and skid marks.
fn slide_intensity(wheel: &WheelState, params: &CarParams) -> f64 {
    let load = (wheel.load / params.tyre.reference_load).min(1.5);
    smoothstep(1.5, 10.0, wheel.slip_speed) * load
}
