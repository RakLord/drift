//! Track boundaries as line segments: walls, painted edges, verge fences.

use common::Vec2;

use super::centreline::Sample;
use super::def::EdgeKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    /// Barrier at the track edge. Solid.
    HardWall,
    /// Painted edge between asphalt and verge. Not solid; rays see it.
    SoftEdge,
    /// Fence at the back of a verge. Solid.
    Fence,
    /// Short wall across the verge where a hard section meets a soft one, so
    /// the verge is closed off. Solid.
    Cap,
}

impl SegmentKind {
    pub fn is_solid(self) -> bool {
        self != Self::SoftEdge
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub a: Vec2,
    pub b: Vec2,
    /// Unit normal pointing to the side a car should be on.
    pub normal: Vec2,
    pub kind: SegmentKind,
}

/// Which side of the driving direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// +1 for left (the sample normal's direction), -1 for right.
    pub fn sign(self) -> f64 {
        match self {
            Self::Left => 1.0,
            Self::Right => -1.0,
        }
    }
}

/// Build every boundary segment. `edge(i, side)` gives the edge kind at
/// sample `i`.
pub fn build(samples: &[Sample], half_width: f64, verge: f64, edge: impl Fn(usize, Side) -> EdgeKind) -> Vec<Segment> {
    let n = samples.len();
    let mut segments = Vec::with_capacity(n * 3);
    for side in [Side::Left, Side::Right] {
        let offset_point = |i: usize, offset: f64| samples[i].position + samples[i].normal() * (side.sign() * offset);
        // Facing back towards the centreline.
        let inward = |i: usize| samples[i].normal() * -side.sign();
        for i in 0..n {
            let j = (i + 1) % n;
            let normal = (inward(i) + inward(j)).normalize_or_zero();
            let edge_a = offset_point(i, half_width);
            let edge_b = offset_point(j, half_width);
            match edge(i, side) {
                EdgeKind::Hard => {
                    segments.push(Segment { a: edge_a, b: edge_b, normal, kind: SegmentKind::HardWall });
                }
                EdgeKind::Soft => {
                    segments.push(Segment { a: edge_a, b: edge_b, normal, kind: SegmentKind::SoftEdge });
                    let fence_a = offset_point(i, half_width + verge);
                    let fence_b = offset_point(j, half_width + verge);
                    segments.push(Segment { a: fence_a, b: fence_b, normal, kind: SegmentKind::Fence });
                }
            }
            // Close the verge where the edge kind changes at sample j.
            let (before, after) = (edge(i, side), edge(j, side));
            if before != after {
                // The verge lies ahead if it starts here, behind if it ends.
                let into_verge = if after == EdgeKind::Soft { samples[j].tangent } else { -samples[j].tangent };
                segments.push(Segment {
                    a: edge_b,
                    b: offset_point(j, half_width + verge),
                    normal: into_verge,
                    kind: SegmentKind::Cap,
                });
            }
        }
    }
    segments
}

/// Where two segments cross, if they do (excluding shared endpoints).
pub fn intersects(s: &Segment, t: &Segment) -> bool {
    let r = s.b - s.a;
    let q = t.b - t.a;
    let denom = r.cross(q);
    if denom.abs() < 1e-12 {
        return false;
    }
    let diff = t.a - s.a;
    let u = diff.cross(q) / denom;
    let v = diff.cross(r) / denom;
    const EPS: f64 = 1e-6;
    u > EPS && u < 1.0 - EPS && v > EPS && v < 1.0 - EPS
}
