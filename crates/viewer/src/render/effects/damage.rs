//! Cosmetic bumper damage: a solid knock loosens a bumper, a big one (or a
//! second solid one) rips it off and leaves it lying on the track.

use common::{Rng, Vec2};
use sim::car::{CarParams, CarState};
use sim::world::Contact;

use crate::render::{draw_box, theme};

/// Impact speed (m/s) that loosens a bumper.
const LOOSEN_SPEED: f64 = 3.5;
/// Impact speed (m/s) that tears a bumper straight off.
const DETACH_SPEED: f64 = 7.0;
/// Contacts this close to the end of the car count as bumper hits, metres.
const BUMPER_ZONE: f64 = 0.7;
pub const BUMPER_DEPTH: f64 = 0.28;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Bumper {
    #[default]
    Fine,
    Loose,
    Gone,
}

/// A bumper lying on the ground after coming off.
struct Debris {
    position: Vec2,
    velocity: Vec2,
    heading: f64,
    spin: f64,
}

pub struct Damage {
    pub front: Bumper,
    pub rear: Bumper,
    debris: Vec<Debris>,
    rng: Rng,
}

impl Damage {
    pub fn new() -> Self {
        Self { front: Bumper::Fine, rear: Bumper::Fine, debris: vec![], rng: Rng::new(0xb0b) }
    }

    pub fn on_contact(&mut self, contact: &Contact, car: &CarState, params: &CarParams) {
        let local = (contact.point - car.position).to_local(car.heading);
        let half = params.body.length / 2.0;
        let is_front = local.x > half - BUMPER_ZONE;
        if !is_front && local.x > -half + BUMPER_ZONE {
            return; // Side swipe: no bumper involved.
        }
        let bumper = if is_front { &mut self.front } else { &mut self.rear };
        let next = match (*bumper, contact.impact_speed) {
            (Bumper::Gone, _) => return,
            (_, speed) if speed >= DETACH_SPEED => Bumper::Gone,
            (Bumper::Loose, speed) if speed >= LOOSEN_SPEED => Bumper::Gone,
            (Bumper::Fine, speed) if speed >= LOOSEN_SPEED => Bumper::Loose,
            (current, _) => current,
        };
        *bumper = next;
        if next == Bumper::Gone {
            let end = if is_front { half - BUMPER_DEPTH / 2.0 } else { -half + BUMPER_DEPTH / 2.0 };
            self.debris.push(Debris {
                position: car.position + Vec2::new(end, 0.0).rotate(car.heading),
                velocity: car.velocity * 0.6 + contact.normal * 2.0,
                heading: car.heading,
                spin: self.rng.range(-4.0, 4.0),
            });
        }
    }

    pub fn update(&mut self, dt: f64) {
        // Plastic skidding on asphalt: strong friction, settles fast.
        for d in &mut self.debris {
            let speed = d.velocity.length();
            let slowed = (speed - 6.0 * dt).max(0.0);
            d.velocity = d.velocity.normalize_or_zero() * slowed;
            d.position += d.velocity * dt;
            d.spin *= (-2.0 * dt).exp();
            d.heading += d.spin * dt;
        }
    }

    pub fn clear(&mut self) {
        *self = Self { rng: self.rng.clone(), ..Self::new() };
    }

    /// Bumpers lying on the track. Draw before cars.
    pub fn draw_debris(&self, params: &CarParams) {
        for d in &self.debris {
            draw_box(d.position, bumper_size(params), d.heading, theme::CAR_BODY);
        }
    }
}

pub fn bumper_size(params: &CarParams) -> Vec2 {
    Vec2::new(BUMPER_DEPTH, params.body.width * 0.9)
}
