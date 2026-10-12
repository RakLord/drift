//! Flat crossroads where the track crosses itself (figure-8s).
//!
//! Where two parts of the track cross, each part's boundaries (walls, painted
//! edges, fences) are opened up where they would sit on the other part's
//! asphalt or verge. What's left is the outline of the union of the two
//! roads, so a car can drive straight across.

use common::Vec2;

use super::boundary::{Segment, SegmentKind, Side};
use super::centreline::Sample;
use super::def::EdgeKind;
use super::grid::SegmentGrid;

/// Crossings shallower than this are rejected: the roads would overlap for a
/// long way rather than cross, radians.
pub const MIN_CROSSING_ANGLE: f64 = 35.0 * std::f64::consts::PI / 180.0;
/// Extra space required between parallel parts of the track, metres.
const CLEARANCE_MARGIN: f64 = 1.0;
/// Only parts of the track this many reaches apart (along the track) count as
/// different roads.
const OTHER_ROAD_REACHES: f64 = 3.0;

#[derive(Clone, Copy, Debug)]
pub struct Crossing {
    pub point: Vec2,
    /// Sample indices of the two passes through the crossing.
    pub first: usize,
    pub second: usize,
    /// Angle between the two roads, radians (0..90 degrees).
    pub angle: f64,
    /// Everything within this distance of `point` belongs to the crossroads.
    pub radius: f64,
}

/// The centreline as segments in a grid, for "is this near another part of
/// the track?" questions.
pub struct Centreline {
    segments: Vec<Segment>,
    grid: SegmentGrid,
    /// Samples closer than this along the track are the same road.
    same_road: usize,
}

impl Centreline {
    pub fn new(samples: &[Sample], length: f64, reach: f64) -> Self {
        let n = samples.len();
        let segments: Vec<Segment> = (0..n)
            .map(|i| Segment {
                a: samples[i].position,
                b: samples[(i + 1) % n].position,
                normal: samples[i].normal(),
                kind: SegmentKind::SoftEdge,
            })
            .collect();
        let spacing = length / n as f64;
        let same_road = (OTHER_ROAD_REACHES * reach / spacing).ceil() as usize;
        Self { grid: SegmentGrid::build(&segments), segments, same_road }
    }

    fn is_other_road(&self, a: usize, b: usize) -> bool {
        let n = self.segments.len();
        let gap = a.abs_diff(b);
        gap.min(n - gap) > self.same_road
    }

    /// Segments of other roads (relative to sample `from`) within `radius`
    /// of `point`: (segment index, distance, side of that road the point is on).
    fn others_near(&self, point: Vec2, radius: f64, from: usize, scratch: &mut Vec<u32>) -> Vec<(usize, f64, Side)> {
        let r = Vec2::new(radius, radius);
        self.grid.query(point - r, point + r, scratch);
        scratch
            .iter()
            .map(|&j| j as usize)
            .filter(|&j| self.is_other_road(from, j))
            .filter_map(|j| {
                let s = &self.segments[j];
                let along = s.b - s.a;
                let t = ((point - s.a).dot(along) / along.length_sq()).clamp(0.0, 1.0);
                let distance = point.distance(s.a + along * t);
                let side = if along.cross(point - s.a) > 0.0 { Side::Left } else { Side::Right };
                (distance < radius).then_some((j, distance, side))
            })
            .collect()
    }

    /// Whether `point` is on the asphalt of a road other than the one at
    /// sample `from`.
    pub fn on_other_asphalt(&self, point: Vec2, from: usize, half_width: f64) -> bool {
        !self.others_near(point, half_width, from, &mut vec![]).is_empty()
    }
}

/// Every place the centreline crosses itself.
pub fn find(samples: &[Sample], centre: &Centreline, reach: f64) -> Result<Vec<Crossing>, (Vec2, f64)> {
    let n = samples.len();
    let mut crossings: Vec<Crossing> = vec![];
    let mut scratch = vec![];
    for i in 0..n {
        let s = &centre.segments[i];
        let lo = Vec2::new(s.a.x.min(s.b.x), s.a.y.min(s.b.y));
        let hi = Vec2::new(s.a.x.max(s.b.x), s.a.y.max(s.b.y));
        centre.grid.query(lo, hi, &mut scratch);
        for &j in &scratch {
            let j = j as usize;
            if j <= i || !centre.is_other_road(i, j) {
                continue;
            }
            let Some(point) = intersection(s, &centre.segments[j]) else { continue };
            let sin = samples[i].tangent.cross(samples[j].tangent).abs();
            let angle = sin.clamp(0.0, 1.0).asin();
            if angle < MIN_CROSSING_ANGLE {
                return Err((point, angle));
            }
            // Out to where the other road's outline meets this one's.
            let radius = reach / angle.sin() + reach + 5.0;
            crossings.push(Crossing { point, first: i, second: j, angle, radius });
        }
    }
    Ok(crossings)
}

/// Parts of the track that aren't crossing each other must stay far enough
/// apart that their verges don't overlap. Returns where they get too close.
pub fn check_clearance(samples: &[Sample], centre: &Centreline, crossings: &[Crossing], reach: f64) -> Result<(), Vec2> {
    let needed = 2.0 * reach + CLEARANCE_MARGIN;
    let mut scratch = vec![];
    for (i, s) in samples.iter().enumerate() {
        if near_crossing(s.position, crossings) {
            continue;
        }
        if !centre.others_near(s.position, needed, i, &mut scratch).is_empty() {
            return Err(s.position);
        }
    }
    Ok(())
}

pub fn near_crossing(point: Vec2, crossings: &[Crossing]) -> bool {
    crossings.iter().any(|c| point.distance(c.point) < c.radius)
}

/// Per sample and side (left, right): whether the edge point and the fence
/// point fall inside another road's asphalt or verge.
pub fn open_points(
    samples: &[Sample],
    centre: &Centreline,
    half_width: f64,
    verge: f64,
    edge: impl Fn(usize, Side) -> EdgeKind,
) -> (Vec<[bool; 2]>, Vec<[bool; 2]>) {
    let reach_of = |j: usize, side: Side| half_width + if edge(j, side) == EdgeKind::Soft { verge } else { 0.0 };
    let mut scratch = vec![];
    let mut inside_other = |point: Vec2, from: usize| {
        centre
            .others_near(point, half_width + verge, from, &mut scratch)
            .into_iter()
            .any(|(j, distance, side)| distance < reach_of(j, side))
    };
    let mut edges = vec![[false; 2]; samples.len()];
    let mut fences = vec![[false; 2]; samples.len()];
    for (i, s) in samples.iter().enumerate() {
        for (k, side) in [Side::Left, Side::Right].into_iter().enumerate() {
            let normal = s.normal() * side.sign();
            edges[i][k] = inside_other(s.position + normal * half_width, i);
            fences[i][k] = inside_other(s.position + normal * (half_width + verge), i);
        }
    }
    (edges, fences)
}

fn intersection(s: &Segment, t: &Segment) -> Option<Vec2> {
    let r = s.b - s.a;
    let q = t.b - t.a;
    let denom = r.cross(q);
    if denom.abs() < 1e-12 {
        return None;
    }
    let diff = t.a - s.a;
    let u = diff.cross(q) / denom;
    let v = diff.cross(r) / denom;
    ((0.0..1.0).contains(&u) && (0.0..1.0).contains(&v)).then(|| s.a + r * u)
}
