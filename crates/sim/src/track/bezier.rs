//! Cubic Bezier curves, and turning a loop of anchors into them.

use common::Vec2;

use super::def::AnchorDef;

#[derive(Clone, Copy, Debug)]
pub struct CubicBezier {
    pub p0: Vec2,
    pub p1: Vec2,
    pub p2: Vec2,
    pub p3: Vec2,
}

impl CubicBezier {
    pub fn point(&self, t: f64) -> Vec2 {
        let u = 1.0 - t;
        self.p0 * (u * u * u) + self.p1 * (3.0 * u * u * t) + self.p2 * (3.0 * u * t * t) + self.p3 * (t * t * t)
    }

    pub fn derivative(&self, t: f64) -> Vec2 {
        let u = 1.0 - t;
        (self.p1 - self.p0) * (3.0 * u * u) + (self.p2 - self.p1) * (6.0 * u * t) + (self.p3 - self.p2) * (3.0 * t * t)
    }
}

/// One curve per anchor, from it to the next (wrapping), in driving order.
///
/// An explicit handle is mirrored for the incoming side unless `handle_in` is
/// given too. Automatic handles point along the line from the previous anchor to the
/// next, each a third of its own section's length, which avoids overshoot
/// when anchors are unevenly spaced.
pub fn curves(anchors: &[AnchorDef]) -> Vec<CubicBezier> {
    let n = anchors.len();
    let pos = |i: usize| anchors[i % n].position();
    let handles: Vec<(Vec2, Vec2)> = (0..n)
        .map(|i| {
            let (prev, here, next) = (pos(i + n - 1), pos(i), pos(i + 1));
            match anchors[i].handle {
                Some([x, y]) => {
                    let out = Vec2::new(x, y);
                    let incoming = anchors[i].handle_in.map_or(-out, |[x, y]| Vec2::new(x, y));
                    (incoming, out)
                }
                None => {
                    let dir = (next - prev).normalize_or_zero();
                    (-dir * (here.distance(prev) / 3.0), dir * (here.distance(next) / 3.0))
                }
            }
        })
        .collect();
    (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            CubicBezier { p0: pos(i), p1: pos(i) + handles[i].1, p2: pos(j) + handles[j].0, p3: pos(j) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_tangents() {
        let c = CubicBezier {
            p0: Vec2::ZERO,
            p1: Vec2::new(1.0, 0.0),
            p2: Vec2::new(2.0, 1.0),
            p3: Vec2::new(3.0, 1.0),
        };
        assert_eq!(c.point(0.0), c.p0);
        assert_eq!(c.point(1.0), c.p3);
        assert_eq!(c.derivative(0.0), Vec2::new(3.0, 0.0));
        assert_eq!(c.derivative(1.0), Vec2::new(3.0, 0.0));
    }
}
