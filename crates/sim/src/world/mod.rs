//! One car on one track: surface lookup, physics, collisions, and progress.
//!
//! Cars never interact, so many cars on a track are just many `World`s
//! sharing the same `Arc<Track>`.

pub mod collision;

use std::sync::Arc;

use crate::Controls;
use crate::car::{Car, CarParams, CarState, Ground};
use crate::track::{Track, TrackPoint};
pub use collision::Contact;

pub struct World {
    pub track: Arc<Track>,
    pub car: Car,
    /// Where the car's centre is relative to the track.
    pub place: TrackPoint,
    /// Signed distance driven along the track since the start, metres.
    /// Going backwards counts negative; laps keep adding up.
    pub progress: f64,
    contacts: Vec<Contact>,
    scratch: Vec<u32>,
}

impl World {
    pub fn new(track: Arc<Track>, params: CarParams) -> Self {
        let (position, heading) = track.start();
        let car = Car::new(params, position, heading);
        let place = track.project(position, None);
        Self { track, car, place, progress: 0.0, contacts: vec![], scratch: vec![] }
    }

    /// Put the car back at the start, standing still.
    pub fn reset(&mut self) {
        let (position, heading) = self.track.start();
        self.car.state = CarState::at_rest(&self.car.params, position, heading);
        self.place = self.track.project(position, None);
        self.progress = 0.0;
        self.contacts.clear();
    }

    /// Replace the car's state (e.g. to place it mid-track for a test).
    pub fn set_car_state(&mut self, state: CarState) {
        self.car.state = state;
        self.place = self.track.project(self.car.state.position, None);
    }

    /// Wall impacts from the last tick.
    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }

    /// Advance one tick. Returns the wall impacts that happened during it.
    pub fn step(&mut self, controls: Controls, dt: f64) -> &[Contact] {
        self.contacts.clear();
        let hint = Some(self.place.index);
        let wheels = self.car.state.wheel_positions(&self.car.params);
        let ground: [Ground; 4] = wheels.map(|p| self.track.ground_at(p, hint));

        self.car.step_on(controls, &ground, dt);
        collision::resolve(&self.track, &mut self.car, &mut self.scratch, &mut self.contacts);

        let place = self.track.project(self.car.state.position, hint);
        self.progress += self.track.delta(self.place.distance, place.distance);
        self.place = place;
        &self.contacts
    }
}
