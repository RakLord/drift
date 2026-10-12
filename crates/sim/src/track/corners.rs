//! Finding corners in a sampled centreline: runs of curvature in one
//! direction that turn far enough to matter.

use super::centreline::Sample;

/// Tighter than this radius counts as cornering, metres.
const CORNERING_RADIUS: f64 = 200.0;
/// Corners turning less than this in total are ignored, radians.
const MIN_ANGLE: f64 = 25.0 * std::f64::consts::PI / 180.0;
/// Same-direction runs separated by less than this are one corner, metres.
const MERGE_GAP: f64 = 12.0;
/// Curvature is averaged over this many samples either side.
const SMOOTHING: usize = 5;

#[derive(Clone, Copy, Debug)]
pub struct Corner {
    /// Sample indices of entry, apex (tightest point) and exit.
    pub entry: usize,
    pub apex: usize,
    pub exit: usize,
    /// Track distances of entry and exit. `exit < entry` if the corner
    /// wraps past the start line.
    pub start: f64,
    pub end: f64,
    /// +1 turning left, -1 turning right.
    pub direction: f64,
    /// Total angle turned, radians (positive).
    pub angle: f64,
    /// Radius at the apex, metres.
    pub min_radius: f64,
    /// Where a clip on the inside sits, as a fraction through the corner.
    pub clip: Option<f64>,
    /// Fraction of the corner with an outer zone on the outside.
    pub outer_coverage: f64,
    /// Whether most of the inside / outside edge is a hard wall.
    pub inside_hard: bool,
    pub outside_hard: bool,
}

impl Corner {
    pub fn length(&self, track_length: f64) -> f64 {
        (self.end - self.start).rem_euclid(track_length)
    }
}

pub fn find(samples: &[Sample], length: f64) -> Vec<Corner> {
    let n = samples.len();
    let spacing = length / n as f64;
    let curvature: Vec<f64> = (0..n)
        .map(|i| {
            let sum: f64 = (0..=2 * SMOOTHING).map(|k| samples[(i + n + k - SMOOTHING) % n].curvature).sum();
            sum / (2 * SMOOTHING + 1) as f64
        })
        .collect();
    let threshold = 1.0 / CORNERING_RADIUS;
    let sign = |i: usize| {
        let k = curvature[i % n];
        if k > threshold {
            1
        } else if k < -threshold {
            -1
        } else {
            0
        }
    };

    // Start scanning on a straight so no corner is split across index 0.
    let Some(origin) = (0..n).find(|&i| sign(i) == 0) else {
        return vec![]; // A loop that never straightens (a circle) has no corners.
    };

    // Runs of constant non-zero sign, as (first, last) offsets from origin.
    let mut runs: Vec<(usize, usize, i32)> = vec![];
    let mut k = 0;
    while k < n {
        let s = sign(origin + k);
        if s == 0 {
            k += 1;
            continue;
        }
        let first = k;
        while k + 1 < n && sign(origin + k + 1) == s {
            k += 1;
        }
        runs.push((first, k, s));
        k += 1;
    }

    // Merge same-direction runs with short gaps between them.
    let max_gap = (MERGE_GAP / spacing).ceil() as usize;
    let mut merged: Vec<(usize, usize, i32)> = vec![];
    for run in runs {
        match merged.last_mut() {
            Some(last) if last.2 == run.2 && run.0 - last.1 <= max_gap => last.1 = run.1,
            _ => merged.push(run),
        }
    }

    merged
        .into_iter()
        .filter_map(|(first, last, s)| {
            let indices = (first..=last).map(|o| (origin + o) % n);
            let angle: f64 = indices.clone().map(|i| curvature[i].abs() * spacing).sum();
            if angle < MIN_ANGLE {
                return None;
            }
            let apex = indices.max_by(|&a, &b| curvature[a].abs().total_cmp(&curvature[b].abs()))?;
            let (entry, exit) = ((origin + first) % n, (origin + last) % n);
            Some(Corner {
                entry,
                apex,
                exit,
                start: samples[entry].distance,
                end: samples[exit].distance,
                direction: f64::from(s),
                angle,
                min_radius: 1.0 / curvature[apex].abs(),
                clip: None,
                outer_coverage: 0.0,
                inside_hard: false,
                outside_hard: false,
            })
        })
        .collect()
}
