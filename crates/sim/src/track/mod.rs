//! Tracks: a closed Bezier loop with a width, edges that are hard (barriers)
//! or soft (grass verge + fence), sampled into a centreline and boundary
//! segments for projection, raycasts and collision.

pub mod bezier;
pub mod boundary;
pub mod centreline;
pub mod corners;
pub mod crossings;
pub mod def;
pub mod generator;
pub mod grid;
pub mod zones;

use std::fmt;

use common::Vec2;
use common::math::angle_diff;

use crate::car::Ground;
pub use boundary::{Segment, SegmentKind, Side};
use centreline::Sample;
pub use def::{EdgeKind, TrackDef};
use def::FORMAT_VERSION;
pub use corners::Corner;
pub use crossings::Crossing;
use crossings::Centreline;
pub use grid::RayHit;
pub use zones::{Zone, ZoneKind};
use grid::SegmentGrid;

/// Grass verge: much less grip, and it drags.
pub const VERGE: Ground = Ground { grip: 0.6, rolling_resistance: 6.0 };

/// Keep curves this much looser than the distance from the centreline to the
/// boundary on the inside of the turn, or the boundary would fold over itself.
const RADIUS_MARGIN: f64 = 1.0;

pub struct Track {
    pub def: TrackDef,
    pub samples: Vec<Sample>,
    pub length: f64,
    pub segments: Vec<Segment>,
    pub zones: Vec<Zone>,
    /// Corners in driving order, annotated with their zones and edges.
    pub corners: Vec<Corner>,
    /// Flat crossroads where the track crosses itself.
    pub crossings: Vec<Crossing>,
    /// Per sample, `[left, right]`: boundary points that lie on another road
    /// at a crossing (see `edge_present` / `fence_present`).
    edge_open: Vec<[bool; 2]>,
    fence_open: Vec<[bool; 2]>,
    centre: Centreline,
    section_starts: Vec<f64>,
    grid: SegmentGrid,
}

/// Where something is relative to the track.
#[derive(Clone, Copy, Debug)]
pub struct TrackPoint {
    /// Nearest sample index (use as the hint for the next projection).
    pub index: usize,
    /// Distance along the track, metres, in [0, length).
    pub distance: f64,
    /// Sideways offset from the centreline, metres. Positive = left.
    pub offset: f64,
    /// Track direction here, unit vector.
    pub tangent: Vec2,
}

#[derive(Debug)]
pub enum TrackError {
    Version(u32),
    TooFewAnchors,
    TooTight { distance: f64, radius: f64, needed: f64 },
    SelfIntersecting { at: Vec2 },
    ShallowCrossing { at: Vec2, angle: f64 },
    TooClose { at: Vec2 },
    Zone(String),
}

impl fmt::Display for TrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version(v) => write!(f, "track format version {v}, expected {FORMAT_VERSION}"),
            Self::TooFewAnchors => write!(f, "a track needs at least 3 anchors"),
            Self::TooTight { distance, radius, needed } => {
                write!(f, "corner at {distance:.0} m has radius {radius:.1} m, needs at least {needed:.1} m")
            }
            Self::SelfIntersecting { at } => write!(f, "boundaries cross near ({:.0}, {:.0})", at.x, at.y),
            Self::ShallowCrossing { at, angle } => write!(
                f,
                "track crosses itself at only {:.0} degrees near ({:.0}, {:.0}), needs at least {:.0}",
                angle.to_degrees(),
                at.x,
                at.y,
                crossings::MIN_CROSSING_ANGLE.to_degrees()
            ),
            Self::TooClose { at } => write!(f, "two parts of the track run too close near ({:.0}, {:.0})", at.x, at.y),
            Self::Zone(message) => write!(f, "zone: {message}"),
        }
    }
}

impl std::error::Error for TrackError {}

