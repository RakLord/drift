//! Sparks from metal (and bumpers) meeting walls.

use common::{Rng, Vec2};
use macroquad::prelude::*;
use sim::world::Contact;

use crate::render::{draw_segment, theme};

const MAX_SPARKS: usize = 1500;
const GRAVITY_LIKE_DRAG: f64 = 4.0;

struct Spark {
    position: Vec2,
    velocity: Vec2,
    age: f64,
    life: f64,
}

pub struct Sparks {
    sparks: Vec<Spark>,
    rng: Rng,
}

impl Sparks {
    pub fn new() -> Self {
        Self { sparks: vec![], rng: Rng::new(0x5a4b) }
    }

    pub fn emit(&mut self, contact: &Contact) {
        let energy = contact.impact_speed * 2.0 + contact.scrape_speed * 0.4;
        let count = (energy * 1.5) as usize;
        let along = contact.normal.perp();
        for _ in 0..count.min(40) {
            if self.sparks.len() >= MAX_SPARKS {
                self.sparks.swap_remove(0);
            }
            let rng = &mut self.rng;
            // Sprayed along the wall in the scrape direction, and off it.
            let spray = along * rng.range(-1.0, 1.0) * contact.scrape_speed.max(3.0)
                + contact.normal * rng.range(0.5, 4.0)
                + Vec2::new(rng.gaussian(), rng.gaussian());
            self.sparks.push(Spark {
                position: contact.point,
                velocity: spray,
                age: 0.0,
                life: rng.range(0.15, 0.5),
            });
        }
    }

    pub fn update(&mut self, dt: f64) {
        let damping = (-GRAVITY_LIKE_DRAG * dt).exp();
        for s in &mut self.sparks {
            s.age += dt;
            s.position += s.velocity * dt;
            s.velocity *= damping;
        }
        self.sparks.retain(|s| s.age < s.life);
    }

    pub fn clear(&mut self) {
        self.sparks.clear();
    }

    pub fn draw(&self) {
        for s in &self.sparks {
            let heat = 1.0 - s.age / s.life;
            let color = Color { a: heat as f32, ..theme::SPARK };
            draw_segment(s.position, s.position - s.velocity * 0.03, 0.06, color);
        }
    }
}
