//! The visual app. For now: drive the car around a track with telemetry.

mod app;
mod dashboard;
mod debug;
mod input;
mod paths;
mod render;
mod screens;
mod ui;
mod video;

use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Drift".to_owned(),
        window_width: 1600,
        window_height: 900,
        sample_count: 4,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut app = app::App::new(app::Options::from_args());
    while app.frame() == app::Flow::Continue {
        next_frame().await;
    }
}
