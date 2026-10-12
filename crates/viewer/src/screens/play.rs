//! Free-drive screen: one car on a track, driven by you.

use std::sync::Arc;

use common::Vec2;
use common::config::WatchedConfig;
use common::math::{angle_diff, wrap_angle};
use macroquad::prelude::*;
use sim::car::{CarParams, CarState};
use sim::scoring::{Scorer, ScoringParams};
use sim::track::{Track, TrackDef};
use sim::World;

use crate::debug::{gamepad_panel, history::History, telemetry};
use crate::input::{Input, autopilot::Autopilot};
use crate::paths::workspace_file;
use crate::render::score::ScoreHud;
use crate::render::{self, camera::FollowCamera, effects::Effects, ground, hud, text, theme, track::TrackMesh};

const TICK: f64 = 1.0 / 120.0;
/// Never simulate more than this many ticks per frame (avoids a death spiral
/// after a stall, e.g. dragging the window).
const MAX_TICKS_PER_FRAME: u32 = 12;
const CONFIG_POLL_SECS: f64 = 0.5;
const TIME_SCALES: [f64; 5] = [0.1, 0.25, 0.5, 1.0, 2.0];

const CAR_FILE: &str = "config/car.toml";
const SCORING_FILE: &str = "config/scoring.toml";
const TRACK_FILE: &str = "tracks/test_loop.toml";
const BUILT_IN_TRACK: &str = include_str!("../../../../tracks/test_loop.toml");

pub struct PlayScreen {
    world: World,
    mesh: TrackMesh,
    scorer: Scorer,
    score_hud: ScoreHud,
    previous: Pose,
    car_config: Option<WatchedConfig<CarParams>>,
    track_config: Option<WatchedConfig<TrackDef>>,
    scoring_config: Option<WatchedConfig<ScoringParams>>,
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
        let mut warnings = vec![];
        let car_config = WatchedConfig::<CarParams>::load(workspace_file(CAR_FILE))
            .map_err(|err| warnings.push(format!("using built-in car: {err}")))
            .ok();
        let params = car_config.as_ref().map_or_else(CarParams::default, |c| c.get().clone());

        let track_config = WatchedConfig::<TrackDef>::load(workspace_file(TRACK_FILE))
            .map_err(|err| warnings.push(format!("using built-in track: {err}")))
            .ok();
        let track = track_config
            .as_ref()
            .and_then(|c| Track::build(c.get().clone()).map_err(|err| warnings.push(format!("track: {err}"))).ok())
            .unwrap_or_else(built_in_track);

        let scoring_config = WatchedConfig::<ScoringParams>::load(workspace_file(SCORING_FILE))
            .map_err(|err| warnings.push(format!("using built-in scoring: {err}")))
            .ok();
        let scoring = scoring_config.as_ref().map_or_else(ScoringParams::default, |c| c.get().clone());