impl Track {
    pub fn build(def: TrackDef) -> Result<Self, TrackError> {
        if def.version != FORMAT_VERSION {
            return Err(TrackError::Version(def.version));
        }
        if def.anchors.len() < 3 {
            return Err(TrackError::TooFewAnchors);
        }
        let (samples, length) = centreline::sample(&bezier::curves(&def.anchors));
        let half_width = def.width / 2.0;
        let edge = |i: usize, side: Side| edge_kind(&def, &samples[i], side);

        for (i, s) in samples.iter().enumerate() {
            let inside = if s.curvature > 0.0 { Side::Left } else { Side::Right };
            let reach = half_width + if edge(i, inside) == EdgeKind::Soft { def.verge } else { 0.0 };
            let radius = 1.0 / s.curvature.abs().max(1e-9);
            if radius < reach + RADIUS_MARGIN {
                return Err(TrackError::TooTight { distance: s.distance, radius, needed: reach + RADIUS_MARGIN });
            }
        }

        let reach = half_width + def.verge;
        let centre = Centreline::new(&samples, length, reach);
        let crossings = crossings::find(&samples, &centre, reach)
            .map_err(|(at, angle)| TrackError::ShallowCrossing { at, angle })?;
        crossings::check_clearance(&samples, &centre, &crossings, reach).map_err(|at| TrackError::TooClose { at })?;
        let (edge_open, fence_open) = crossings::open_points(&samples, &centre, half_width, def.verge, edge);

        let segments = boundary::build(&samples, half_width, def.verge, edge, &edge_open, &fence_open);
        let grid = SegmentGrid::build(&segments);
        let zones = zones::resolve(&def, &samples, length).map_err(|e| TrackError::Zone(e.0))?;
        let section_starts = zones::section_starts(&samples, def.anchors.len()).map_err(|e| TrackError::Zone(e.0))?;
        let corners = corners::find(&samples, length);
        let mut track = Self {
            def,
            samples,
            length,
            segments,
            zones,
            corners,
            crossings,
            edge_open,
            fence_open,
            centre,
            section_starts,
            grid,
        };
        track.check_intersections()?;
        track.annotate_corners();
        Ok(track)
    }

    pub fn half_width(&self) -> f64 {
        self.def.width / 2.0
    }

    /// Edge kind on one side at a given sample.
    pub fn edge(&self, index: usize, side: Side) -> EdgeKind {
        edge_kind(&self.def, &self.samples[index], side)
    }

    /// Start of the track: position and heading.
    pub fn start(&self) -> (Vec2, f64) {
        let s = &self.samples[0];
        (s.position, s.tangent.angle())
    }

    /// Project a point onto the centreline. With a `hint` (the previous
    /// result's index) this searches locally, which is fast and never jumps to
    /// a different part of the track that happens to be close by.
    pub fn project(&self, point: Vec2, hint: Option<usize>) -> TrackPoint {
        let n = self.samples.len();
        let dist_sq = |i: usize| (self.samples[i].position - point).length_sq();
        let mut best = match hint {
            Some(start) => {
                let mut best = start % n;
                loop {
                    let forward = (best + 1) % n;
                    let back = (best + n - 1) % n;
                    let next = if dist_sq(forward) < dist_sq(back) { forward } else { back };
                    if dist_sq(next) >= dist_sq(best) {
                        break best;
                    }
                    best = next;
                }
            }
            None => (0..n).min_by(|&a, &b| dist_sq(a).total_cmp(&dist_sq(b))).unwrap_or(0),
        };

        // Refine along whichever neighbouring segment the point lies over.
        let here = &self.samples[best];
        let along = (point - here.position).dot(here.tangent);
        let spacing = self.length / n as f64;
        let mut distance = here.distance + along;
        if along.abs() > spacing / 2.0 {
            best = if along > 0.0 { (best + 1) % n } else { (best + n - 1) % n };
        }
        distance = distance.rem_euclid(self.length);
        let nearest = &self.samples[best];
        TrackPoint { index: best, distance, offset: (point - nearest.position).dot(nearest.normal()), tangent: nearest.tangent }
    }

    /// Whether the painted edge / barrier between samples `i` and `i + 1`
    /// on `side` exists (it's left out where it would sit on another road at
    /// a crossing).
    pub fn edge_present(&self, i: usize, side: Side) -> bool {
        let (j, k) = ((i + 1) % self.samples.len(), side_index(side));
        !(self.edge_open[i][k] && self.edge_open[j][k])
    }

    /// Like `edge_present`, for the fence behind a verge.
    pub fn fence_present(&self, i: usize, side: Side) -> bool {
        let (j, k) = ((i + 1) % self.samples.len(), side_index(side));
        !(self.fence_open[i][k] && self.fence_open[j][k])
    }

