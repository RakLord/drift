//! Drift scoring, judged like Formula Drift: Line (clips and zones), Angle,
//! and Style (transitions, wall taps, no bobbles), multiplied by speed, held
//! in a combo that is banked on a clean exit and lost on a spin or crash.

pub mod combo;
pub mod drift;
pub mod flow;
pub mod params;

use common::Vec2;

use crate::World;
use crate::track::{SegmentKind, Track, TrackPoint, ZoneKind};
use crate::world::Contact;
pub use combo::{Breakdown, Combo};
use drift::{DriftDetector, DriftEvent};
use flow::Flow;
pub use params::ScoringParams;
use params::proximity;

/// Contacts this close to the rear of the car count as rear-bumper taps, m.
const REAR_BUMPER_ZONE: f64 = 0.7;
/// The "recent peak" angle used to spot bobbles fades at this rate, deg/s, so
/// deliberately easing off the angle isn't mistaken for a bobble.
const PEAK_DECAY_DEG_PER_SEC: f64 = 25.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScoreEventKind {
    DriftStart,
    /// `quality` 0..1: how close the front bumper got.
    Clip { quality: f64 },
    ZoneEnter,
    Transition,
    WallTap,
    Bobble,
    Banked,
    Spin,
    HardHit,
}

#[derive(Clone, Copy, Debug)]
pub struct ScoreEvent {
    pub kind: ScoreEventKind,
    /// Points gained (or lost, negative) by this event.
    pub points: f64,
}

#[derive(Clone, Debug, Default)]
struct ClipState {
    in_window: bool,
    /// Closest the front bumper got during this approach, metres.
    best: Option<f64>,
    /// Lap on which this clip last paid out.
    paid_lap: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct OuterState {
    inside: bool,
    /// Already counted towards the multiplier on this pass.
    entered: bool,
}

#[derive(Clone, Debug, Default)]
struct Bobble {
    peak: f64,
    /// When and how low the angle dipped below the recent peak.
    dip: Option<(f64, f64)>,
}

pub struct Scorer {
    pub params: ScoringParams,
    pub drift: DriftDetector,
    /// Smoothness credit for training (see `flow`).
    pub flow: Flow,
    pub combo: Combo,
    /// Everything banked so far (and penalties outside combos).
    pub total: Breakdown,
    /// Per track zone: is it being scored right now (for rendering).
    pub zone_lit: Vec<bool>,
    events: Vec<ScoreEvent>,
    clips: Vec<ClipState>,
    outers: Vec<OuterState>,
    bobble: Bobble,
    time: f64,
    last_tap: f64,
}

impl Scorer {
    pub fn new(params: ScoringParams, track: &Track) -> Self {
        let zones = track.zones.len();
        Self {
            params,
            drift: DriftDetector::default(),
            flow: Flow::default(),
            combo: Combo::default(),
            total: Breakdown::default(),
            zone_lit: vec![false; zones],
            events: vec![],
            clips: vec![ClipState::default(); zones],
            outers: vec![OuterState::default(); zones],
            bobble: Bobble::default(),
            time: 0.0,
            last_tap: f64::NEG_INFINITY,
        }
    }

