//! Dev tool: a scripted driver for checking visuals without a pad. It follows
//! the centreline at a steady speed, and every few seconds flicks the car
//! sideways with the handbrake so there is smoke to look at.

use sim::{Controls, World};

const TARGET_SPEED: f64 = 70.0 / 3.6;
const LOOK_AHEAD: usize = 14;

pub struct Autopilot {
    time: f64,
}

impl Autopilot {
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    pub fn controls(&mut self, world: &World, dt: f64) -> Controls {
        self.time += dt;
        let track = &world.track;
        let s = &world.car.state;
        let target = track.samples[(world.place.index + LOOK_AHEAD) % track.samples.len()].position;
        let local = (target - s.position).to_local(s.heading);
        let steer = (local.y.atan2(local.x) / world.car.params.steering.max_lock()).clamp(-1.0, 1.0);
        let speed_error = TARGET_SPEED - s.speed();
        let flick = self.time % 6.0 > 5.4;
        Controls {
            steer,
            throttle: (speed_error * 0.3).clamp(0.0, 1.0),
            brake: (-speed_error * 0.1).clamp(0.0, 1.0),
            handbrake: if flick { 1.0 } else { 0.0 },
            clutch: if flick { 1.0 } else { 0.0 },
        }
    }
}
