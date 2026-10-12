//! Lay out a track like a turtle: straights and arcs from the current pose,
//! then close the loop with the shortest path a car of a given turning
//! radius could drive back to the start (a Dubins path).

use std::f64::consts::TAU;

use common::Vec2;

use crate::track::def::{AnchorDef, EdgeKind};

/// Arcs get an anchor at least this often, radians (45 degrees).
const MAX_ARC_STEP: f64 = std::f64::consts::FRAC_PI_4;
/// Points closer than this to the previous one are merged, metres. Tiny
/// slivers between anchors (e.g. a 2 degree closing turn) bend the Bezier
/// sharply, so keep anchors well apart.
const MIN_SPACING: f64 = 6.0;

/// Turn, straight, turn back to the start. Turns in radians, positive left.
#[derive(Clone, Copy, Debug)]
pub struct ClosingPath {
    pub radius: f64,
    pub first: f64,
    pub straight: f64,
    pub second: f64,
}

impl ClosingPath {
    pub fn length(&self) -> f64 {
        self.radius * (self.first.abs() + self.second.abs()) + self.straight
    }
}

pub struct Turtle {
    pub position: Vec2,
    pub heading: f64,
    /// Every point laid so far, with the heading there.
    points: Vec<(Vec2, f64)>,
}

impl Default for Turtle {
    fn default() -> Self {
        Self { position: Vec2::ZERO, heading: 0.0, points: vec![(Vec2::ZERO, 0.0)] }
    }
}

impl Turtle {
    pub fn straight(&mut self, length: f64) {
        self.position += Vec2::from_angle(self.heading) * length;
        self.mark();
    }

    /// Arc of `radius` through `angle` radians (positive turns left).
    pub fn arc(&mut self, radius: f64, angle: f64) {
        let steps = (angle.abs() / MAX_ARC_STEP).ceil().max(1.0);
        let step = angle / steps;
        let side = angle.signum();
        for _ in 0..steps as usize {
            let centre = self.position + Vec2::from_angle(self.heading).perp() * (radius * side);
            self.position = centre + (self.position - centre).rotate(step);
            self.heading += step;
            self.mark();
        }
    }

    fn mark(&mut self) {
        let (last, _) = self.points[self.points.len() - 1];
        if last.distance(self.position) >= MIN_SPACING {
            self.points.push((self.position, self.heading));
        }
    }

    /// A point `lead` metres behind the start, on its line: heading for it
    /// arrives lined up with the start rather than at an awkward angle.
    pub fn gate(&self, lead: f64) -> Vec2 {
        let (start, heading) = self.points[0];
        start - Vec2::from_angle(heading) * lead
    }

    pub fn distance_to(&self, point: Vec2) -> f64 {
        self.position.distance(point)
    }

    /// Angle from the current heading to `point`, radians (positive = left).
    pub fn bearing_to(&self, point: Vec2) -> f64 {
        common::math::angle_diff(self.heading, (point - self.position).angle())
    }

    /// Which way to turn to face `point`: +1 left, -1 right.
    pub fn towards(&self, point: Vec2) -> f64 {
        if self.bearing_to(point) >= 0.0 { 1.0 } else { -1.0 }
    }

    /// Every turn-straight-turn path back to the start pose with turns of
    /// `radius` (up to four), without laying any.
    pub fn closing_paths(&self, radius: f64) -> Vec<ClosingPath> {
        let (goal, goal_heading) = self.points[0];
        dubins(self.position, self.heading, goal, goal_heading, radius)
            .into_iter()
            .map(|(first, straight, second)| ClosingPath { radius, first, straight, second })
            .collect()
    }

    /// Drive back to the start pose along the shortest closing path. Returns
    /// false if no such path exists.
    pub fn close(&mut self, radius: f64) -> bool {
        let shortest = self.closing_paths(radius).into_iter().min_by(|a, b| a.length().total_cmp(&b.length()));
        match shortest {
            Some(path) => {
                self.lay_closing(&path);
                true
            }
            None => false,
        }
    }

    pub fn lay_closing(&mut self, path: &ClosingPath) {
        let ClosingPath { radius, first, straight, second } = *path;
        if first.abs() > 1e-6 {
            self.arc(radius, first);
        }
        if straight > 1e-6 {
            self.straight(straight);
        }
        if second.abs() > 1e-6 {
            self.arc(radius, second);
        }
    }

    /// Anchors for a closed track through the laid points. Handles follow the
    /// heading, each a third of its own side's chord: close to a true arc, and
    /// no kinks where a short piece meets a long one.
    pub fn anchors(&self) -> Vec<AnchorDef> {
        let mut points = self.points.clone();
        // After closing, the last point lands back on the first.
        if points.len() > 1 && points[points.len() - 1].0.distance(points[0].0) < MIN_SPACING {
            points.pop();
        }
        let n = points.len();
        (0..n)
            .map(|i| {
                let (here, heading) = points[i];
                let prev = points[(i + n - 1) % n].0;
                let next = points[(i + 1) % n].0;
                let dir = Vec2::from_angle(heading);
                let out = dir * (here.distance(next) / 3.0);
                let incoming = dir * (-here.distance(prev) / 3.0);
                AnchorDef {
                    pos: [here.x, here.y],
                    handle: Some([out.x, out.y]),
                    handle_in: Some([incoming.x, incoming.y]),
                    left: EdgeKind::Soft,
                    right: EdgeKind::Soft,
                }
            })
            .collect()
    }
}

/// The turn-straight-turn paths from one pose to another (left-left,
/// right-right, left-right, right-left; the mixed ones need room). Each is
/// the two turn angles (positive = left) and the straight length.
fn dubins(from: Vec2, from_heading: f64, to: Vec2, to_heading: f64, radius: f64) -> Vec<(f64, f64, f64)> {
    let left = |h: f64| Vec2::from_angle(h).perp();
    let mut paths = vec![];
    for (s1, s2) in [(1.0, 1.0), (-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0)] {
        let c1 = from + left(from_heading) * (radius * s1);
        let c2 = to + left(to_heading) * (radius * s2);
        let v = c2 - c1;
        let d = v.length();
        let psi = v.angle();
        let (phi, straight) = if s1 == s2 {
            (psi, d)
        } else {
            if d < 2.0 * radius {
                continue;
            }
            let delta = (2.0 * radius / d).asin();
            (psi + s1 * delta, (d * d - 4.0 * radius * radius).sqrt())
        };
        let turn = |from: f64, to: f64, s: f64| s * (s * (to - from)).rem_euclid(TAU);
        paths.push((turn(from_heading, phi, s1), straight, turn(phi, to_heading, s2)));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_turns_and_moves_correctly() {
        let mut t = Turtle::default();
        t.arc(10.0, std::f64::consts::FRAC_PI_2);
        assert!((t.position - Vec2::new(10.0, 10.0)).length() < 1e-9);
        assert!((t.heading - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn close_returns_to_the_start_pose() {
        for (straight, turn) in [(100.0, 1.0), (50.0, -2.5), (300.0, 0.3), (20.0, 3.0)] {
            let mut t = Turtle::default();
            t.straight(straight);
            t.arc(40.0, turn);
            t.straight(60.0);
            assert!(t.close(30.0));
            assert!(t.position.length() < 1e-6, "ended at {:?}", t.position);
            let heading = common::math::wrap_angle(t.heading);
            assert!(heading.abs() < 1e-6, "heading {heading}");
        }
    }
}