    /// Judge one tick. `contacts` are the wall impacts from that tick.
    pub fn update(&mut self, world: &World, contacts: &[Contact], dt: f64) -> &[ScoreEvent] {
        self.events.clear();
        self.time += dt;
        let state = &world.car.state;
        let angle = state.body_slip_angle();
        let forward = state.velocity.dot(world.place.tangent) > 0.0;
        let on_verge = world.ground.iter().all(|g| g.grip < 1.0);
        let speed_mult = self.params.speed.multiplier(state.speed() * 3.6);

        self.check_hard_hits(contacts);

        let event = self.drift.update(&self.params.drift, angle, state.speed(), forward, dt);
        self.flow.update(&self.params, event, self.drift.active, angle.abs().to_degrees(), on_verge, dt);
        match event {
            DriftEvent::Started => {
                self.combo.start();
                self.bobble = Bobble { peak: angle.abs().to_degrees(), dip: None };
                self.emit(ScoreEventKind::DriftStart, 0.0);
            }
            DriftEvent::Transition => {
                let points = self.params.style.transition_points * speed_mult;
                self.combo.pending.style += points;
                self.combo.bump(&self.params.combo);
                self.emit(ScoreEventKind::Transition, points);
            }
            DriftEvent::Ended => self.bank(),
            DriftEvent::Spin => {
                self.combo.lose();
                self.total.penalty += self.params.combo.spin_penalty;
                self.emit(ScoreEventKind::Spin, -self.params.combo.spin_penalty);
            }
            DriftEvent::Flick | DriftEvent::None => {}
        }

        let scoring = self.drift.active && self.combo.active && !on_verge;
        if scoring {
            let deg = angle.abs().to_degrees();
            let factor = self.params.angle.factor(deg, self.params.drift.start_angle_deg);
            self.combo.pending.angle += self.params.angle.points_per_sec * factor * speed_mult * dt;
            self.check_wall_taps(world, contacts, speed_mult);
            self.check_bobble(deg, dt);
        }
        self.update_zones(world, scoring, speed_mult, dt);
        &self.events
    }

    /// The run is over (e.g. time is up): bank any live combo as if the drift
    /// ended cleanly.
    pub fn finish(&mut self) {
        self.events.clear();
        self.bank();
    }

    pub fn events(&self) -> &[ScoreEvent] {
        &self.events
    }

    fn emit(&mut self, kind: ScoreEventKind, points: f64) {
        self.events.push(ScoreEvent { kind, points });
    }

    fn bank(&mut self) {
        if !self.combo.active {
            return;
        }
        let value = self.combo.bank();
        self.total += value;
        self.emit(ScoreEventKind::Banked, value.total());
    }

    fn check_hard_hits(&mut self, contacts: &[Contact]) {
        let worst = contacts.iter().map(|c| c.impact_speed).fold(0.0, f64::max);
        if worst >= self.params.combo.hard_hit_speed {
            self.combo.lose();
            self.drift.interrupt();
            self.total.penalty += self.params.combo.hard_hit_penalty;
            self.emit(ScoreEventKind::HardHit, -self.params.combo.hard_hit_penalty);
        }
    }

    fn check_wall_taps(&mut self, world: &World, contacts: &[Contact], speed_mult: f64) {
        let p = &self.params.style;
        let car = &world.car;
        let rear = -car.params.body.length / 2.0 + REAR_BUMPER_ZONE;
        for c in contacts {
            let local_x = (c.point - car.state.position).to_local(car.state.heading).x;
            let is_tap = c.kind == SegmentKind::HardWall
                && local_x < rear
                && (p.wall_tap_min_speed..=p.wall_tap_max_speed).contains(&c.impact_speed);
            let allowed = self.time - self.last_tap >= p.wall_tap_cooldown_secs && self.combo.taps < p.max_taps_per_combo;
            if is_tap && allowed {
                let points = p.wall_tap_points * speed_mult;
                self.combo.pending.style += points;
                self.combo.taps += 1;
                self.combo.bump(&self.params.combo);
                self.last_tap = self.time;
                self.emit(ScoreEventKind::WallTap, points);
                return;
            }
        }
    }

    /// A bobble: the angle drops well below its recent peak, then comes back
    /// quickly. That's a correction, not a deliberate change of angle.
    fn check_bobble(&mut self, deg: f64, dt: f64) {
        let p = &self.params.style;
        let b = &mut self.bobble;
        b.peak = deg.max(b.peak - PEAK_DECAY_DEG_PER_SEC * dt);
        match b.dip {
            None if deg < b.peak - p.bobble_drop_deg => b.dip = Some((self.time, deg)),
            None => {}
            Some((start, low)) => {
                let low = low.min(deg);
                b.dip = Some((start, low));
                if self.time - start > p.bobble_window_secs {
                    // Held the lower angle: deliberate, not a bobble.
                    b.dip = None;
                    b.peak = deg;
                } else if deg > low + p.bobble_drop_deg / 2.0 {
                    b.dip = None;
                    b.peak = deg;
                    let penalty = p.bobble_penalty;
                    self.combo.pending.penalty += penalty;
                    self.emit(ScoreEventKind::Bobble, -penalty);
                }
            }
        }
    }

