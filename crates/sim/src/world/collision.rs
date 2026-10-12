//! Car vs solid boundary collisions.
//!
//! The car is a rigid box. A contact gets a proper impulse at the contact
//! point: how much it changes speed and spin depends on how fast that point
//! hits the wall and on the car's mass and yaw inertia. So a light bumper tap
//! only nudges the car, and a big hit spins it. Overlap is then removed by
//! moving the car, never by adding velocity, so resolving a contact can't
//! inject energy.

use common::Vec2;

use crate::car::Car;
use crate::track::{SegmentKind, Track};

/// Bounce: near zero, bumpers crumple rather than spring back.
const RESTITUTION: f64 = 0.05;
/// Friction coefficient for scraping along a wall.
const WALL_FRICTION: f64 = 0.35;
/// Corners further behind a wall than this are treated as on the far side,
/// not penetrating (guards against grabbing the wrong wall).
const MAX_PENETRATION: f64 = 1.5;
/// Contacts are resolved deepest first, re-checking after each.
const MAX_ITERATIONS: usize = 4;
/// Impacts slower than this aren't reported as events.
const REPORT_SPEED: f64 = 0.05;

#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub point: Vec2,
    /// Unit normal pointing away from the wall.
    pub normal: Vec2,
    /// Speed of the contact point into the wall at impact, m/s.
    pub impact_speed: f64,
    /// Speed of the contact point along the wall at impact, m/s.
    pub scrape_speed: f64,
    pub kind: SegmentKind,
}

struct Penetration {
    point: Vec2,
    normal: Vec2,
    depth: f64,
    kind: SegmentKind,
}

/// Push the car out of any solid boundary it overlaps and apply impulses.
/// Reported impacts are appended to `contacts`.
pub fn resolve(track: &Track, car: &mut Car, scratch: &mut Vec<u32>, contacts: &mut Vec<Contact>) {
    for _ in 0..MAX_ITERATIONS {
        let Some(p) = deepest_penetration(track, car, scratch) else { return };
        if let Some(contact) = apply_impulse(car, &p) {
            contacts.push(contact);
        }
        car.state.position += p.normal * p.depth;
    }
}

fn corners(car: &Car) -> [Vec2; 4] {
    let half = Vec2::new(car.params.body.length / 2.0, car.params.body.width / 2.0);
    let s = &car.state;
    [Vec2::new(half.x, half.y), Vec2::new(half.x, -half.y), Vec2::new(-half.x, -half.y), Vec2::new(-half.x, half.y)]
        .map(|c| s.position + c.rotate(s.heading))
}

fn deepest_penetration(track: &Track, car: &Car, scratch: &mut Vec<u32>) -> Option<Penetration> {
    let corners = corners(car);
    let reach = Vec2::new(1.0, 1.0);
    let lo = corners.iter().fold(Vec2::new(f64::MAX, f64::MAX), |m, c| Vec2::new(m.x.min(c.x), m.y.min(c.y)));
    let hi = corners.iter().fold(Vec2::new(f64::MIN, f64::MIN), |m, c| Vec2::new(m.x.max(c.x), m.y.max(c.y)));
    track.solid_near(lo - reach, hi + reach, scratch);

    let mut best: Option<Penetration> = None;
    let mut consider = |p: Penetration| {
        if p.depth > 0.0 && p.depth < MAX_PENETRATION && best.as_ref().is_none_or(|b| p.depth > b.depth) {
            best = Some(p);
        }
    };
    for &index in scratch.iter() {
        let seg = &track.segments[index as usize];
        let along = seg.b - seg.a;
        let length_sq = along.length_sq();

        // Car corners behind the wall line, within the segment's extent.
        for &corner in &corners {
            let u = (corner - seg.a).dot(along) / length_sq;
            if (0.0..=1.0).contains(&u) {
                let depth = -(corner - seg.a).dot(seg.normal);
                consider(Penetration { point: corner, normal: seg.normal, depth, kind: seg.kind });
            }
        }

        // Wall vertices poking into the car's side (a convex wall bulge can
        // hit the flat side of the car without any car corner crossing it).
        if inside_box(car, seg.a) {
            let depth = corners.iter().map(|&c| (seg.a - c).dot(seg.normal)).fold(f64::MIN, f64::max);
            consider(Penetration { point: seg.a, normal: seg.normal, depth, kind: seg.kind });
        }
    }
    best
}

fn inside_box(car: &Car, point: Vec2) -> bool {
    let local = (point - car.state.position).to_local(car.state.heading);
    local.x.abs() < car.params.body.length / 2.0 && local.y.abs() < car.params.body.width / 2.0
}

/// Impulse at the contact point: stop it moving into the wall (with a little
/// bounce) and resist sliding along it with Coulomb friction.
fn apply_impulse(car: &mut Car, p: &Penetration) -> Option<Contact> {
    let mass = car.params.body.mass;
    let inertia = car.params.body.yaw_inertia;
    let s = &mut car.state;
    let r = p.point - s.position;
    let point_velocity = s.velocity + r.perp() * s.yaw_rate;
    let normal_speed = point_velocity.dot(p.normal);
    if normal_speed >= 0.0 {
        return None; // Already separating.
    }
    let tangent = p.normal.perp();
    let tangent_speed = point_velocity.dot(tangent);

    let effective = |dir: Vec2| 1.0 / mass + r.cross(dir).powi(2) / inertia;
    let normal_impulse = -(1.0 + RESTITUTION) * normal_speed / effective(p.normal);
    let max_friction = WALL_FRICTION * normal_impulse;
    let friction_impulse = (-tangent_speed / effective(tangent)).clamp(-max_friction, max_friction);

    let impulse = p.normal * normal_impulse + tangent * friction_impulse;
    s.velocity += impulse / mass;
    s.yaw_rate += r.cross(impulse) / inertia;

    (-normal_speed > REPORT_SPEED).then_some(Contact {
        point: p.point,
        normal: p.normal,
        impact_speed: -normal_speed,
        scrape_speed: tangent_speed.abs(),
        kind: p.kind,
    })
}
