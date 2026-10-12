//! Free-drive screen: one car on the open lot, driven by you.

use std::path::PathBuf;

use common::config::WatchedConfig;
use common::math::{angle_diff, wrap_angle};
use common::Vec2;
use macroquad::prelude::*;
use sim::car::{Car, CarParams, CarState};

use crate::debug::{gamepad_panel, history::History, telemetry};
use crate::input::{autopilot::Autopilot, Input};
use crate::render::{self, camera::FollowCamera, effects::Effects, ground, hud, text, theme};

const TICK: f64 = 1.0 / 120.0;
/// Never simulate more than this many ticks per frame (avoids a death spiral
/// after a stall, e.g. dragging the window).
const MAX_TICKS_PER_FRAME: u32 = 12;
const CONFIG_POLL_SECS: f64 = 0.5;
const TIME_SCALES: [f64; 5] = [0.1, 0.25, 0.5, 1.0, 2.0];

const START_POSITION: Vec2 = Vec2::ZERO;
const START_HEADING: f64 = 0.0;

pub struct PlayScreen {
    car: Car,
    previous: Pose,
    config: Option<WatchedConfig<CarParams>>,
    config_timer: f64,
    input: Input,
    autopilot: Option<Autopilot>,
    camera: FollowCamera,
    history: History,
    effects: Effects,
    accumulator: f64,
    time_scale_index: usize,
    paused: bool,
    show_effects: bool,
    show_telemetry: bool,
    show_gamepad: bool,
    show_help: bool,
    toast: Option<(String, f64)>,
}

#[derive(Clone, Copy)]
struct Pose {
    position: Vec2,
    heading: f64,
}

impl Pose {
    fn of(state: &CarState) -> Self {
        Self { position: state.position, heading: state.heading }
    }
}

