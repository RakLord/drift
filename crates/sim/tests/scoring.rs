//! Scoring behaviour, using scripted car poses along the oval's first
//! straight (y = 0, driving +x, left is +y, track edges at y = +-6).

mod support;

use common::Vec2;
use sim::World;
use sim::car::{CarParams, CarState};
use sim::scoring::{ScoreEventKind, Scorer, ScoringParams};
use sim::track::def::ZoneDef;
use sim::track::{EdgeKind, SegmentKind, Side, VERGE};
use sim::world::Contact;
use support::{DT, oval};

const SPEED: f64 = 15.0;

struct Rig {
    world: World,
    scorer: Scorer,
    x: f64,
    events: Vec<ScoreEventKind>,
}

impl Rig {
    fn new(zones: Vec<ZoneDef>) -> Self {
        Self::with(ScoringParams::default(), zones)
    }

    fn with(params: ScoringParams, zones: Vec<ZoneDef>) -> Self {
        let track = oval(EdgeKind::Hard, EdgeKind::Hard, zones);
        let scorer = Scorer::new(params, &track);
        let world = World::new(track, CarParams::default());
        Self { world, scorer, x: 10.0, events: vec![] }
    }

    /// Slide along the straight at `y` for `secs`, with the nose turned
    /// `angle_deg` from the direction of travel (positive = nose left, tail
    /// out to the right, as in a left-hand drift).
    fn slide(&mut self, y: f64, angle_deg: f64, secs: f64) {
        self.slide_with(y, angle_deg, secs, |_, _| vec![]);
    }

    fn slide_with(&mut self, y: f64, angle_deg: f64, secs: f64, mut contacts: impl FnMut(&World, f64) -> Vec<Contact>) {
        let ticks = (secs / DT).round() as usize;
        for tick in 0..ticks {
            let params = self.world.car.params.clone();
            let mut state = CarState::at_rest(&params, Vec2::new(self.x, y), angle_deg.to_radians());
            state.velocity = Vec2::new(SPEED, 0.0);
            self.world.set_car_state(state);
            let c = contacts(&self.world, tick as f64 * DT);
            for e in self.scorer.update(&self.world, &c, DT) {
                self.events.push(e.kind);
            }
            self.x += SPEED * DT;
        }
    }

    fn count(&self, pred: impl Fn(&ScoreEventKind) -> bool) -> usize {
        self.events.iter().filter(|e| pred(e)).count()
    }

    /// A contact on the rear bumper against the right-hand wall.
    fn rear_contact(world: &World, impact_speed: f64) -> Contact {
        let s = &world.car.state;
        let rear = s.position + Vec2::new(-world.car.params.body.length / 2.0, 0.0).rotate(s.heading);
        Contact { point: rear, normal: Vec2::new(0.0, 1.0), impact_speed, scrape_speed: 5.0, kind: SegmentKind::HardWall }
    }
}

#[test]
fn held_drift_banks_angle_points_on_clean_exit() {
    let mut rig = Rig::new(vec![]);
    rig.slide(0.0, 35.0, 2.0);
    assert!(rig.scorer.combo.active);
    assert_eq!(rig.scorer.total.total(), 0.0, "nothing banked mid-drift");
    rig.slide(0.0, 0.0, 1.0);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::Banked), 1);
    let total = rig.scorer.total;
    println!("banked {total:?}");
    assert!(total.angle > 100.0 && total.line == 0.0);
}

#[test]
fn spin_loses_the_combo_and_costs_points() {
    let mut rig = Rig::new(vec![]);
    rig.slide(0.0, 35.0, 2.0);
    rig.slide(0.0, 150.0, 0.5);
    rig.slide(0.0, 0.0, 1.0);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::Spin), 1);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::Banked), 0);
    assert!(rig.scorer.total.total() < 0.0);
}

#[test]
fn outer_zone_pays_for_rear_bumper_near_the_edge() {
    let zone = || vec![ZoneDef::Outer { from: 0.2, to: 0.6, side: Side::Right }];
    // Nose left of travel puts the tail out to the right: at 30 degrees the
    // rear-right corner sits ~1.85 m right of the car's centre.
    let close = {
        let mut rig = Rig::new(zone());
        rig.slide(-6.0 + 0.3 + 1.85, 30.0, 8.0);
        assert_eq!(rig.count(|e| *e == ScoreEventKind::ZoneEnter), 1);
        rig.scorer.combo.pending.line
    };
    let far = {
        let mut rig = Rig::new(zone());
        rig.slide(3.0, 30.0, 8.0);
        rig.scorer.combo.pending.line
    };
    println!("outer zone line points: close {close:.0}, far {far:.0}");
    assert!(close > 300.0 && far == 0.0);
}

#[test]
fn clip_pays_once_by_front_bumper_proximity() {
    let clip = || vec![ZoneDef::Clip { at: 0.5, side: Side::Left }];
    let quality = |y: f64| {
        let mut rig = Rig::new(clip());
        rig.slide(y, 30.0, 9.0);
        rig.events.iter().find_map(|e| match e {
            ScoreEventKind::Clip { quality } => Some(*quality),
            _ => None,
        })
    };
    // Nose pointing into the left-hand corner: the front-left corner sits
    // ~1.85 m left of the car's centre.
    let close = quality(6.0 - 0.3 - 1.85);
    let wide = quality(-2.0);
    println!("clip quality: close {close:?}, wide {wide:?}");
    assert!(close.unwrap() > 0.9);
    assert!(wide.is_none());
}

