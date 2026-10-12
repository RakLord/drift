//! Tyre smoke: soft puffs emitted at sliding contact patches. They drift with
//! some of the car's speed, slow down, swell, and fade.

use common::{Rng, Vec2};
use macroquad::prelude::*;
use sim::car::{CarState, WheelState};

use crate::render::{theme, to_mq};

const MAX_PARTICLES: usize = 4000;
/// Puffs per second per wheel at full intensity.
const EMIT_RATE: f64 = 90.0;
const MIN_INTENSITY: f64 = 0.05;
/// Share of the contact patch's ground velocity a new puff carries.
const INHERIT_VELOCITY: f64 = 0.25;
const DRAG: f64 = 1.6;
const PEAK_ALPHA: f64 = 0.22;

struct Puff {
    position: Vec2,
    velocity: Vec2,
    radius: f64,
    growth: f64,
    age: f64,
    life: f64,
    strength: f64,
    rotation: f64,
    spin: f64,
}

pub struct Smoke {
    puffs: Vec<Puff>,
    /// Fractional puffs owed to each wheel, so emission is smooth at any rate.
    owed: [f64; 4],
    rng: Rng,
    texture: Texture2D,
}

impl Smoke {
    pub fn new() -> Self {
        let mut rng = Rng::new(0x5307e);
        let texture = puff_texture(&mut rng);
        Self { puffs: Vec::with_capacity(MAX_PARTICLES), owed: [0.0; 4], rng, texture }
    }

    pub fn emit(&mut self, wheel_index: usize, at: Vec2, car: &CarState, wheel: &WheelState, intensity: f64, dt: f64) {
        if intensity < MIN_INTENSITY {
            self.owed[wheel_index] = 0.0;
            return;
        }
        self.owed[wheel_index] += EMIT_RATE * intensity * dt;
        // Velocity of the patch over the ground, world frame.
        let offset = at - car.position;
        let ground_velocity = car.velocity + offset.perp() * car.yaw_rate;

        while self.owed[wheel_index] >= 1.0 {
            self.owed[wheel_index] -= 1.0;
            if self.puffs.len() >= MAX_PARTICLES {
                self.puffs.swap_remove(0);
            }
            let rng = &mut self.rng;
            let jitter = Vec2::new(rng.gaussian(), rng.gaussian());
            self.puffs.push(Puff {
                position: at + jitter * 0.15,
                velocity: ground_velocity * INHERIT_VELOCITY + jitter * (0.8 + wheel.slip_speed * 0.05),
                radius: rng.range(0.35, 0.6),
                growth: rng.range(1.2, 2.2),
                age: 0.0,
                life: rng.range(1.6, 3.0),
                strength: intensity.min(1.0),
                rotation: rng.range(0.0, std::f64::consts::TAU),
                spin: rng.range(-0.8, 0.8),
            });
        }
    }

    pub fn update(&mut self, dt: f64) {
        let damping = (-DRAG * dt).exp();
        for puff in &mut self.puffs {
            puff.age += dt;
            puff.position += puff.velocity * dt;
            puff.velocity *= damping;
            // Swell quickly at first, then slowly.
            puff.radius += puff.growth * dt / (1.0 + puff.age);
            puff.rotation += puff.spin * dt;
        }
        self.puffs.retain(|p| p.age < p.life);
    }

    pub fn clear(&mut self) {
        self.puffs.clear();
        self.owed = [0.0; 4];
    }

    /// Draw with the world camera active.
    pub fn draw(&self) {
        for puff in &self.puffs {
            let t = puff.age / puff.life;
            // Fade in over the first 10% of life, out over the rest.
            let fade = if t < 0.1 { t / 0.1 } else { (1.0 - t) / 0.9 };
            let alpha = (PEAK_ALPHA * puff.strength * fade * fade.sqrt()) as f32;
            let color = Color { a: alpha, ..theme::SMOKE };
            let size = (puff.radius * 2.0) as f32;
            let p = to_mq(puff.position);
            draw_texture_ex(
                &self.texture,
                p.x - size / 2.0,
                p.y - size / 2.0,
                color,
                DrawTextureParams {
                    dest_size: Some(vec2(size, size)),
                    rotation: puff.rotation as f32,
                    ..Default::default()
                },
            );
        }
    }
}

/// A soft, slightly lumpy white blob.
fn puff_texture(rng: &mut Rng) -> Texture2D {
    const SIZE: u16 = 64;
    let mut image = Image::gen_image_color(SIZE, SIZE, Color::new(1.0, 1.0, 1.0, 0.0));
    let half = f64::from(SIZE) / 2.0;
    // A few overlapping lobes make the edge irregular.
    let lobes: Vec<(f64, f64, f64)> = (0..5)
        .map(|_| (rng.range(-0.25, 0.25), rng.range(-0.25, 0.25), rng.range(0.55, 0.8)))
        .collect();
    for y in 0..u32::from(SIZE) {
        for x in 0..u32::from(SIZE) {
            let px = (f64::from(x) + 0.5 - half) / half;
            let py = (f64::from(y) + 0.5 - half) / half;
            let density = lobes
                .iter()
                .map(|&(cx, cy, r)| {
                    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() / r;
                    (1.0 - d).max(0.0).powi(2)
                })
                .fold(0.0, f64::max);
            let alpha = (density * rng.range(0.85, 1.0)).min(1.0) as f32;
            image.set_pixel(x, y, Color::new(1.0, 1.0, 1.0, alpha));
        }
    }
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    texture
}
