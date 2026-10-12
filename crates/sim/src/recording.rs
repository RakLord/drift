//! Recording a run for replay: state frames at a fixed rate, plus the
//! things that happen between them (wall contacts, score events, zones
//! lighting up) and what the driver saw at each decision.
//!
//! Ticks count from the start of the run. A frame or record at tick `t`
//! shows the state after `t` ticks; a decision at tick `t` was made before
//! tick `t` was simulated.

use crate::World;
use crate::frame::Frame;
use crate::scoring::{ScoreEvent, Scorer};
use crate::sensors::observation::Observation;
use crate::world::Contact;

/// Ticks per frame: 4 at 120 Hz = 30 frames a second.
pub const FRAME_EVERY: u32 = 4;

#[derive(Clone, Copy, Debug)]
pub struct ContactRecord {
    pub tick: u64,
    pub contact: Contact,
}

#[derive(Clone, Copy, Debug)]
pub struct EventRecord {
    pub tick: u64,
    pub event: ScoreEvent,
}

/// A zone starting or stopping being scored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneRecord {
    pub tick: u64,
    pub zone: u32,
    pub lit: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionRecord {
    pub tick: u64,
    pub inputs: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct Recording {
    pub frame_every: u32,
    /// Frame `i` is at tick `i * frame_every`, except the last, which is at
    /// `ticks` (where the run ended).
    pub frames: Vec<Frame>,
    pub ticks: u64,
    pub contacts: Vec<ContactRecord>,
    pub events: Vec<EventRecord>,
    pub zones: Vec<ZoneRecord>,
    pub decisions: Vec<DecisionRecord>,
}

impl Recording {
    pub fn frame_tick(&self, i: usize) -> u64 {
        (i as u64 * u64::from(self.frame_every)).min(self.ticks)
    }

    /// The state at `tick` (fractional), interpolated between frames.
    pub fn frame_at(&self, tick: f64) -> Frame {
        let Some(last) = self.frames.len().checked_sub(1) else { return Frame::default() };
        let tick = tick.clamp(0.0, self.ticks as f64);
        let i = ((tick / f64::from(self.frame_every.max(1))) as usize).min(last);
        if i == last {
            return self.frames[last];
        }
        let (from, to) = (self.frame_tick(i) as f64, self.frame_tick(i + 1) as f64);
        let t = if to > from { (tick - from) / (to - from) } else { 0.0 };
        self.frames[i].lerp(&self.frames[i + 1], t.clamp(0.0, 1.0))
    }

    /// Contacts in ticks `from + 1 ..= to` (what happened after `from`).
    pub fn contacts_between(&self, from: u64, to: u64) -> &[ContactRecord] {
        between(&self.contacts, |r| r.tick, from, to)
    }

    pub fn events_between(&self, from: u64, to: u64) -> &[EventRecord] {
        between(&self.events, |r| r.tick, from, to)
    }

    /// Which zones are lit at `tick`.
    pub fn zones_lit_at(&self, tick: u64, zones: usize) -> Vec<bool> {
        let mut lit = vec![false; zones];
        for r in self.zones.iter().take_while(|r| r.tick <= tick) {
            if let Some(z) = lit.get_mut(r.zone as usize) {
                *z = r.lit;
            }
        }
        lit
    }

    /// The latest decision made at or before `tick`, and its index.
    pub fn decision_at(&self, tick: u64) -> Option<(usize, &DecisionRecord)> {
        let n = self.decisions.partition_point(|d| d.tick <= tick);
        n.checked_sub(1).map(|i| (i, &self.decisions[i]))
    }
}

fn between<T>(records: &[T], tick: impl Fn(&T) -> u64, from: u64, to: u64) -> &[T] {
    let start = records.partition_point(|r| tick(r) <= from);
    let end = records.partition_point(|r| tick(r) <= to);
    &records[start..end.max(start)]
}

/// Builds a `Recording` as a run goes.
pub struct Recorder {
    recording: Recording,
    zone_lit: Vec<bool>,
}

impl Recorder {
    /// Start recording, with the run as it is now as the first frame.
    pub fn new(world: &World, scorer: &Scorer) -> Self {
        let recording =
            Recording { frame_every: FRAME_EVERY, frames: vec![Frame::capture(world, scorer)], ..Recording::default() };
        Self { recording, zone_lit: scorer.zone_lit.clone() }
    }

    pub fn ticks(&self) -> u64 {
        self.recording.ticks
    }

    /// The driver decided, seeing `observation`, before the next tick.
    pub fn on_decision(&mut self, observation: &Observation) {
        let tick = self.recording.ticks;
        self.recording.decisions.push(DecisionRecord { tick, inputs: observation.values.to_vec() });
    }

    /// A tick was simulated and scored.
    pub fn on_tick(&mut self, world: &World, scorer: &Scorer) {
        let r = &mut self.recording;
        r.ticks += 1;
        let tick = r.ticks;
        r.contacts.extend(world.contacts().iter().map(|&contact| ContactRecord { tick, contact }));
        self.on_events(scorer.events());
        let r = &mut self.recording;
        for (zone, (&now, was)) in scorer.zone_lit.iter().zip(&mut self.zone_lit).enumerate() {
            if now != *was {
                r.zones.push(ZoneRecord { tick, zone: zone as u32, lit: now });
                *was = now;
            }
        }
        if tick.is_multiple_of(u64::from(r.frame_every)) {
            r.frames.push(Frame::capture(world, scorer));
        }
    }

    /// Score events outside a normal tick (banking at the final bell).
    pub fn on_events(&mut self, events: &[ScoreEvent]) {
        let tick = self.recording.ticks;
        self.recording.events.extend(events.iter().map(|&event| EventRecord { tick, event }));
    }

    /// Stop, with the run as it is now as the last frame.
    pub fn finish(mut self, world: &World, scorer: &Scorer) -> Recording {
        let r = &mut self.recording;
        let last = Frame::capture(world, scorer);
        if r.ticks.is_multiple_of(u64::from(r.frame_every)) && r.frames.len() > 1 {
            // Already have a frame here; refresh it (e.g. a final bank).
            *r.frames.last_mut().expect("frames") = last;
        } else if r.ticks > 0 {
            r.frames.push(last);
        }
        self.recording
    }
}
