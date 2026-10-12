//! Procedural drift tracks. A run of corner pieces (hairpins, sweepers,
//! S-bends, ...) laid by a turtle, closed with the shortest smooth path back
//! to the start. Tracks may cross themselves (figure-8s). Deterministic: the
//! same seed and parameters always give the same track.

pub mod pieces;
pub mod rejection;
pub mod turtle;

use common::Rng;
use serde::{Deserialize, Serialize};

use super::Track;
use super::boundary::Side;
use super::crossings::near_crossing;
use super::def::{EdgeKind, FORMAT_VERSION, TrackDef, ZoneDef};
use pieces::Piece;
pub use rejection::{GenerationFailure, Rejection};
use turtle::{ClosingPath, Turtle};

const DEFAULT_GENERATOR_TOML: &str = include_str!("../../../../../config/generator.toml");

/// Everything that shapes a generated track. Field meanings are documented
/// in `config/generator.toml`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorParams {
    pub attempts: u32,
    pub layout: LayoutParams,
    pub pieces: PieceWeights,
    pub shapes: ShapeParams,
    pub surface: SurfaceParams,
    pub zones: ZoneParams,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutParams {
    pub length_min: f64,
    pub length_max: f64,
    pub pieces_min: u32,
    pub pieces_max: u32,
    pub initiation: [f64; 2],
    pub straight_chance: f64,
    pub straight: [f64; 2],
    pub homing: f64,
    pub homing_end: f64,
    pub figure8: f64,
    pub closing_radius: f64,
    pub max_closing_straight: f64,
    pub max_closing_turn: f64,
    pub return_pieces: u32,
    pub min_crossings: u32,
    pub max_crossings: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceWeights {
    pub hairpin: f64,
    pub sweeper: f64,
    pub s_bend: f64,
    pub decreasing: f64,
    pub chicane: f64,
    pub kink: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeParams {
    pub hairpin_radius: [f64; 2],
    pub hairpin_angle: [f64; 2],
    pub sweeper_radius: [f64; 2],
    pub sweeper_angle: [f64; 2],
    pub s_bend_radius: [f64; 2],
    pub s_bend_angle: [f64; 2],
    pub s_bend_gap: [f64; 2],
    pub decreasing_open_radius: [f64; 2],
    pub decreasing_tight_radius: [f64; 2],
    pub decreasing_angle: [f64; 2],
    pub chicane_radius: [f64; 2],
    pub chicane_angle: [f64; 2],
    pub chicane_flicks: [f64; 2],
    pub kink_radius: [f64; 2],
    pub kink_angle: [f64; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceParams {
    pub width: [f64; 2],
    pub verge: f64,
    pub corner_radius: f64,
    pub outside_hard_chance: f64,
    pub inside_hard_chance: f64,
    pub straight_hard_chance: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneParams {
    pub clip_min_angle: f64,
    pub outer_min_angle: f64,
    pub outer_start: f64,
    pub outer_end: f64,
}

impl Default for GeneratorParams {
    fn default() -> Self {
        toml::from_str(DEFAULT_GENERATOR_TOML).expect("config/generator.toml is invalid")
    }
}

/// Generate a valid track for `seed`. If every attempt fails, says why.
pub fn generate(p: &GeneratorParams, seed: u64) -> Result<TrackDef, GenerationFailure> {
    let mut failure = GenerationFailure::new(seed, p.attempts);
    for attempt in 0..u64::from(p.attempts) {
        let mut rng = Rng::with_stream(seed, attempt);
        match attempt_track(p, &mut rng, seed) {
            Ok(def) => return Ok(def),
            Err(reason) => failure.record(reason),
        }
    }
    Err(failure)
}

fn attempt_track(p: &GeneratorParams, rng: &mut Rng, seed: u64) -> Result<TrackDef, Rejection> {
    let build = |def: &TrackDef| Track::build(def.clone()).map_err(|e| Rejection::from_build(&e));
    let layout = &p.layout;
    let mut turtle = Turtle::default();
    turtle.straight(range(rng, layout.initiation));

    let count = layout.pieces_min + rng.below(layout.pieces_max.saturating_sub(layout.pieces_min) + 1);
    let lobe = if rng.chance(0.5) { 1.0 } else { -1.0 };
    // Head for a gate behind the start line, so the track arrives lined up.
    let gate = turtle.gate(2.0 * layout.closing_radius);
    for k in 0..count {
        // Pieces curl back more and more as the track goes on, so the
        // closing path stays short.
        let progress = f64::from(k) / f64::from(count.max(2) - 1);
        let homing = layout.homing + (layout.homing_end - layout.homing) * progress;
        let direction = if rng.chance(layout.figure8) {
            if k < count / 2 { lobe } else { -lobe }
        } else if rng.chance(homing) {
            turtle.towards(gate)
        } else if rng.chance(0.5) {
            1.0
        } else {
            -1.0
        };
        Piece::choose(&p.pieces, rng).lay(&mut turtle, direction, &p.shapes, rng);
        if k + 1 < count && rng.chance(layout.straight_chance) {
            turtle.straight(range(rng, layout.straight));
        }
    }

    // Still far from the gate? Head back with a few more pieces (or a
    // straight when already pointing the right way) before closing.
    // Never two straights in a row: that's how drag strips happen.
    let near = layout.max_closing_straight * 0.5 + layout.closing_radius;
    let mut last_was_straight = false;
    for _ in 0..layout.return_pieces {
        let distance = turtle.distance_to(gate);
        if distance <= near {
            break;
        }
        let facing_gate = turtle.bearing_to(gate).abs() < 25f64.to_radians();
        if facing_gate && !last_was_straight {
            turtle.straight((distance - near * 0.5).min(layout.straight[1]));
            last_was_straight = true;
        } else {
            last_was_straight = false;
            let direction = turtle.towards(gate);
            Piece::choose(&p.pieces, rng).lay(&mut turtle, direction, &p.shapes, rng);
        }
    }

    // Close the loop with the shortest path that isn't a drag strip or a
    // huge looping turn, trying a few turn radii.
    let max_turn = layout.max_closing_turn.to_radians();
    let candidates: Vec<_> =
        [1.0, 1.5, 2.0, 3.0].iter().flat_map(|k| turtle.closing_paths(layout.closing_radius * k)).collect();
    if candidates.is_empty() {
        return Err(Rejection::NoClosingPath);
    }
    let fits_turn = |c: &ClosingPath| c.first.abs() <= max_turn && c.second.abs() <= max_turn;
    let closing = candidates
        .iter()
        .filter(|c| fits_turn(c) && c.straight <= layout.max_closing_straight)
        .min_by(|a, b| a.length().total_cmp(&b.length()))
        .copied()
        .ok_or(if candidates.iter().any(fits_turn) { Rejection::ClosingStraight } else { Rejection::ClosingTurn })?;
    turtle.lay_closing(&closing);

    let mut def = TrackDef {
        version: FORMAT_VERSION,
        name: format!("Generated {seed}"),
        width: range(rng, p.surface.width),
        verge: p.surface.verge,
        anchors: turtle.anchors(),
        zones: vec![],
    };

    // First pass (all soft): check the shape, find straights and corners.
    let draft = build(&def)?;
    let crossings = draft.crossings.len() as u32;
    if draft.length < layout.length_min {
        return Err(Rejection::TooShort);
    }
    if draft.length > layout.length_max {
        return Err(Rejection::TooLong);
    }
    if crossings < layout.min_crossings {
        return Err(Rejection::TooFewCrossings);
    }
    if crossings > layout.max_crossings {
        return Err(Rejection::TooManyCrossings);
    }
    start_on_longest_straight(&mut def, &draft);
    let draft = build(&def)?;
    assign_edges(&mut def, &draft, &p.surface, rng);
    def.zones = place_zones(&draft, &p.zones);
    if def.zones.is_empty() {
        return Err(Rejection::NoZones);
    }

    // Final build checks the walls and zones still fit.
    build(&def)?;
    Ok(def)
}

fn range(rng: &mut Rng, [lo, hi]: [f64; 2]) -> f64 {
    rng.range(lo, hi.max(lo))
}

/// Rotate the anchor list so the track starts on its longest straight.
fn start_on_longest_straight(def: &mut TrackDef, track: &Track) {
    let sections = def.anchors.len();
    let mut stats = vec![(0.0, 0usize); sections];
    for s in &track.samples {
        stats[s.section].0 += s.curvature.abs();
        stats[s.section].1 += 1;
    }
    // Straight-ish sections, longest first; fall back to the least curved.
    let straightness = |&(turn, len): &(f64, usize)| turn / len.max(1) as f64;
    let best = (0..sections)
        .filter(|&i| straightness(&stats[i]) < 1.0 / 400.0)
        .max_by_key(|&i| stats[i].1)
        .or_else(|| (0..sections).min_by(|&a, &b| straightness(&stats[a]).total_cmp(&straightness(&stats[b]))))
        .unwrap_or(0);
    def.anchors.rotate_left(best);
}

fn assign_edges(def: &mut TrackDef, track: &Track, p: &SurfaceParams, rng: &mut Rng) {
    let sections = def.anchors.len();
    let mut curvature = vec![(0.0, 0usize); sections];
    for s in &track.samples {
        curvature[s.section].0 += s.curvature;
        curvature[s.section].1 += 1;
    }
    for (anchor, (sum, n)) in def.anchors.iter_mut().zip(curvature) {
        let mean = sum / n.max(1) as f64;
        let hard = |rng: &mut Rng, chance| if rng.chance(chance) { EdgeKind::Hard } else { EdgeKind::Soft };
        if mean.abs() > 1.0 / p.corner_radius {
            let (outside, inside) = (hard(rng, p.outside_hard_chance), hard(rng, p.inside_hard_chance));
            // Turning left: the outside is on the right.
            (anchor.left, anchor.right) = if mean > 0.0 { (inside, outside) } else { (outside, inside) };
        } else {
            anchor.left = hard(rng, p.straight_hard_chance);
            anchor.right = hard(rng, p.straight_hard_chance);
        }
    }
}

/// A clip at each corner's apex on the inside, and an outer zone along the
/// outside of the bigger corners. Nothing is placed at crossroads.
fn place_zones(track: &Track, p: &ZoneParams) -> Vec<ZoneDef> {
    let n = track.samples.len();
    let at_crossing = |i: usize| near_crossing(track.samples[i % n].position, &track.crossings);
    let mut zones = vec![];
    for c in &track.corners {
        let (inside, outside) = if c.direction > 0.0 { (Side::Left, Side::Right) } else { (Side::Right, Side::Left) };
        let degrees = c.angle.to_degrees();
        if degrees >= p.clip_min_angle && !at_crossing(c.apex) {
            let at = track.section_coordinate(track.samples[c.apex].distance);
            zones.push(ZoneDef::Clip { at, side: inside });
        }
        let length = c.length(track.length);
        let (zone_start, zone_end) = (c.start + length * p.outer_start, c.start + length * p.outer_end);
        let spacing = track.length / n as f64;
        let first = (zone_start.rem_euclid(track.length) / spacing) as usize;
        let span = ((zone_end - zone_start) / spacing) as usize;
        let clear = !(0..=span).any(|k| at_crossing(first + k));
        if degrees >= p.outer_min_angle && clear {
            let from = track.section_coordinate(zone_start);
            let to = track.section_coordinate(zone_end);
            zones.push(ZoneDef::Outer { from, to, side: outside });
        }
    }
    zones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_track() {
        let p = GeneratorParams::default();
        let a = toml::to_string(&generate(&p, 7).unwrap()).unwrap();
        let b = toml::to_string(&generate(&p, 7).unwrap()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn most_seeds_produce_valid_tracks_with_zones() {
        let p = GeneratorParams::default();
        let mut built = 0;
        for seed in 0..30 {
            if let Ok(def) = generate(&p, seed) {
                let track = Track::build(def).expect("generated tracks must build");
                assert!(!track.zones.is_empty(), "seed {seed} has no zones");
                assert!(track.crossings.len() as u32 <= p.layout.max_crossings);
                built += 1;
            }
        }
        assert_eq!(built, 30, "only {built}/30 seeds produced a track");
    }

    #[test]
    fn impossible_settings_explain_themselves() {
        let mut p = GeneratorParams { attempts: 50, ..Default::default() };
        // Let every attempt close, so the length check gets its say.
        p.layout.max_closing_straight = 1e6;
        p.layout.length_min = 4000.0;
        p.layout.length_max = 4500.0;
        let failure = generate(&p, 1).unwrap_err();
        assert!(failure.reasons().iter().any(|&(r, _)| r == Rejection::TooShort), "{failure}");
        let explained = failure.lines(12).join("\n");
        assert!(explained.contains("length min"), "{explained}");
    }

    /// Longest run of near-straight centreline, metres.
    fn longest_straight(track: &Track) -> f64 {
        let n = track.samples.len();
        let (mut best, mut run) = (0, 0);
        for k in 0..2 * n {
            run = if track.samples[k % n].curvature.abs() < 1.0 / 400.0 { run + 1 } else { 0 };
            best = best.max(run);
        }
        best.min(n) as f64 * track.length / n as f64
    }

    #[test]
    fn no_drag_strips() {
        let p = GeneratorParams::default();
        // The closing straight can run straight into the initiation straight.
        let limit = p.layout.initiation[1] + p.layout.max_closing_straight.max(p.layout.straight[1]) + 10.0;
        for seed in 0..20 {
            let track = Track::build(generate(&p, seed).unwrap()).unwrap();
            let longest = longest_straight(&track);
            assert!(longest < limit, "seed {seed}: {longest:.0} m straight");
        }
    }

    #[test]
    fn figure_eights_on_request() {
        let mut p = GeneratorParams::default();
        p.layout.figure8 = 0.8;
        p.layout.min_crossings = 1;
        let mut built = 0;
        for seed in 0..12 {
            if let Ok(def) = generate(&p, seed) {
                assert!(!Track::build(def).unwrap().crossings.is_empty());
                built += 1;
            }
        }
        assert!(built >= 9, "only {built}/12 seeds produced a figure-8");
    }

    #[test]
    fn generated_tracks_round_trip_through_toml() {
        let def = generate(&GeneratorParams::default(), 3).unwrap();
        let text = toml::to_string(&def).unwrap();
        let back: TrackDef = toml::from_str(&text).unwrap();
        assert_eq!(toml::to_string(&back).unwrap(), text);
        Track::build(back).unwrap();
    }
}
