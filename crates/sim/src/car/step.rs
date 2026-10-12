//! One physics substep: tyres -> wheel spin -> body forces -> integration.

use common::Vec2;

use super::drivetrain::{self, AxleLoad};
use super::ground::Ground;
use super::params::CarParams;
use super::state::CarState;
use super::tyre::{self, TyreOutput};
use super::wheel::{self, FL, FR, RL, RR, SpinStep};
use super::steering;
use crate::Controls;

pub fn substep(p: &CarParams, s: &mut CarState, controls: &Controls, ground: &[Ground; 4], dt: f64) {
    s.steer_angle = steering::update(&p.steering, s.steer_angle, controls.steer, dt);

    let offsets = p.body.wheel_offsets();
    let loads = s.suspension.wheel_loads(&p.suspension, &p.body);
    let body_velocity = s.local_velocity();
    let radius = p.wheel.radius;
    let steer = s.steer_angle;
    let steer_of = |i: usize| if wheel::is_front(i) { steer } else { 0.0 };
    let patch_velocity: [Vec2; 4] =
        std::array::from_fn(|i| wheel::contact_velocity(body_velocity, s.yaw_rate, offsets[i], steer_of(i)));

    // Tyre forces at the current spin, and how they change with spin.
    let before: [TyreOutput; 4] =
        std::array::from_fn(|i| {
            tyre::evaluate(&p.tyre, patch_velocity[i], s.wheels[i].omega * radius, loads[i], ground[i].grip)
        });
    let stiffness: [f64; 4] = std::array::from_fn(|i| {
        let surface = s.wheels[i].omega * radius;
        tyre::longitudinal_stiffness(&p.tyre, patch_velocity[i], surface, loads[i], ground[i].grip, before[i].force.x)
    });
    let rolling = |i: usize| p.tyre.rolling_resistance * ground[i].rolling_resistance * loads[i] * radius;

    // Front wheels spin freely.
    for i in [FL, FR] {
        s.wheels[i].omega = SpinStep {
            inertia: p.wheel.inertia,
            omega: s.wheels[i].omega,
            drive_torque: 0.0,
            radius,
            tyre_force: before[i].force.x,
            tyre_stiffness: stiffness[i],
            friction_torque: controls.brake * p.brakes.front_torque + rolling(i),
        }
        .solve(dt);
    }

    // Rear wheels are one axle, driven through the clutch.
    let axle = AxleLoad {
        tyre_force: before[RL].force.x + before[RR].force.x,
        tyre_stiffness: stiffness[RL] + stiffness[RR],
        friction_torque: 2.0 * controls.brake * p.brakes.rear_torque
            + controls.handbrake * p.brakes.handbrake_torque
            + rolling(RL)
            + rolling(RR),
    };
    let axle_omega =
        drivetrain::step(p, &mut s.drivetrain, s.wheels[RL].omega, &axle, controls.throttle, controls.clutch, dt);
    s.wheels[RL].omega = axle_omega;
    s.wheels[RR].omega = axle_omega;

    // Re-evaluate the tyres at the new spin; these forces move the body.
    let mut force = Vec2::ZERO;
    let mut torque = 0.0;
    for i in 0..4 {
        let out = tyre::evaluate(&p.tyre, patch_velocity[i], s.wheels[i].omega * radius, loads[i], ground[i].grip);
        let car_force = out.force.rotate(steer_of(i));
        force += car_force;
        torque += offsets[i].cross(car_force);

        let w = &mut s.wheels[i];
        w.load = loads[i];
        w.force = car_force;
        w.slip_ratio = out.slip_ratio;
        w.slip_angle = out.slip_angle;
        w.combined_slip = out.combined_slip;
        w.slip_speed = out.slip_speed;
    }
    force -= body_velocity * (p.aero.drag * body_velocity.length());

    // Semi-implicit Euler: velocities first, then positions with the new
    // velocities.
    s.accel = force / p.body.mass;
    s.velocity += s.accel.to_world(s.heading) * dt;
    s.yaw_rate += torque / p.body.yaw_inertia * dt;
    s.heading = common::math::wrap_angle(s.heading + s.yaw_rate * dt);
    s.position += s.velocity * dt;

    s.suspension.update(&p.suspension, &p.body, s.accel, dt);
}
