//! Shared helpers for integration tests.

#![allow(dead_code)]

use std::sync::Arc;

use sim::track::def::{AnchorDef, ZoneDef};
use sim::track::{EdgeKind, Track, TrackDef};

pub const DT: f64 = 1.0 / 120.0;
pub const KMH: f64 = 1.0 / 3.6;

/// Two 200 m straights joined by radius-60 half circles, 12 m wide. Section 0
/// is the straight along y = 0 driving +x, so left is +y.
pub fn oval(left: EdgeKind, right: EdgeKind, zones: Vec<ZoneDef>) -> Arc<Track> {
    let h = 0.5523 * 60.0;
    let points = [
        ([0.0, 0.0], [h, 0.0]),
        ([200.0, 0.0], [h, 0.0]),
        ([260.0, 60.0], [0.0, h]),
        ([200.0, 120.0], [-h, 0.0]),
        ([0.0, 120.0], [-h, 0.0]),
        ([-60.0, 60.0], [0.0, -h]),
    ];
    let def = TrackDef {
        version: 1,
        name: "test oval".into(),
        width: 12.0,
        verge: 8.0,
        test: true,
        anchors: points.iter().map(|&(pos, handle)| AnchorDef { pos, handle: Some(handle), handle_in: None, left, right }).collect(),
        zones,
    };
    Arc::new(Track::build(def).unwrap())
}
