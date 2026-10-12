//! Dev tool: a scripted driver for checking visuals without a pad. It follows
//! the centreline at a steady speed, and every few seconds flicks the car
//! sideways with the handbrake so there is smoke to look at.
//!
//! In doughnut mode it instead initiates a left-hand drift, locks the steering
//! once the target angle is reached, and holds the angle with throttle alone.

use sim::{Controls, World};

const TARGET_SPEED: f64 = 70.0 / 3.6;
const LOOK_AHEAD: usize = 14;

pub struct Autopilot {
    time: f64,
    donut: Option<Donut>,
}

impl Autopilot {
    pub fn new(donut: bool) -> Self {
        Self { time: 0.0, donut: donut.then(Donut::default) }
    }

    /// A fresh autopilot in the same mode.
    pub fn restarted(&self) -> Self {
        Self::new(self.donut.is_some())
    }

    pub fn controls(&mut self, world: &World, dt: f64) -> Controls {
        self.time += dt;
        if let Some(donut) = &mut self.donut {
            return donut.controls(world, dt);
        }
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

const DONUT_ENTRY_SPEED: f64 = 8.0;
const DONUT_INITIATE_STEER: f64 = 0.5;
/// Steering held once the drift is established (negative = countersteer).
const DONUT_COUNTERSTEER: f64 = -0.4;
const DONUT_ANGLE: f64 = 40.0_f64.to_radians();
const DONUT_KP: f64 = 2.0;
const DONUT_KI: f64 = 0.5;
const DONUT_KD: f64 = 3.0;

#[derive(Default)]
enum DonutPhase {
    #[default]
    RollUp,
    /// Steer in on full throttle until the rear steps out.
    Initiate,
    /// Countersteer grows with the angle.
    Settle,
    /// Steering locked; throttle alone holds the angle.
    Hold,
}

#[derive(Default)]
struct Donut {
    phase: DonutPhase,
    integral: f64,
    previous_angle: f64,
}

impl Donut {
    fn controls(&mut self, world: &World, dt: f64) -> Controls {
        let s = &world.car.state;
        // Positive = drifting left, the way this script turns.
        let angle = -s.body_slip_angle();
        let angle_rate = (angle - self.previous_angle) / dt;
        self.previous_angle = angle;

        match self.phase {
            DonutPhase::RollUp if s.speed() >= DONUT_ENTRY_SPEED => self.phase = DonutPhase::Initiate,
            DonutPhase::Initiate if angle >= 10.0_f64.to_radians() => {
                self.phase = DonutPhase::Settle;
                self.integral = 0.4 / DONUT_KI;
            }
            DonutPhase::Settle if (angle - DONUT_ANGLE).abs() < 3.0_f64.to_radians() => self.phase = DonutPhase::Hold,
            _ => {}
        }

        let hold_throttle = |integral: &mut f64| {
            let error = DONUT_ANGLE - angle;
            *integral = (*integral + error * dt).clamp(-1.0 / DONUT_KI, 1.0 / DONUT_KI);
            (DONUT_KP * error + DONUT_KI * *integral - DONUT_KD * angle_rate).clamp(0.0, 1.0)
        };
        let (steer, throttle) = match self.phase {
            DonutPhase::RollUp => (0.0, ((DONUT_ENTRY_SPEED - s.speed()) * 0.5 + 0.3).clamp(0.0, 1.0)),
            DonutPhase::Initiate => (DONUT_INITIATE_STEER, 1.0),
            DonutPhase::Settle => {
                (DONUT_COUNTERSTEER * (angle / DONUT_ANGLE).clamp(0.0, 1.0), hold_throttle(&mut self.integral))
            }
            DonutPhase::Hold => (DONUT_COUNTERSTEER, hold_throttle(&mut self.integral)),
        };
        Controls { steer, throttle, ..Default::default() }
    }
}
