//! Engine -> clutch -> fixed ratio -> welded diff -> rear wheels.
//!
//! The welded diff makes both rear wheels one spinning body (the axle). The
//! clutch either holds the engine and axle together (locked) or slips,
//! passing at most its capacity in torque. A clutch kick is this model doing
//! its thing: pedal in, the engine revs up freely; pedal out, the clutch locks
//! and dumps that spinning energy into the rear tyres.

use super::engine;
use super::params::CarParams;
use super::wheel::SpinStep;

#[derive(Clone, Copy, Debug, Default)]
pub struct DrivetrainState {
    pub engine_omega: f64,
    pub clutch_locked: bool,
    pub limiter_active: bool,
}

/// Rear tyre forces linearised around the current axle spin (see `SpinStep`).
pub struct AxleLoad {
    pub tyre_force: f64,
    pub tyre_stiffness: f64,
    pub friction_torque: f64,
}

/// Advance engine and rear axle by `dt`. Returns the new axle spin.
pub fn step(
    p: &CarParams,
    state: &mut DrivetrainState,
    axle_omega: f64,
    axle: &AxleLoad,
    throttle: f64,
    clutch_pedal: f64,
    dt: f64,
) -> f64 {
    let ratio = p.drivetrain.ratio;
    let engine_inertia = p.engine.inertia;
    let axle_inertia = 2.0 * p.wheel.inertia + p.drivetrain.inertia;

    let throttle = engine::effective_throttle(&p.engine, state.engine_omega, throttle, &mut state.limiter_active);
    let engine_torque = engine::torque(&p.engine, state.engine_omega, throttle);
    let capacity = engine::clutch_capacity(&p.clutch, clutch_pedal, state.engine_omega);

    let spin = |inertia: f64, drive_torque: f64| SpinStep {
        inertia,
        omega: axle_omega,
        drive_torque,
        radius: p.wheel.radius,
        tyre_force: axle.tyre_force,
        tyre_stiffness: axle.tyre_stiffness,
        friction_torque: axle.friction_torque,
    };

    let mut slip_direction = (state.engine_omega - ratio * axle_omega).signum();
    if state.clutch_locked {
        // Engine and axle move together; the engine's inertia appears at the
        // axle multiplied by ratio^2.
        let combined = axle_inertia + ratio * ratio * engine_inertia;
        let new_axle = spin(combined, ratio * engine_torque).solve(dt);
        let new_engine = ratio * new_axle;
        // Torque the clutch had to carry to make the engine follow the axle.
        let clutch_torque = engine_torque - engine_inertia * (new_engine - state.engine_omega) / dt;
        if clutch_torque.abs() <= capacity && new_engine >= 0.0 {
            state.engine_omega = new_engine;
            return new_axle;
        }
        state.clutch_locked = false;
        slip_direction = clutch_torque.signum();
    }

    // Slipping: the clutch passes its full capacity in the direction of slip.
    let clutch_torque = capacity * slip_direction;
    let new_engine = (state.engine_omega + dt * (engine_torque - clutch_torque) / engine_inertia).max(0.0);
    let new_axle = spin(axle_inertia, ratio * clutch_torque).solve(dt);

    let crossed = (new_engine - ratio * new_axle).signum() != slip_direction;
    if crossed && capacity > 1.0 {
        // Speeds met during the step: lock, conserving angular momentum.
        let momentum = axle_inertia * new_axle + ratio * engine_inertia * new_engine;
        let locked_axle = momentum / (axle_inertia + ratio * ratio * engine_inertia);
        state.clutch_locked = true;
        state.engine_omega = (ratio * locked_axle).max(0.0);
        return locked_axle;
    }

    state.engine_omega = new_engine;
    new_axle
}
