//! Judged zones resolved from section coordinates to track distances.

use common::Vec2;

use super::boundary::Side;
use super::centreline::Sample;
use super::def::{TrackDef, ZoneDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneKind {
    Clip,
    Outer,
}

#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub kind: ZoneKind,
    pub side: Side,
    /// Distance along the track where the zone starts (clips: the point).
    pub start: f64,
    /// Distance where it ends (clips: same as `start`).
    pub end: f64,
    /// Clips: the clipping point on the track edge.
    pub point: Vec2,
}

#[derive(Debug)]
pub struct ZoneError(pub String);

/// Resolve every zone in `def` against the sampled centreline.
pub fn resolve(def: &TrackDef, samples: &[Sample], length: f64) -> Result<Vec<Zone>, ZoneError> {
    let sections = def.anchors.len();
    // Distance at which each section starts.
    let mut starts = vec![f64::NAN; sections];
    for s in samples {
        if starts[s.section].is_nan() {
            starts[s.section] = s.distance;
        }
    }
    if let Some(i) = starts.iter().position(|d| d.is_nan()) {
        return Err(ZoneError(format!("section {i} is too short to hold a zone")));
    }
    let to_distance = |at: f64| -> Result<f64, ZoneError> {
        if !(0.0..sections as f64).contains(&at) {
            return Err(ZoneError(format!("zone position {at} is outside sections 0..{sections}")));
        }
        let index = at.floor() as usize;
        let start = starts[index];
        let end = if index + 1 < sections { starts[index + 1] } else { length };
        Ok(start + (end - start) * at.fract())
    };
    let spacing = length / samples.len() as f64;
    let edge_point = |distance: f64, side: Side| {
        let s = &samples[((distance / spacing).round() as usize) % samples.len()];
        s.position + s.normal() * (side.sign() * def.width / 2.0)
    };

    def.zones
        .iter()
        .map(|zone| match *zone {
            ZoneDef::Clip { at, side } => {
                let d = to_distance(at)?;
                Ok(Zone { kind: ZoneKind::Clip, side, start: d, end: d, point: edge_point(d, side) })
            }
            ZoneDef::Outer { from, to, side } => {
                let (start, end) = (to_distance(from)?, to_distance(to)?);
                Ok(Zone { kind: ZoneKind::Outer, side, start, end, point: edge_point(start, side) })
            }
        })
        .collect()
}
