//! Street lamps: tracks carry no lights, so they're placed from the track
//! itself, once per track (viewer-only; the sim never sees them). One on
//! the outside of each corner's apex, then one about every `SPACING`
//! metres on alternating sides, behind the wall or on the verge fence.
//! Never where the edge is open (figure-8 crossings).

use common::Vec2;
use macroquad::prelude::*;
use sim::track::{EdgeKind, Side, Track};

use super::camera::FollowCamera;
use super::{draw_segment, light, theme, to_mq};

const SPACING: f64 = 35.0;
/// No two lamps closer than this.
const MIN_GAP: f64 = 18.0;
/// Behind a hard wall, this far out from the asphalt edge.
const BEHIND_WALL: f64 = 1.2;
/// The arm reaches this far over the road from the post.
const ARM: f64 = 2.5;
/// Light pool radius, metres, and its strength.
const POOL_RADIUS: f64 = 19.0;
const POOL_ALPHA: f32 = 0.30;
/// The lamp head's own small, bright glow.
const HEAD_RADIUS: f64 = 1.6;
const HEAD_ALPHA: f32 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    /// The post, beside the track.
    pub post: Vec2,
    /// The lamp head over the road edge; the pool is centred here.
    pub head: Vec2,
}

/// Lamps for a track, in a fixed order (the same track always gets the
/// same lamps).
pub fn place(track: &Track) -> Vec<Lamp> {
    let mut lamps: Vec<Lamp> = vec![];
    let add = |lamps: &mut Vec<Lamp>, i: usize, side: Side| {
        let Some(lamp) = lamp_at(track, i, side) else { return false };
        if lamps.iter().any(|l| l.post.distance(lamp.post) < MIN_GAP) {
            return false;
        }
        lamps.push(lamp);
        true
    };
    // The outside of each corner, where a drift swings wide.
    for corner in &track.corners {
        let outside = if corner.direction > 0.0 { Side::Right } else { Side::Left };
        add(&mut lamps, corner.apex, outside);
    }
    // Then along the track, alternating sides.
    let mut next = 0.0;
    let mut side = Side::Left;
    for (i, sample) in track.samples.iter().enumerate() {
        if sample.distance < next {
            continue;
        }
        let other = if side == Side::Left { Side::Right } else { Side::Left };
        if add(&mut lamps, i, side) || add(&mut lamps, i, other) {
            side = other;
        }
        next = sample.distance + SPACING;
    }
    lamps
}

fn lamp_at(track: &Track, i: usize, side: Side) -> Option<Lamp> {
    let s = &track.samples[i];
    let out = s.normal() * side.sign();
    let edge = s.position + out * track.half_width();
    let post = match track.edge(i, side) {
        EdgeKind::Hard if track.edge_present(i, side) => edge + out * BEHIND_WALL,
        EdgeKind::Soft if track.fence_present(i, side) => edge + out * track.def.verge,
        _ => return None,
    };
    Some(Lamp { post, head: post - out * ARM })
}

/// The posts and arms (the track pass).
pub fn draw_posts(lamps: &[Lamp], camera: &FollowCamera) {
    for lamp in lamps.iter().filter(|l| light::in_view(camera, l.post, ARM)) {
        draw_segment(lamp.post, lamp.head, 0.14, theme::LAMP_POST);
        let (p, h) = (to_mq(lamp.post), to_mq(lamp.head));
        draw_circle(p.x, p.y, 0.25, theme::LAMP_POST);
        draw_circle(h.x, h.y, 0.22, theme::SODIUM_LIGHT);
    }
}

/// The pools of light (inside `light::pass`).
pub fn draw_pools(lamps: &[Lamp], camera: &FollowCamera) {
    for lamp in lamps.iter().filter(|l| light::in_view(camera, l.head, POOL_RADIUS)) {
        light::glow(lamp.head, POOL_RADIUS, Color { a: POOL_ALPHA, ..theme::SODIUM_LIGHT });
        light::glow(lamp.head, HEAD_RADIUS, Color { a: HEAD_ALPHA, ..theme::SODIUM_LIGHT });
    }
}

#[cfg(test)]
mod tests {
    use sim::track::generator::{self, GeneratorParams};

    use super::*;

    fn track(seed: u64) -> Track {
        Track::build(generator::generate(&GeneratorParams::default(), seed).unwrap()).unwrap()
    }

    #[test]
    fn lamps_are_spread_out_and_beside_the_track() {
        for seed in [1, 7, 11] {
            let track = track(seed);
            let lamps = place(&track);
            let expected = track.length / SPACING;
            assert!(lamps.len() as f64 > expected * 0.5, "seed {seed}: {} lamps for {:.0} m", lamps.len(), track.length);
            for (k, a) in lamps.iter().enumerate() {
                assert!(lamps[k + 1..].iter().all(|b| a.post.distance(b.post) >= MIN_GAP), "seed {seed}: two lamps too close");
                // Off the asphalt of every part of the track (crossings too).
                let nearest = track.project(a.post, None);
                assert!(nearest.offset.abs() > track.half_width(), "seed {seed}: a post on the road");
            }
        }
    }

    #[test]
    fn both_sides_get_lamps() {
        let track = track(1);
        let lamps = place(&track);
        let left = lamps.iter().filter(|l| track.project(l.post, None).offset > 0.0).count();
        assert!(left > 0 && left < lamps.len(), "{left} of {} on the left", lamps.len());
    }

    #[test]
    fn the_same_track_gets_the_same_lamps() {
        assert_eq!(place(&track(7)), place(&track(7)));
    }
}