    /// What a tyre at `point` is standing on.
    pub fn ground_at(&self, point: Vec2, hint: Option<usize>) -> Ground {
        let p = self.project(point, hint);
        if p.offset.abs() <= self.half_width() || self.centre.on_other_asphalt(point, p.index, self.half_width()) {
            return Ground::ASPHALT;
        }
        let side = if p.offset > 0.0 { Side::Left } else { Side::Right };
        match self.edge(p.index, side) {
            EdgeKind::Soft => VERGE,
            EdgeKind::Hard => Ground::ASPHALT,
        }
    }

    /// Nearest boundary (any kind) along a ray. `direction` must be unit length.
    pub fn raycast(&self, origin: Vec2, direction: Vec2, max_distance: f64) -> Option<RayHit> {
        self.grid.raycast(&self.segments, origin, direction, max_distance, |_| true)
    }

    /// Indices of solid segments near a box, into `out`.
    pub fn solid_near(&self, lo: Vec2, hi: Vec2, out: &mut Vec<u32>) {
        self.grid.query(lo, hi, out);
        out.retain(|&i| self.segments[i as usize].kind.is_solid());
    }

    /// Signed change in distance along the track from `from` to `to`,
    /// handling the wrap at the start line.
    pub fn delta(&self, from: f64, to: f64) -> f64 {
        let d = to - from;
        let half = self.length / 2.0;
        if d > half {
            d - self.length
        } else if d < -half {
            d + self.length
        } else {
            d
        }
    }

    /// Whether `distance` lies between `start` and `end` going forwards,
    /// handling ranges that wrap past the start line.
    pub fn in_range(&self, distance: f64, start: f64, end: f64) -> bool {
        if start <= end { (start..=end).contains(&distance) } else { distance >= start || distance <= end }
    }

    /// Heading of the track at a sample, radians.
    pub fn heading_at(&self, index: usize) -> f64 {
        self.samples[index].tangent.angle()
    }

    /// Signed angle from the track direction to `heading`.
    pub fn heading_error(&self, index: usize, heading: f64) -> f64 {
        angle_diff(self.heading_at(index), heading)
    }

    /// Section coordinate (anchor index + fraction) of a track distance.
    pub fn section_coordinate(&self, distance: f64) -> f64 {
        zones::section_coordinate(&self.section_starts, self.length, distance)
    }

    /// Fill in each corner's clip, outer zone coverage and wall sides.
    fn annotate_corners(&mut self) {
        let n = self.samples.len();
        let spacing = self.length / n as f64;
        for i in 0..self.corners.len() {
            let c = self.corners[i];
            let inside = if c.direction > 0.0 { Side::Left } else { Side::Right };
            let outside = if inside == Side::Left { Side::Right } else { Side::Left };
            let span = (c.exit + n - c.entry) % n + 1;
            let indices = || (0..span).map(|k| (c.entry + k) % n);
            let hard_share =
                |side: Side| indices().filter(|&j| self.edge(j, side) == EdgeKind::Hard).count() as f64 / span as f64;
            let length = c.length(self.length).max(spacing);

            let clip = self
                .zones
                .iter()
                .filter(|z| z.kind == ZoneKind::Clip && z.side == inside && self.in_range(z.start, c.start, c.end))
                .map(|z| (z.start - c.start).rem_euclid(self.length) / length)
                .next();
            let covered = indices()
                .filter(|&j| {
                    let d = self.samples[j].distance;
                    self.zones
                        .iter()
                        .any(|z| z.kind == ZoneKind::Outer && z.side == outside && self.in_range(d, z.start, z.end))
                })
                .count();

            let (inside_hard, outside_hard) = (hard_share(inside) > 0.5, hard_share(outside) > 0.5);

            let corner = &mut self.corners[i];
            corner.clip = clip;
            corner.outer_coverage = covered as f64 / span as f64;
            corner.inside_hard = inside_hard;
            corner.outside_hard = outside_hard;
        }
    }

    /// Boundaries must never cross, except where they meet at a crossroads.
    fn check_intersections(&self) -> Result<(), TrackError> {
        let mut nearby = vec![];
        let at_crossing = |s: &Segment| crossings::near_crossing((s.a + s.b) / 2.0, &self.crossings);
        for (i, s) in self.segments.iter().enumerate() {
            if at_crossing(s) {
                continue;
            }
            let lo = Vec2::new(s.a.x.min(s.b.x), s.a.y.min(s.b.y));
            let hi = Vec2::new(s.a.x.max(s.b.x), s.a.y.max(s.b.y));
            self.grid.query(lo, hi, &mut nearby);
            for &j in &nearby {
                let j = j as usize;
                if j > i && !at_crossing(&self.segments[j]) && boundary::intersects(s, &self.segments[j]) {
                    return Err(TrackError::SelfIntersecting { at: s.a });
                }
            }
        }
        Ok(())
    }
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Left => 0,
        Side::Right => 1,
    }
}

