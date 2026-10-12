//! Figure-8 tracks: crossings are found, opened up, and drivable.

mod support;

use std::sync::Arc;

use common::Vec2;
use sim::car::{CarParams, CarState, Ground};
use sim::track::generator::turtle::Turtle;
use sim::track::{EdgeKind, Track, TrackDef, TrackError};
use sim::{Controls, World};
use support::{DT, KMH};

/// Straight along y = 0, a 270 degree left loop, straight down x = 20
/// (crossing the first straight at right angles at (20, 0)), a 270 degree
/// right loop, and back to the start.
fn figure_eight(edge: EdgeKind) -> Result<Track, TrackError> {
    let mut t = Turtle::default();
    t.straight(60.0);
    t.arc(40.0, 270f64.to_radians());
    t.straight(80.0);
    t.arc(40.0, -270f64.to_radians());
    t.straight(20.0);
    let mut anchors = t.anchors();
    for a in &mut anchors {
        a.left = edge;
        a.right = edge;
    }
    Track::build(TrackDef { version: 1, name: "figure 8".into(), width: 12.0, verge: 8.0, anchors, zones: vec![] })
}

#[test]
fn crossing_is_found_at_right_angles() {
    let track = figure_eight(EdgeKind::Hard).unwrap();
    assert_eq!(track.crossings.len(), 1);
    let c = track.crossings[0];
    assert!(c.point.distance(Vec2::new(20.0, 0.0)) < 1.0, "{:?}", c.point);
    assert!((c.angle.to_degrees() - 90.0).abs() < 5.0);
}

#[test]
fn walls_open_up_across_the_crossroads() {
    for edge in [EdgeKind::Hard, EdgeKind::Soft] {
        let track = figure_eight(edge).unwrap();
        // Along the first straight, through the crossing: the first thing hit
        // must be far beyond the other road (the loop's far side).
        let hit = track.raycast(Vec2::new(5.0, 0.0), Vec2::new(1.0, 0.0), 60.0);
        assert!(hit.is_none(), "{edge:?}: blocked at {:?}", hit.map(|h| h.distance));
        // Across the other road's asphalt counts as asphalt.
        assert_eq!(track.ground_at(Vec2::new(20.0, 5.5), Some(20)), Ground::ASPHALT);
    }
}

#[test]
fn a_car_drives_straight_through_without_touching_anything() {
    let track = Arc::new(figure_eight(EdgeKind::Hard).unwrap());
    let params = CarParams::default();
    let mut world = World::new(track, params.clone());
    world.set_car_state(CarState::rolling(&params, Vec2::new(2.0, 0.0), 0.0, 60.0 * KMH));
    let mut contacts = 0;
    while world.car.state.position.x < 50.0 {
        contacts += world.step(Controls { throttle: 0.3, ..Default::default() }, DT).len();
    }
    assert_eq!(contacts, 0);
    assert!(world.progress > 45.0, "progress followed the wrong road: {}", world.progress);
}

fn build(t: &Turtle) -> Result<Track, TrackError> {
    Track::build(TrackDef { version: 1, name: "test".into(), width: 12.0, verge: 8.0, anchors: t.anchors(), zones: vec![] })
}

#[test]
fn shallow_crossings_are_rejected() {
    // Out along y = 0, round a hairpin, then back on a line that crosses the
    // first straight at x = 100 at only 20 degrees.
    let mut t = Turtle::default();
    t.straight(300.0);
    t.arc(30.0, 180f64.to_radians());
    t.arc(200.0, 20f64.to_radians());
    t.straight(200.0);
    assert!(t.close(40.0));
    assert!(matches!(build(&t), Err(TrackError::ShallowCrossing { .. })), "{:?}", build(&t).err());
}

#[test]
fn parallel_overlaps_are_rejected() {
    // A hairpin whose second leg bends back towards the first, so the two
    // run side by side with their verges overlapping.
    let mut t = Turtle::default();
    t.straight(200.0);
    t.arc(16.0, 180f64.to_radians());
    t.arc(150.0, 8f64.to_radians());
    t.straight(120.0);
    assert!(t.close(40.0));
    assert!(build(&t).is_err());
}
