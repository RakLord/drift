//! Top-level app: owns the active screen and dev options.

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::screens::play::PlayScreen;

#[derive(Default)]
pub struct Options {
    /// Dev tool: save a PNG after this many frames, then exit.
    pub screenshot: Option<(PathBuf, u32)>,
    /// Dev tool: a scripted driver laps the track.
    pub autopilot: bool,
    /// Dev tool: a scripted driver does a throttle-only doughnut.
    pub donut: bool,
    /// Start on the generated track for this seed instead of the library track.
    pub seed: Option<u64>,
    /// Start in map view.
    pub map_view: bool,
    /// Start with the generator panel open.
    pub generator: bool,
    /// A trained genome drives instead of you.
    pub genome: Option<PathBuf>,
    /// Keep loading the newest best genome as training writes it.
    pub follow: bool,
}

impl Options {
    /// `[--seed N] [--map] [--generator] [--genome path | --follow run] [--screenshot <path> [--frames N]] [--autopilot] [--donut]`
    pub fn from_args() -> Self {
        let mut options = Self::default();
        let mut frames = 240;
        let mut path = None;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--screenshot" => path = args.next().map(PathBuf::from),
                "--frames" => frames = args.next().and_then(|n| n.parse().ok()).unwrap_or(frames),
                "--autopilot" => options.autopilot = true,
                "--donut" => options.donut = true,
                "--seed" => options.seed = args.next().and_then(|n| n.parse().ok()),
                "--map" => options.map_view = true,
                "--generator" => options.generator = true,
                "--genome" => options.genome = args.next().map(PathBuf::from),
                "--follow" => {
                    // A run name (runs/<name>/best.genome) or a genome path.
                    options.genome = args.next().map(|run| {
                        if run.ends_with(".genome") {
                            PathBuf::from(run)
                        } else {
                            common::paths::workspace_path("runs").join(run).join("best.genome")
                        }
                    });
                    options.follow = true;
                }
                other => eprintln!("ignoring unknown argument {other}"),
            }
        }
        options.screenshot = path.map(|p| (p, frames));
        options
    }
}

#[derive(PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

pub struct App {
    options: Options,
    play: PlayScreen,
    frame: u32,
}

impl App {
    pub fn new(options: Options) -> Self {
        let play = PlayScreen::new(&options);
        Self { options, play, frame: 0 }
    }

    pub fn frame(&mut self) -> Flow {
        // A screenshot run uses a fixed frame time so it is reproducible.
        let dt = if self.options.screenshot.is_some() { 1.0 / 60.0 } else { f64::from(get_frame_time()) };
        self.play.update(dt);
        self.play.draw();
        self.frame += 1;

        if let Some((path, frames)) = &self.options.screenshot
            && self.frame >= *frames
        {
            let mut image = get_screen_data();
            // Blending leaves the framebuffer's alpha below 1 in places; a
            // PNG viewer would show those pixels see-through.
            for pixel in image.bytes.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            image.export_png(&path.to_string_lossy());
            println!("saved {}", path.display());
            return Flow::Exit;
        }
        if is_key_pressed(KeyCode::Escape) {
            return Flow::Exit;
        }
        Flow::Continue
    }
}