        let mesh = TrackMesh::new(&track);
        let scorer = Scorer::new(scoring, &track);
        let world = World::new(Arc::new(track), params);
        let start = world.car.state.position;
        Self {
            previous: Pose::of(&world.car.state),
            world,
            mesh,
            scorer,
            score_hud: ScoreHud::new(),
            car_config,
            track_config,
            scoring_config,
            config_timer: 0.0,
            input: Input::new(),
            autopilot: autopilot.then(Autopilot::new),
            camera: FollowCamera::new(start),
            history: History::new(),
            effects: Effects::new(),
            accumulator: 0.0,
            time_scale_index: 3,
            paused: false,
            show_effects: true,
            show_telemetry: true,
            show_gamepad: false,
            show_help: true,
            toast: (!warnings.is_empty()).then(|| (warnings.join("; "), 8.0)),
        }
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1);
        self.handle_keys();
        self.reload_configs(dt);
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }

        self.score_hud.update(dt);

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
                Some(autopilot) => autopilot.controls(&self.world, TICK),
                None => input,
            };
            self.previous = Pose::of(&self.world.car.state);
            self.world.step(controls, TICK);
            let car = &self.world.car;
            self.effects.on_tick(&car.state, &car.params, self.world.contacts(), TICK);
            let events = self.scorer.update(&self.world, self.world.contacts(), TICK);
            self.score_hud.on_events(events);
            self.history.record(&car.state);
            self.accumulator -= TICK;
            ticks += 1;
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }

        let shown = self.interpolated();
        self.camera.update(shown.position, self.world.car.state.velocity, dt);
    }

    pub fn draw(&self) {
        let shown = self.interpolated();
        let car = &self.world.car;
        let mut state = car.state.clone();
        state.position = shown.position;
        state.heading = shown.heading;

        self.camera.apply();
        ground::draw(&self.camera);
        self.mesh.draw(&self.camera);
        self.mesh.draw_zones(&self.scorer.zone_lit);
        if self.show_effects {
            self.effects.draw_ground(&self.camera, &car.params);
        }
        render::car::draw(&state, &car.params, &self.effects.damage);
        if self.show_effects {
            self.effects.draw_air();
        }
        if self.show_telemetry {
            telemetry::draw_world(&state, &car.params);
        }

        set_default_camera();
        hud::draw(&state, &car.params);
        hud::draw_track_info(&self.world);
        self.score_hud.draw(&self.scorer);
        if self.show_telemetry {
            telemetry::draw_panel(&state, &car.params);
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
        let current = Pose::of(&self.world.car.state);
        Pose {
            position: self.previous.position.lerp(current.position, alpha),
            heading: wrap_angle(self.previous.heading + angle_diff(self.previous.heading, current.heading) * alpha),
        }
    }

    fn reset(&mut self) {
        self.world.reset();
        self.previous = Pose::of(&self.world.car.state);
        self.camera.snap_to(self.world.car.state.position);
        self.history.clear();
        self.effects.clear();
        self.scorer = Scorer::new(self.scorer.params.clone(), &self.world.track);
        self.score_hud.clear();
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

    fn reload_configs(&mut self, dt: f64) {
        self.config_timer += dt;
        if self.config_timer < CONFIG_POLL_SECS {
            return;
        }
        self.config_timer = 0.0;

        if let Some(config) = &mut self.car_config {
            match config.poll() {
                Some(Ok(())) => {
                    self.world.car.params = config.get().clone();
                    self.toast = Some(("car.toml reloaded".to_owned(), 2.0));
                }
                Some(Err(err)) => self.toast = Some((format!("car.toml rejected: {err}"), 6.0)),
                None => {}
            }
        }

        if let Some(config) = &mut self.scoring_config {
            match config.poll() {
                Some(Ok(())) => {
                    self.scorer.params = config.get().clone();
                    self.toast = Some(("scoring.toml reloaded".to_owned(), 2.0));
                }
                Some(Err(err)) => self.toast = Some((format!("scoring.toml rejected: {err}"), 6.0)),
                None => {}
            }
        }

        if let Some(config) = &mut self.track_config {
            match config.poll() {
                Some(Ok(())) => match Track::build(config.get().clone()) {
                    Ok(track) => {
                        self.mesh = TrackMesh::new(&track);
                        self.world = World::new(Arc::new(track), self.world.car.params.clone());
                        self.reset();
                        self.toast = Some(("track reloaded".to_owned(), 2.0));
                    }
                    Err(err) => self.toast = Some((format!("track rejected: {err}"), 6.0)),
                },
                Some(Err(err)) => self.toast = Some((format!("track rejected: {err}"), 6.0)),
                None => {}
            }
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
            text(&status.join("   "), w / 2.0 - 80.0, h - 70.0, 28.0, theme::SODIUM);
        }
        if let Some((message, _)) = &self.toast {
            text(message, w / 2.0 - 160.0, h - 100.0, 22.0, theme::NEON_CYAN);
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

fn built_in_track() -> Track {
    let def: TrackDef = toml::from_str(BUILT_IN_TRACK).expect("built-in track is invalid");
    Track::build(def).expect("built-in track does not build")
}
