//! Resampling the Bezier loop into evenly spaced centreline samples.

use common::Vec2;
use common::math::angle_diff;

use super::bezier::CubicBezier;

/// Target spacing between samples, metres. The actual spacing is adjusted so
/// the loop closes evenly.
pub const SAMPLE_SPACING: f64 = 1.0;
/// Subdivisions per curve when measuring arc length.
const LENGTH_STEPS: usize = 200;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub position: Vec2,
    /// Unit vector in the driving direction.
    pub tangent: Vec2,
    /// Distance along the track from the start, metres.
    pub distance: f64,
    /// 1 / radius, positive when the track turns left.
    pub curvature: f64,
    /// Index of the anchor whose section this sample is in.
    pub section: usize,
}

impl Sample {
    /// Unit vector to the left of the driving direction.
    pub fn normal(&self) -> Vec2 {
        self.tangent.perp()
    }
}

/// Evenly spaced samples around the loop, and the loop's length.
pub fn sample(curves: &[CubicBezier]) -> (Vec<Sample>, f64) {
    // Arc-length table: (cumulative length, curve index, t).
    let mut table = vec![(0.0, 0, 0.0)];
    let mut length = 0.0;
    for (index, curve) in curves.iter().enumerate() {
        let mut previous = curve.point(0.0);
        for step in 1..=LENGTH_STEPS {
            let t = step as f64 / LENGTH_STEPS as f64;
            let point = curve.point(t);
            length += point.distance(previous);
            previous = point;
            table.push((length, index, t));
        }
    }

    let count = ((length / SAMPLE_SPACING).round() as usize).max(3);
    let spacing = length / count as f64;
    let mut samples = Vec::with_capacity(count);
    let mut cursor = 0;
    for k in 0..count {
        let distance = k as f64 * spacing;
        while table[cursor + 1].0 < distance {
            cursor += 1;
        }
        let (d0, c0, t0) = table[cursor];
        let (d1, c1, t1) = table[cursor + 1];
        // A step that crosses into the next curve starts from that curve's t = 0.
        let t0 = if c0 == c1 { t0 } else { 0.0 };
        let f = if d1 > d0 { (distance - d0) / (d1 - d0) } else { 0.0 };
        let (curve, t) = (c1, t0 + (t1 - t0) * f);
        let bezier = &curves[curve];
        samples.push(Sample {
            position: bezier.point(t),
            tangent: bezier.derivative(t).normalize_or_zero(),
            distance,
            curvature: 0.0,
            section: curve,
        });
    }

    for i in 0..count {
        let prev = samples[(i + count - 1) % count].tangent.angle();
        let next = samples[(i + 1) % count].tangent.angle();
        samples[i].curvature = angle_diff(prev, next) / (2.0 * spacing);
    }
    (samples, length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::bezier::curves;
    use crate::track::def::{AnchorDef, EdgeKind};

    fn circle_anchors(radius: f64, count: usize) -> Vec<AnchorDef> {
        (0..count)
            .map(|i| {
                let a = i as f64 / count as f64 * std::f64::consts::TAU;
                AnchorDef {
                    pos: [radius * a.cos(), radius * a.sin()],
                    handle: None,
                    left: EdgeKind::Soft,
                    right: EdgeKind::Soft,
                }
            })
            .collect()
    }

    #[test]
    fn circle_has_right_length_and_curvature() {
        let radius = 50.0;
        let (samples, length) = sample(&curves(&circle_anchors(radius, 12)));
        let expected = std::f64::consts::TAU * radius;
        assert!((length - expected).abs() / expected < 0.01, "length {length}");
        for s in &samples {
            assert!((s.curvature - 1.0 / radius).abs() < 0.002, "curvature {}", s.curvature);
            assert!((s.position.length() - radius).abs() < 0.5);
        }
    }

    #[test]
    fn samples_are_evenly_spaced() {
        let (samples, length) = sample(&curves(&circle_anchors(40.0, 7)));
        let spacing = length / samples.len() as f64;
        for pair in samples.windows(2) {
            let gap = pair[0].position.distance(pair[1].position);
            assert!((gap - spacing).abs() < 0.02, "gap {gap} vs {spacing}");
        }
    }
}