#[test]
fn clips_pay_by_angle_and_misses_are_counted() {
    let clip = || vec![ZoneDef::Clip { at: 0.5, side: Side::Left }];
    let close = 6.0 - 0.3 - 1.85;
    // (clip events, missed events, line points / quality, passes, angle at the pass)
    let run = |params: ScoringParams, y: f64, angle: f64| {
        let mut rig = Rig::with(params, clip());
        rig.slide(y, angle, 9.0);
        let quality = rig.events.iter().find_map(|e| match e {
            ScoreEventKind::Clip { quality } => Some(*quality),
            _ => None,
        });
        let per_quality = quality.map_or(0.0, |q| rig.scorer.combo.pending.line / q);
        let passes = rig.scorer.clip_passes;
        (
            rig.count(|e| matches!(e, ScoreEventKind::Clip { .. })),
            rig.count(|e| *e == ScoreEventKind::ClipMissed),
            per_quality,
            passes.count,
            passes.angle_sum,
        )
    };
    let scaled = ScoringParams::default();
    assert!(scaled.line.clip_angle_scaled, "on in config/scoring.toml");

    let (hits, misses, half, passes, angle) = run(scaled.clone(), close, 30.0);
    assert_eq!((hits, misses, passes), (1, 0, 1));
    assert!((angle - 30.0).abs() < 1.0, "angle at the pass {angle}");
    let (_, _, full, ..) = run(scaled.clone(), close, 50.0);
    println!("clip points per quality: 30 deg {half:.0}, 50 deg {full:.0}");
    assert!((half / full - 0.5).abs() < 0.01, "30 deg is halfway up the 20-40 deg ramp");

    let (hits, misses, ..) = run(scaled.clone(), close, 15.0);
    assert_eq!((hits, misses), (0, 1), "drifting, but under the angle credit floor: a miss");
    let (hits, misses, ..) = run(scaled.clone(), -2.0, 30.0);
    assert_eq!((hits, misses), (0, 1), "too wide: a miss");

    let mut flat = scaled;
    flat.line.clip_angle_scaled = false;
    let (hits, misses, ..) = run(flat, close, 15.0);
    assert_eq!((hits, misses), (1, 0), "older files: proximity only");
}

#[test]
fn wall_taps_are_capped_per_combo_and_rate_limited() {
    let mut rig = Rig::new(vec![]);
    let params = ScoringParams::default().style;
    // A gentle rear tap every tick for 10 s: only the cap's worth may count.
    rig.slide_with(0.0, 30.0, 10.0, |w, _| vec![Rig::rear_contact(w, 1.0)]);
    let taps = rig.count(|e| *e == ScoreEventKind::WallTap);
    assert_eq!(taps, params.max_taps_per_combo as usize);

    // Within one cooldown, only one tap counts.
    let mut rig = Rig::new(vec![]);
    rig.slide_with(0.0, 30.0, params.wall_tap_cooldown_secs * 0.9, |w, _| vec![Rig::rear_contact(w, 1.0)]);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::WallTap), 1);
}

#[test]
fn hard_hit_loses_the_combo() {
    let mut rig = Rig::new(vec![]);
    rig.slide(0.0, 30.0, 1.0);
    rig.slide_with(0.0, 30.0, DT, |w, _| vec![Rig::rear_contact(w, 6.0)]);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::HardHit), 1);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::WallTap), 0);
    assert!(!rig.scorer.combo.active);
}

#[test]
fn four_wheels_on_the_verge_pauses_scoring() {
    let mut rig = Rig::new(vec![]);
    rig.slide(0.0, 30.0, 0.5);
    let pending = rig.scorer.combo.pending.angle;
    assert!(pending > 0.0);

    // One more drifting tick, but with every wheel on the grass.
    let params = rig.world.car.params.clone();
    let mut state = CarState::at_rest(&params, Vec2::new(rig.x, 0.0), 30f64.to_radians());
    state.velocity = Vec2::new(SPEED, 0.0);
    rig.world.set_car_state(state);
    rig.world.ground = [VERGE; 4];
    rig.scorer.update(&rig.world, &[], DT);

    assert_eq!(rig.scorer.combo.pending.angle, pending, "no points with all four wheels on the verge");
    assert!(rig.scorer.combo.active, "the combo is kept");
}

#[test]
fn a_drift_too_short_to_judge_banks_only_penalties() {
    let mut p = ScoringParams::default();
    p.drift.min_bank_secs = 1.0;
    let mut rig = Rig::with(p.clone(), vec![]);
    rig.slide(0.0, 45.0, 0.5);
    rig.slide(0.0, 0.0, 1.0);
    assert_eq!(rig.count(|e| *e == ScoreEventKind::Banked), 1);
    assert_eq!(rig.scorer.total.total(), 0.0, "a 0.5 s drift banks nothing");

    let mut rig = Rig::with(p, vec![]);
    rig.slide(0.0, 45.0, 1.5);
    rig.slide(0.0, 0.0, 1.0);
    assert!(rig.scorer.total.angle > 0.0, "a 1.5 s drift banks");
}

#[test]
fn scaled_transitions_pay_for_the_angle_held() {
    let mut p = ScoringParams::default();
    p.drift.transition_min_hold_secs = 0.75;
    p.angle.credit_from_deg = 20.0;
    p.style.transition_scaled = true;
    let transition = |deg: f64| {
        let mut rig = Rig::with(p.clone(), vec![]);
        rig.slide(0.0, deg, 1.0);
        rig.slide(0.0, -deg, 0.5);
        (rig.count(|e| *e == ScoreEventKind::Transition), rig.scorer.combo.pending.style)
    };
    let (big_count, big) = transition(50.0);
    let (small_count, small) = transition(30.0);
    let (tiny_count, tiny) = transition(16.0);
    assert_eq!((big_count, small_count), (1, 1));
    assert!(big > 1.5 * small && small > 0.0, "big {big} vs small {small}");
    assert_eq!((tiny_count, tiny), (0, 0.0), "a side under the credit floor isn't judged");
}