impl PlayScreen {
    pub fn new(autopilot: bool) -> Self {
        let (config, params, toast) = match WatchedConfig::<CarParams>::load(car_config_path()) {
            Ok(config) => {
                let params = config.get().clone();
                (Some(config), params, None)
            }
            Err(err) => (None, CarParams::default(), Some((format!("using built-in car: {err}"), 6.0))),
        };
        let car = Car::new(params, START_POSITION, START_HEADING);
        Self {
            previous: Pose::of(&car.state),
            car,
            config,
            config_timer: 0.0,
            input: Input::new(),
            autopilot: autopilot.then(Autopilot::new),
            camera: FollowCamera::new(START_POSITION),
            history: History::new(),
            effects: Effects::new(),
            accumulator: 0.0,
            time_scale_index: 3,
            paused: false,
            show_effects: true,
            show_telemetry: true,
            show_gamepad: false,
            show_help: true,
            toast,
        }
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1);
        self.handle_keys();
        self.reload_config(dt);
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }

        // Read input every frame (the gamepad needs polling) even if paused.
        let input = self.input.controls(dt);
        if self.input.reset_pressed() {
            self.reset();
        }
        if self.paused {
            return;
        }

        self.accumulator += dt * TIME_SCALES[self.time_scale_index];
        let mut ticks = 0;
        while self.accumulator >= TICK && ticks < MAX_TICKS_PER_FRAME {
            let controls = match &mut self.autopilot {
                Some(autopilot) => autopilot.controls(&self.car.state, &self.car.params, TICK),
                None => input,
            };
            self.previous = Pose::of(&self.car.state);
            self.car.step(controls, TICK);
            self.history.record(&self.car.state);
            self.effects.on_tick(&self.car.state, &self.car.params, TICK);
            self.accumulator -= TICK;
            ticks += 1;
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }

        let shown = self.interpolated();
        self.camera.update(shown.position, self.car.state.velocity, dt);
    }

    pub fn draw(&self) {
        let shown = self.interpolated();
        let mut state = self.car.state.clone();
        state.position = shown.position;
        state.heading = shown.heading;

        self.camera.apply();
        ground::draw(&self.camera);
        if self.show_effects {
            self.effects.draw_ground(&self.camera);
        }
        render::car::draw(&state, &self.car.params);
        if self.show_effects {
            self.effects.draw_air();
        }
        if self.show_telemetry {
            telemetry::draw_world(&state, &self.car.params);
        }

        set_default_camera();
        hud::draw(&state, &self.car.params);
        if self.show_telemetry {
            telemetry::draw_panel(&state, &self.car.params);
            self.history.draw();
        }
        if self.show_gamepad {
            gamepad_panel::draw(&self.input.gamepad);
        }
        self.draw_status();
    }

    /// Render pose between the last two ticks, so motion is smooth at any
    /// refresh rate.
    fn interpolated(&self) -> Pose {
        let alpha = (self.accumulator / TICK).clamp(0.0, 1.0);
        let current = Pose::of(&self.car.state);
        Pose {
            position: self.previous.position.lerp(current.position, alpha),
            heading: wrap_angle(self.previous.heading + angle_diff(self.previous.heading, current.heading) * alpha),
        }
    }

    fn reset(&mut self) {
        self.car.state = CarState::at_rest(&self.car.params, START_POSITION, START_HEADING);
        self.previous = Pose::of(&self.car.state);
        self.camera.snap_to(START_POSITION);
        self.history.clear();
        self.effects.clear();
        if self.autopilot.is_some() {
            self.autopilot = Some(Autopilot::new());
        }
    }

    fn handle_keys(&mut self) {
        if is_key_pressed(KeyCode::R) {
            self.reset();
        }
        if is_key_pressed(KeyCode::P) {
            self.paused = !self.paused;
        }
        if is_key_pressed(KeyCode::E) {
            self.show_effects = !self.show_effects;
        }
        if is_key_pressed(KeyCode::T) {
            self.show_telemetry = !self.show_telemetry;
        }
        if is_key_pressed(KeyCode::G) {
            self.show_gamepad = !self.show_gamepad;
        }
        if is_key_pressed(KeyCode::H) {
            self.show_help = !self.show_help;
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.time_scale_index = self.time_scale_index.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            self.time_scale_index = (self.time_scale_index + 1).min(TIME_SCALES.len() - 1);
        }
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            self.camera.zoom_by(if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 });
        }
    }

    fn reload_config(&mut self, dt: f64) {
        self.config_timer += dt;
        if self.config_timer < CONFIG_POLL_SECS {
            return;
        }
        self.config_timer = 0.0;
        let Some(config) = &mut self.config else { return };
        match config.poll() {
            Some(Ok(())) => {
                self.car.params = config.get().clone();
                self.toast = Some(("car.toml reloaded".to_owned(), 2.0));
            }
            Some(Err(err)) => self.toast = Some((format!("car.toml rejected: {err}"), 6.0)),
            None => {}
        }
    }

    fn draw_status(&self) {
        let (w, h) = (screen_width(), screen_height());
        let scale = TIME_SCALES[self.time_scale_index];
        let mut status = vec![];
        if self.paused {
            status.push("PAUSED".to_owned());
        }
        if scale != 1.0 {
            status.push(format!("{scale}x speed"));
        }
        if self.autopilot.is_some() {
            status.push("AUTOPILOT".to_owned());
        }
        if !status.is_empty() {
            text(&status.join("   "), w / 2.0 - 80.0, 40.0, 28.0, theme::SODIUM);
        }
        if let Some((message, _)) = &self.toast {
            text(message, w / 2.0 - 160.0, 72.0, 22.0, theme::NEON_CYAN);
        }

        let help: &[&str] = if self.show_help {
            &[
                "pad: stick steer  RT throttle  LT brake  A/RB handbrake  X/LB clutch  Start reset",
                "keys: WASD/arrows  Space handbrake  Shift clutch  R reset  P pause  [ ] speed  \
                 E effects  T telemetry  G gamepad  H help  wheel zoom",
            ]
        } else {
            &["H: help"]
        };
        for (i, line) in help.iter().rev().enumerate() {
            text(line, 16.0, h - 8.0 - i as f32 * 18.0, 16.0, theme::TEXT_DIM);
        }
    }
}

/// `config/car.toml` relative to where the viewer was started, falling back
/// to the workspace copy.
fn car_config_path() -> PathBuf {
    let local = PathBuf::from("config/car.toml");
    if local.exists() {
        local
    } else {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/car.toml"))
    }
}
