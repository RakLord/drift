//! Summary numbers for a whole track, for picking and comparing tracks.

use super::{EdgeKind, Side, Track};

/// A corner turning at least this much counts as a hairpin, radians.
pub const HAIRPIN_ANGLE: f64 = 150.0 * std::f64::consts::PI / 180.0;

#[derive(Clone, Copy, Debug)]
pub struct TrackStats {
    /// Metres.
    pub length: f64,
    pub corners: usize,
    pub corners_per_km: f64,
    /// Radius of the tightest corner's apex, metres (`None` with no corners).
    pub sharpest: Option<f64>,
    pub hairpins: usize,
    /// Figure-8 crossroads.
    pub crossings: usize,
    /// Share of both edges, by length, that is a hard wall (0..1).
    pub hard_share: f64,
}

impl TrackStats {
    pub fn of(track: &Track) -> Self {
        let n = track.samples.len();
        let hard = (0..n)
            .flat_map(|i| [Side::Left, Side::Right].map(|side| track.edge(i, side)))
            .filter(|&e| e == EdgeKind::Hard)
            .count();
        Self {
            length: track.length,
            corners: track.corners.len(),
            corners_per_km: track.corners.len() as f64 / (track.length / 1000.0).max(1e-9),
            sharpest: track.corners.iter().map(|c| c.min_radius).min_by(f64::total_cmp),
            hairpins: track.corners.iter().filter(|c| c.angle >= HAIRPIN_ANGLE).count(),
            crossings: track.crossings.len(),
            hard_share: hard as f64 / (2 * n).max(1) as f64,
        }
    }
}