fn edge_kind(def: &TrackDef, sample: &Sample, side: Side) -> EdgeKind {
    let anchor = &def.anchors[sample.section];
    match side {
        Side::Left => anchor.left,
        Side::Right => anchor.right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use def::AnchorDef;

    pub(crate) fn oval(left: EdgeKind, right: EdgeKind) -> TrackDef {
        // Two 100 m straights joined by radius-50 half circles (Bezier
        // quarter-circle handles are 0.5523 * radius).
        let h = 0.5523 * 50.0;
        let points = [
            ([0.0, 0.0], [h, 0.0]),
            ([100.0, 0.0], [h, 0.0]),
            ([150.0, 50.0], [0.0, h]),
            ([100.0, 100.0], [-h, 0.0]),
            ([0.0, 100.0], [-h, 0.0]),
            ([-50.0, 50.0], [0.0, -h]),
        ];
        TrackDef {
            version: FORMAT_VERSION,
            name: "oval".into(),
            width: 12.0,
            verge: 8.0,
            test: true,
            anchors: points.iter().map(|&(pos, handle)| AnchorDef { pos, handle: Some(handle), handle_in: None, left, right }).collect(),
            zones: vec![],
        }
    }

    #[test]
    fn oval_builds_and_projects() {
        let track = Track::build(oval(EdgeKind::Hard, EdgeKind::Soft)).unwrap();
        let p = track.project(Vec2::new(50.0, 3.0), None);
        assert!((p.offset - 3.0).abs() < 0.2, "offset {}", p.offset);
        assert!(p.tangent.x > 0.99);
        let hinted = track.project(Vec2::new(52.0, -2.0), Some(p.index));
        assert!((hinted.distance - p.distance - 2.0).abs() < 0.2);
    }

    #[test]
    fn ground_is_verge_only_past_a_soft_edge() {
        // Driving +x along y = 0: left is +y.
        let track = Track::build(oval(EdgeKind::Hard, EdgeKind::Soft)).unwrap();
        assert_eq!(track.ground_at(Vec2::new(50.0, 0.0), None), Ground::ASPHALT);
        assert_eq!(track.ground_at(Vec2::new(50.0, -9.0), None), VERGE);
        assert_eq!(track.ground_at(Vec2::new(50.0, 9.0), None), Ground::ASPHALT);
    }

    #[test]
    fn rays_hit_the_nearest_boundary() {
        let track = Track::build(oval(EdgeKind::Hard, EdgeKind::Soft)).unwrap();
        let up = track.raycast(Vec2::new(50.0, 0.0), Vec2::new(0.0, 1.0), 100.0).unwrap();
        assert!((up.distance - 6.0).abs() < 0.2 && up.kind == SegmentKind::HardWall, "{up:?}");
        let down = track.raycast(Vec2::new(50.0, 0.0), Vec2::new(0.0, -1.0), 100.0).unwrap();
        assert!((down.distance - 6.0).abs() < 0.2 && down.kind == SegmentKind::SoftEdge, "{down:?}");
        assert!(track.raycast(Vec2::new(50.0, 0.0), Vec2::new(0.0, 1.0), 3.0).is_none());
    }

    #[test]
    fn delta_wraps_across_the_start_line() {
        let track = Track::build(oval(EdgeKind::Soft, EdgeKind::Soft)).unwrap();
        let l = track.length;
        assert!((track.delta(l - 1.0, 1.0) - 2.0).abs() < 1e-9);
        assert!((track.delta(1.0, l - 1.0) + 2.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_corners_too_tight_for_the_verge() {
        let mut def = oval(EdgeKind::Soft, EdgeKind::Soft);
        def.width = 90.0; // half width + verge > the 50 m corner radius
        assert!(matches!(Track::build(def), Err(TrackError::TooTight { .. })));
    }

    #[test]
    fn closes_the_verge_where_edges_change() {
        let mut def = oval(EdgeKind::Soft, EdgeKind::Soft);
        def.anchors[2].left = EdgeKind::Hard;
        let track = Track::build(def).unwrap();
        let caps = track.segments.iter().filter(|s| s.kind == SegmentKind::Cap).count();
        assert_eq!(caps, 2);
    }
}
