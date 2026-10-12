//! Driver prep: before a run the track is analysed into corners (see
//! `track::corners`); while driving, the car knows the next few of them
//! before its rays can see them, like a driver who has walked the course.

use crate::track::{Corner, Track};

/// Corners described ahead of the car.
pub const CORNERS_AHEAD: usize = 3;
/// Values per corner in `features`.
pub const FEATURES_PER_CORNER: usize = 8;

/// Normalisation references.
const DISTANCE_REF: f64 = 150.0;
const RADIUS_REF: f64 = 30.0;

/// One upcoming corner, relative to the car.
#[derive(Clone, Copy, Debug, Default)]
pub struct CornerNote {
    /// Metres until the corner starts; 0 once inside it.
    pub distance_to: f64,
    /// 0..1 through the corner; 0 if not yet in it.
    pub progress: f64,
    /// +1 left, -1 right.
    pub direction: f64,
    /// Total angle, radians.
    pub angle: f64,
    /// Apex radius, metres.
    pub min_radius: f64,
    /// Clip position as a fraction through the corner, if it has one.
    pub clip: Option<f64>,
    pub outer_coverage: f64,
    pub inside_hard: bool,
    pub outside_hard: bool,
}

/// The next corners from `distance` along the track: the one the car is in
/// (if any) first, then the ones ahead, wrapping round the lap.
pub fn upcoming(track: &Track, distance: f64) -> [Option<CornerNote>; CORNERS_AHEAD] {
    let mut notes = [None; CORNERS_AHEAD];
    let corners = &track.corners;
    if corners.is_empty() {
        return notes;
    }
    // The corner we're inside, else the next one to start, else wrap round.
    let first = corners
        .iter()
        .position(|c| track.in_range(distance, c.start, c.end))
        .or_else(|| corners.iter().position(|c| c.start >= distance))
        .unwrap_or(0);
    for (k, note) in notes.iter_mut().enumerate().take(corners.len()) {
        let c = &corners[(first + k) % corners.len()];
        *note = Some(describe(track, c, distance));
    }
    notes
}

fn describe(track: &Track, c: &Corner, distance: f64) -> CornerNote {
    let inside = track.in_range(distance, c.start, c.end);
    let length = c.length(track.length).max(1.0);
    CornerNote {
        distance_to: if inside { 0.0 } else { (c.start - distance).rem_euclid(track.length) },
        progress: if inside { ((distance - c.start).rem_euclid(track.length) / length).clamp(0.0, 1.0) } else { 0.0 },
        direction: c.direction,
        angle: c.angle,
        min_radius: c.min_radius,
        clip: c.clip,
        outer_coverage: c.outer_coverage,
        inside_hard: c.inside_hard,
        outside_hard: c.outside_hard,
    }
}

/// Network inputs for the corners ahead, each roughly in -1..1. Missing
/// corners (very simple tracks) are all zeros.
pub fn features(notes: &[Option<CornerNote>; CORNERS_AHEAD]) -> [f64; CORNERS_AHEAD * FEATURES_PER_CORNER] {
    let mut out = [0.0; CORNERS_AHEAD * FEATURES_PER_CORNER];
    for (slot, note) in out.chunks_exact_mut(FEATURES_PER_CORNER).zip(notes) {
        let Some(n) = note else { continue };
        let flag = |b: bool| if b { 1.0 } else { 0.0 };
        slot.copy_from_slice(&[
            (n.distance_to / DISTANCE_REF).min(1.0),
            n.progress,
            n.direction * (n.angle / std::f64::consts::PI).min(1.0),
            (RADIUS_REF / n.min_radius).min(1.0),
            n.clip.unwrap_or(-1.0),
            n.outer_coverage,
            flag(n.inside_hard),
            flag(n.outside_hard),
        ]);
    }
    out
}