    fn update_zones(&mut self, world: &World, scoring: bool, speed_mult: f64, dt: f64) {
        let track = world.track.clone();
        let lap_of = |start: f64| ((world.progress - start) / track.length).floor() as i64;
        for (i, zone) in track.zones.iter().enumerate() {
            match zone.kind {
                ZoneKind::Outer => {
                    let inside = track.in_range(world.place.distance, zone.start, zone.end);
                    let line = &self.params.line;
                    // Only measure where it can count (it's a few projections).
                    let closeness = if inside && scoring {
                        let gap = rear_gap_to_edge(world, &track, zone.side);
                        proximity(gap, line.outer_full_within, line.outer_zero_beyond)
                    } else {
                        0.0
                    };
                    let state = &mut self.outers[i];
                    if !inside {
                        state.entered = false;
                    }
                    state.inside = inside;
                    let earning = inside && scoring && closeness > 0.0;
                    self.zone_lit[i] = earning;
                    if earning {
                        self.combo.pending.line += line.outer_points_per_sec * closeness * speed_mult * dt;
                        if !state.entered {
                            state.entered = true;
                            self.combo.bump(&self.params.combo);
                            self.emit(ScoreEventKind::ZoneEnter, 0.0);
                        }
                    }
                }
                ZoneKind::Clip => {
                    let line = &self.params.line;
                    let delta = track.delta(zone.start, world.place.distance);
                    let in_window = (-line.clip_window_before..=line.clip_window_after).contains(&delta);
                    let state = &mut self.clips[i];
                    if in_window && scoring {
                        let distance = front_distance_to(world, zone.point);
                        state.best = Some(state.best.map_or(distance, |b: f64| b.min(distance)));
                    }
                    let left_window = state.in_window && !in_window;
                    state.in_window = in_window;
                    self.zone_lit[i] = in_window && scoring;
                    if left_window {
                        let best = state.best.take();
                        let lap = lap_of(zone.start);
                        if let Some(best) = best
                            && state.paid_lap != Some(lap)
                            && self.combo.active
                        {
                            let quality = proximity(best, line.clip_full_within, line.clip_zero_beyond);
                            if quality > 0.0 {
                                state.paid_lap = Some(lap);
                                let points = line.clip_points * quality * speed_mult;
                                self.combo.pending.line += points;
                                self.combo.bump(&self.params.combo);
                                self.emit(ScoreEventKind::Clip { quality }, points);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Car-frame points along a bumper: both corners and the middle.
fn bumper_points(world: &World, front: bool) -> [Vec2; 3] {
    let body = &world.car.params.body;
    let s = &world.car.state;
    let x = if front { body.length / 2.0 } else { -body.length / 2.0 };
    let y = body.width / 2.0;
    [Vec2::new(x, y), Vec2::new(x, 0.0), Vec2::new(x, -y)].map(|p| s.position + p.rotate(s.heading))
}

fn front_distance_to(world: &World, point: Vec2) -> f64 {
    bumper_points(world, true).iter().map(|p| p.distance(point)).fold(f64::MAX, f64::min)
}

/// Closest the rear bumper is to the track edge on `side`, metres (0 when at
/// or past it).
fn rear_gap_to_edge(world: &World, track: &Track, side: crate::track::Side) -> f64 {
    let hint = Some(world.place.index);
    bumper_points(world, false)
        .iter()
        .map(|&p| {
            let TrackPoint { offset, .. } = track.project(p, hint);
            (track.half_width() - offset * side.sign()).max(0.0)
        })
        .fold(f64::MAX, f64::min)
}
