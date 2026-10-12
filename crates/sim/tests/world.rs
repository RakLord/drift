//! Car-on-track behaviour: walls, verges, progress.

use std::sync::Arc;

use common::{Rng, Vec2};
use sim::car::{CarParams, CarState};
use sim::track::{EdgeKind, Track, TrackDef};
use sim::track::def::AnchorDef;
use sim::{Controls, World};

const DT: f64 = 1.0 / 120.0;
const KMH: f64 = 1.0 / 3.6;

/// Two 200 m straights joined by radius-60 half circles. Driving +x along
/// y = 0 first, so left is +y.
fn oval(left: EdgeKind, right: EdgeKind) -> Arc<Track> {
    let h = 0.5523 * 60.0;
    let points = [
        ([0.0, 0.0], [h, 0.0]),
        ([200.0, 0.0], [h, 0.0]),
        ([260.0, 60.0], [0.0, h]),
        ([200.0, 120.0], [-h, 0.0]),
        ([0.0, 120.0], [-h, 0.0]),
        ([-60.0, 60.0], [0.0, -h]),
    ];
    let def = TrackDef {
        version: 1,
        name: "test oval".into(),
        width: 12.0,
        verge: 8.0,
        anchors: points.iter().map(|&(pos, handle)| AnchorDef { pos, handle: Some(handle), left, right }).collect(),
    };
    Arc::new(Track::build(def).unwrap())
}

fn world_with(track: Arc<Track>, position: Vec2, heading: f64, velocity: Vec2) -> World {
    let params = CarParams::default();
    let mut state = CarState::rolling(&params, position, heading, velocity.length());
    state.velocity = velocity;
    let mut world = World::new(track, params);
    world.set_car_state(state);
    world
}

/// Steer towards a point on the centreline a little ahead (pure pursuit).
fn follow_centreline(world: &World, throttle: f64) -> Controls {
    let track = &world.track;
    let ahead = (world.place.index + 12) % track.samples.len();
    let target = track.samples[ahead].position;
    let s = &world.car.state;
    let local = (target - s.position).to_local(s.heading);
    let steer = (local.y.atan2(local.x) / world.car.params.steering.max_lock()).clamp(-1.0, 1.0);
    Controls { steer, throttle, ..Default::default() }
}

#[test]
fn head_on_hit_stops_at_the_wall() {
    let track = oval(EdgeKind::Hard, EdgeKind::Hard);
    // Pointing straight at the left wall (y = 6) at 50 km/h.
    let mut world = world_with(track, Vec2::new(100.0, 0.0), std::f64::consts::FRAC_PI_2, Vec2::new(0.0, 50.0 * KMH));
    let mut biggest: f64 = 0.0;
    for _ in 0..240 {
        for c in world.step(Controls::default(), DT) {
            biggest = biggest.max(c.impact_speed);
        }
        assert!(world.place.offset < 6.0, "went through the wall: offset {}", world.place.offset);
    }
    println!("impact {biggest:.1} m/s, rebound {:.2} m/s", world.car.state.velocity.y);
    assert!(biggest > 10.0);
    assert!(world.car.state.velocity.y.abs() < 2.0, "bounced off at {:.1} m/s", world.car.state.velocity.y);
}

#[test]
fn light_tap_does_not_spin_the_car() {
    let track = oval(EdgeKind::Hard, EdgeKind::Hard);
    // Cruising at 60 km/h just off the left wall, angled 2 degrees into it:
    // the front corner glances the wall at about 0.6 m/s.
    let angle = 2f64.to_radians();
    let start = Vec2::new(60.0, 6.0 - 1.2);
    let mut world = world_with(track, start, angle, Vec2::from_angle(angle) * (60.0 * KMH));
    let mut hits = 0;
    let mut peak_yaw: f64 = 0.0;
    for _ in 0..240 {
        hits += world.step(Controls { throttle: 0.2, ..Default::default() }, DT).len();
        peak_yaw = peak_yaw.max(world.car.state.yaw_rate.abs());
    }
    let heading = world.car.state.heading.to_degrees();
    println!("tap: {hits} contacts, peak yaw {peak_yaw:.2} rad/s, heading {heading:.1} deg");
    assert!(hits > 0, "never touched the wall");
    assert!(peak_yaw < 0.3, "yaw kicked to {peak_yaw:.2} rad/s");
    assert!(heading.abs() < 15.0, "car turned to {heading:.1} deg");
}

#[test]
fn hard_angled_hit_rotates_the_car() {
    let track = oval(EdgeKind::Hard, EdgeKind::Hard);
    let angle = 35f64.to_radians();
    let velocity = Vec2::from_angle(angle) * (90.0 * KMH);
    let mut world = world_with(track, Vec2::new(60.0, 0.0), angle, velocity);
    let mut peak_yaw: f64 = 0.0;
    for _ in 0..120 {
        world.step(Controls::default(), DT);
        peak_yaw = peak_yaw.max(world.car.state.yaw_rate.abs());
    }
    println!("hard hit: peak yaw {peak_yaw:.2} rad/s");
    assert!(peak_yaw > 0.8, "only {peak_yaw:.2} rad/s");
}

#[test]
fn never_escapes_the_walls_under_random_driving() {
    let track = oval(EdgeKind::Hard, EdgeKind::Soft);
    let (lo, hi) = (-(6.0 + 8.0), 6.0);
    let mut rng = Rng::new(1234);
    let mut world = World::new(track, CarParams::default());
    let mut controls = Controls::default();
    for tick in 0..120 * 90 {
        if tick % 30 == 0 {
            controls = Controls {
                steer: rng.range(-1.0, 1.0),
                throttle: rng.next_f64(),
                brake: if rng.chance(0.1) { 1.0 } else { 0.0 },
                handbrake: if rng.chance(0.15) { 1.0 } else { 0.0 },
                clutch: if rng.chance(0.1) { 1.0 } else { 0.0 },
            };
        }
        world.step(controls, DT);
        let offset = world.place.offset;
        // A car corner may sit up to ~2.4 m from the centre of the car.
        assert!(offset > lo - 0.2 && offset < hi + 0.2, "escaped: offset {offset:.2} at tick {tick}");
        assert!(world.car.state.velocity.length().is_finite());
    }
}

#[test]
fn verge_slows_the_car_more_than_asphalt() {
    let coast = |y: f64| {
        let track = oval(EdgeKind::Soft, EdgeKind::Soft);
        let mut world = world_with(track, Vec2::new(20.0, y), 0.0, Vec2::new(60.0 * KMH, 0.0));
        for _ in 0..240 {
            world.step(Controls { clutch: 1.0, ..Default::default() }, DT);
        }
        world.car.state.speed()
    };
    let (asphalt, verge) = (coast(0.0), coast(-10.0));
    println!("after 2 s coasting: asphalt {:.1} km/h, verge {:.1} km/h", asphalt / KMH, verge / KMH);
    assert!(verge < asphalt - 1.0);
}

#[test]
fn progress_counts_a_full_lap() {
    let track = oval(EdgeKind::Soft, EdgeKind::Soft);
    let length = track.length;
    let mut world = World::new(track, CarParams::default());
    let mut ticks = 0;
    while world.progress < length * 1.1 && ticks < 120 * 120 {
        let controls = follow_centreline(&world, 0.45);
        world.step(controls, DT);
        ticks += 1;
    }
    println!("lap of {length:.0} m: progress {:.0} m in {:.1} s", world.progress, ticks as f64 * DT);
    assert!(world.progress >= length * 1.1, "only {:.0} m", world.progress);
}
