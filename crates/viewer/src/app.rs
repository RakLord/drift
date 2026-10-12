//! Top-level app: owns the active screen and dev options.

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::screens::play::PlayScreen;
use crate::screens::replay::{self, ReplayScreen};
use crate::screens::Switch;
use crate::screens::watch::WatchScreen;

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
    /// The library track file to drive on (default tracks/test_loop.toml).
    pub track: Option<String>,
    /// Start in map view.
    pub map_view: bool,
    /// Start with the generator panel open.
    pub generator: bool,
    /// A trained genome drives instead of you.
    pub genome: Option<PathBuf>,
    /// Keep loading the newest best genome as training writes it.
    pub follow: bool,
    /// Start with the track library panel open.
    pub library: bool,
    /// Start with the run dashboard open.
    pub charts: bool,
    /// Start with the network view open (split).
    pub net: bool,
    /// Watch this replay file.
    pub replay: Option<PathBuf>,
    /// Open the replay picker (with `--replays <run>`).
    pub replays: bool,
    /// Race this run's top genomes live.
    pub watch: Option<String>,
}

impl Options {
    /// `[--seed N | --track path] [--map] [--generator] [--library] [--genome path | --follow run] [--net]
    /// [--replay file | --replays run | --watch run] [--screenshot <path> [--frames N]] [--autopilot] [--donut]`
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
                "--track" => options.track = args.next(),
                "--map" => options.map_view = true,
                "--generator" => options.generator = true,
                "--library" => options.library = true,
                "--net" => options.net = true,
                "--genome" => options.genome = args.next().map(PathBuf::from),
                "--charts" => options.charts = true,
                "--replay" => options.replay = args.next().map(PathBuf::from),
                "--watch" => options.watch = args.next(),
                "--replays" => {
                    // A run's newest replay, with the picker open.
                    if let Some(run) = args.next() {
                        let dir = common::paths::workspace_path("runs").join(&run);
                        match replay::newest(&dir) {
                            Some(path) => options.replay = Some(path),
                            None => eprintln!("no replays in {}", dir.join("replays").display()),
                        }
                        options.replays = true;
                    }
                }
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

impl App {
    /// Open another view of a run, keeping the play screen to come back to.
    fn open(&mut self, switch: Switch) {
        let next = match &switch {
            Switch::Watch(run) => Ok(Screen::Watch(Box::new(WatchScreen::new(run, false)))),
            Switch::Replays(run) => {
                let dir = common::paths::workspace_path("runs").join(run);
                replay::newest(&dir)
                    .ok_or_else(|| format!("no replays for {run} yet"))
                    .and_then(|path| ReplayScreen::new(path, true, false))
                    .map(|r| Screen::Replay(Box::new(r)))
            }
        };
        match next {
            Ok(next) => match std::mem::replace(&mut self.screen, next) {
                Screen::Play(play) => self.back = Some(play),
                // Watch <-> replays: the play screen (if any) stays put.
                Screen::Replay(_) | Screen::Watch(_) => {}
            },
            Err(err) => match &mut self.screen {
                Screen::Play(play) => play.notify(err),
                Screen::Replay(replay) => replay.notify(err),
                Screen::Watch(watch) => watch.notify(err),
            },
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

enum Screen {
    Play(Box<PlayScreen>),
    Replay(Box<ReplayScreen>),
    Watch(Box<WatchScreen>),
}

pub struct App {
    options: Options,
    screen: Screen,
    /// The play screen to go back to (Esc) from a view opened with Q / J.
    back: Option<Box<PlayScreen>>,
    frame: u32,
}

impl App {
    pub fn new(options: Options) -> Self {
        if let Some(run) = &options.watch {
            let mut watch = WatchScreen::new(run, options.charts);
            if options.net {
                watch.open_network();
            }
            return Self { screen: Screen::Watch(Box::new(watch)), options, back: None, frame: 0 };
        }
        let replay = options.replay.clone().map(|path| ReplayScreen::new(path, options.replays, options.charts));
        let screen = match replay {
            Some(Ok(mut replay)) => {
                if options.net {
                    replay.open_network();
                }
                Screen::Replay(Box::new(replay))
            }
            Some(Err(err)) => {
                eprintln!("{err}");
                Screen::Play(Box::new(PlayScreen::new(&options)))
            }
            None => Screen::Play(Box::new(PlayScreen::new(&options))),
        };
        Self { options, screen, back: None, frame: 0 }
    }

    pub fn frame(&mut self) -> Flow {
        // A screenshot run uses a fixed frame time so it is reproducible.
        // Checked before the frame runs: Esc that cancels typing mustn't quit.
        let typing = matches!(&self.screen, Screen::Play(play) if play.captures_keyboard());
        let dt = if self.options.screenshot.is_some() { 1.0 / 60.0 } else { f64::from(get_frame_time()) };
        let switch = match &mut self.screen {
            Screen::Play(play) => {
                play.update(dt);
                play.draw();
                play.take_switch()
            }
            Screen::Replay(replay) => {
                replay.update(dt);
                replay.draw();
                replay.take_switch()
            }
            Screen::Watch(watch) => {
                watch.update(dt);
                watch.draw();
                watch.take_switch()
            }
        };
        if let Some(switch) = switch {
            self.open(switch);
        }
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
        if is_key_pressed(KeyCode::Escape) && !typing {
            // Back from a view opened with Q / J; otherwise quit.
            match self.back.take() {
                Some(play) if !matches!(self.screen, Screen::Play(_)) => self.screen = Screen::Play(play),
                _ => return Flow::Exit,
            }
        }
        Flow::Continue
    }
}
