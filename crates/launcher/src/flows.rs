//! What each menu item does: the prompts, then starting or stopping things.

use std::time::Duration;

use inquire::validator::Validation;
use inquire::{Confirm, CustomType, InquireError, MultiSelect, Select, Text};

use crate::launch::{self, Children};
use crate::runs::{self, Run};
use crate::tracks::{self, SavedTrack};
use crate::paths;

#[derive(Default)]
pub struct App {
    pub children: Children,
    /// One line shown under the header on the next menu.
    pub notice: Option<String>,
}

/// Why a flow ended early: Esc/Ctrl-C (back to the menu) or an error.
pub enum Exit {
    Back,
    Fail(String),
}

impl From<InquireError> for Exit {
    fn from(e: InquireError) -> Self {
        match e {
            InquireError::OperationCanceled | InquireError::OperationInterrupted => Exit::Back,
            other => Exit::Fail(other.to_string()),
        }
    }
}

impl From<String> for Exit {
    fn from(message: String) -> Self {
        Exit::Fail(message)
    }
}

/// A flow's outcome: the notice to show, or why it stopped.
pub type Flow = Result<String, Exit>;

/// Settings a new run starts from (config/training.toml).
struct Defaults {
    generations: u32,
    per_generation: u32,
    fixed: Vec<String>,
}

impl Defaults {
    fn load() -> Self {
        let table = std::fs::read_to_string(paths::root().join("config/training.toml"))
            .ok()
            .and_then(|t| t.parse::<toml::Table>().ok())
            .unwrap_or_default();
        let tracks = table.get("tracks").and_then(|t| t.as_table());
        let int = |v: Option<&toml::Value>, default: u32| v.and_then(|v| v.as_integer()).map_or(default, |n| n as u32);
        Self {
            generations: int(table.get("generations"), 500),
            per_generation: int(tracks.and_then(|t| t.get("per_generation")), 3),
            fixed: tracks
                .and_then(|t| t.get("fixed"))
                .and_then(|f| f.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        }
    }
}

impl App {
    pub fn finish(&mut self, result: Flow) {
        self.notice = match result {
            Ok(notice) if notice.is_empty() => None,
            Ok(notice) => Some(notice),
            Err(Exit::Back) => None,
            Err(Exit::Fail(message)) => Some(format!("✗ {message}")),
        };
    }

    pub fn new_training(&mut self, runs: &[Run], preselect: Option<&str>) -> Flow {
        let defaults = Defaults::load();
        let taken: Vec<String> = runs.iter().map(|r| r.name.clone()).collect();
        let name = Text::new("Run name:")
            .with_default(&runs::next_name(runs))
            .with_validator(move |s: &str| {
                Ok(if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                    Validation::Invalid("letters, digits, _ and - only".into())
                } else if taken.iter().any(|t| t == s) {
                    Validation::Invalid("a run with that name already exists".into())
                } else {
                    Validation::Valid
                })
            })
            .prompt()?;

        // Only real courses: tracks marked `test = false` (the skidpad) aren't.
        let courses: Vec<SavedTrack> = tracks::all().into_iter().filter(|t| t.test).collect();
        let selected: Vec<usize> = courses
            .iter()
            .enumerate()
            .filter(|(_, t)| preselect.map_or_else(|| defaults.fixed.contains(&t.path), |p| p == t.path))
            .map(|(i, _)| i)
            .collect();
        let fixed: Vec<String> = MultiSelect::new("Your maps to train on every generation:", courses)
            .with_default(&selected)
            .with_help_message("space toggle · enter confirm · none = generated tracks only")
            .prompt()?
            .into_iter()
            .map(|t| t.path)
            .collect();

        let need_generated = fixed.is_empty();
        let generated = CustomType::<u32>::new("Generated tracks per generation:")
            .with_default(if need_generated { defaults.per_generation.max(1) } else { defaults.per_generation })
            .with_help_message("fresh procedural tracks each generation, alongside your maps (0 = your maps only)")
            .with_validator(move |n: &u32| {
                Ok(if need_generated && *n == 0 { Validation::Invalid("no maps picked: need at least 1".into()) } else { Validation::Valid })
            })
            .prompt()?;
        let generations = CustomType::<u32>::new("Generations:")
            .with_default(defaults.generations)
            .with_validator(|n: &u32| Ok(if *n == 0 { Validation::Invalid("at least 1".into()) } else { Validation::Valid }))
            .prompt()?;

        let maps: Vec<String> = fixed.iter().map(|f| tracks::stem(f)).collect();
        let summary = match (maps.is_empty(), generated) {
            (true, n) => format!("{n} generated"),
            (false, 0) => maps.join(" + "),
            (false, n) => format!("{} + {n} generated", maps.join(" + ")),
        };
        if !Confirm::new(&format!("Start {name}: {generations} generations on {summary} per generation?")).with_default(true).prompt()? {
            return Err(Exit::Back);
        }

        let mut args = vec!["--name".to_string(), name.clone(), "--generations".into(), generations.to_string()];
        if fixed.is_empty() {
            args.push("--no-fixed".into());
        }
        for path in &fixed {
            args.extend(["--track".into(), path.clone()]);
        }
        args.extend(["--generated".into(), generated.to_string()]);
        self.children.trainer(&name, &args)?;

        if Confirm::new("Watch it now?").with_default(true).prompt().unwrap_or(false) {
            self.open_viewer(&name, fixed.first())?;
        }
        Ok(format!("started {name} (log: runs/{name}/train.log)"))
    }

    pub fn watch(&mut self, run: &Run) -> Flow {
        self.open_viewer(&run.name, run.fixed.first())?;
        let on = run.fixed.first().map_or_else(|| "test_loop".into(), |f| tracks::stem(f));
        Ok(format!("viewer following {} on {on}", run.name))
    }

    /// The run's top genomes racing together (multi-car view).
    pub fn watch_top(&mut self, run: &Run) -> Flow {
        self.children.viewer(&["--watch".into(), run.name.clone()])?;
        Ok(format!("viewer racing {}'s top cars", run.name))
    }

    /// The run's replays, newest first.
    pub fn replays(&mut self, run: &Run) -> Flow {
        self.children.viewer(&["--replays".into(), run.name.clone()])?;
        Ok(format!("viewer playing {}'s replays", run.name))
    }

    fn open_viewer(&mut self, run: &str, track: Option<&String>) -> Result<(), String> {
        let mut args = vec!["--follow".to_string(), run.to_string()];
        if let Some(track) = track {
            args.extend(["--track".into(), track.clone()]);
        }
        self.children.viewer(&args)
    }

    pub fn pick_resume(&mut self, runs: &[Run]) -> Flow {
        let stopped: Vec<&Run> = runs.iter().filter(|r| !r.running()).collect();
        let run = Select::new("Resume which run?", stopped).with_page_size(12).prompt()?;
        self.resume(run)
    }

    fn resume(&mut self, run: &Run) -> Flow {
        let current = run.generation.map_or(0, |g| g + 1);
        let target = run.target.unwrap_or(500);
        let default = if current >= target { current + 500 } else { target };
        let until = CustomType::<u32>::new("Train until generation:")
            .with_default(default)
            .with_validator(move |n: &u32| {
                Ok(if *n <= current { Validation::Invalid(format!("it's already at generation {current}").into()) } else { Validation::Valid })
            })
            .prompt()?;
        let from = run.last_checkpoint.map_or_else(|| "the start (no checkpoint yet)".into(), |c| format!("its checkpoint at gen {c}"));
        if !Confirm::new(&format!("Resume {} from {from}, up to gen {until}?", run.name)).with_default(true).prompt()? {
            return Err(Exit::Back);
        }
        let args = vec!["--name".to_string(), run.name.clone(), "--resume".into(), "--generations".into(), until.to_string()];
        self.children.trainer(&run.name, &args)?;
        Ok(format!("resumed {} (log: runs/{}/train.log)", run.name, run.name))
    }

    pub fn stop(&mut self, run: &Run) -> Flow {
        let Some(pid) = run.pid else { return Ok(format!("{} isn't training", run.name)) };
        let lost = match (run.last_checkpoint, run.generation) {
            (Some(c), Some(g)) if g > c => format!(" Resuming restarts from its checkpoint at gen {c} (now gen {g})."),
            (None, _) => " It has no checkpoint yet, so it can't be resumed.".into(),
            _ => String::new(),
        };
        if !Confirm::new(&format!("Stop {}?{lost}", run.name)).with_default(false).prompt()? {
            return Err(Exit::Back);
        }
        launch::stop(pid)?;
        for _ in 0..50 {
            self.children.reap();
            if !runs::all().iter().any(|r| r.pid == Some(pid)) {
                return Ok(format!("stopped {}", run.name));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(Exit::Fail(format!("{} (pid {pid}) is still running", run.name)))
    }

    pub fn runs_menu(&mut self, runs: &[Run]) -> Flow {
        if runs.is_empty() {
            return Ok("no runs yet".into());
        }
        let run = Select::new("Runs:", runs.iter().collect()).with_page_size(15).prompt()?;
        let has_best = run.dir.join("best.genome").exists();
        let has_log = run.log().exists();
        let has_top = run.dir.join("top.genomes").exists();
        let has_replays = std::fs::read_dir(run.dir.join("replays"))
            .is_ok_and(|mut d| d.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "replay"))));
        let mut actions = vec![];
        if run.running() {
            actions.push("Watch live");
        } else if has_best {
            actions.push("Watch best driver");
        }
        if has_top {
            actions.push("Watch top cars (multi-car)");
        }
        if has_replays {
            actions.push("Replays");
        }
        if run.running() {
            actions.extend(["Show log", "Stop training"]);
        } else {
            actions.push("Resume training…");
            if has_log {
                actions.push("Show log");
            }
        }
        match Select::new(&format!("{}:", run.name), actions).prompt()? {
            "Watch live" | "Watch best driver" => self.watch(run),
            "Watch top cars (multi-car)" => self.watch_top(run),
            "Replays" => self.replays(run),
            "Stop training" => self.stop(run),
            "Resume training…" => self.resume(run),
            _ => show_log(run),
        }
    }

    pub fn drive_menu(&mut self) -> Flow {
        let track = Select::new("Drive on:", tracks::all()).with_page_size(15).prompt()?;
        self.drive(&track)
    }

    fn drive(&mut self, track: &SavedTrack) -> Flow {
        self.children.viewer(&["--track".into(), track.path.clone()])?;
        Ok(format!("viewer open on {}", track.stem))
    }

    pub fn tracks_menu(&mut self, runs: &[Run]) -> Flow {
        let all = tracks::all();
        let labels: Vec<String> = all.iter().map(|t| format!("{:<30} {}", t.to_string(), track_note(t, runs))).collect();
        let picked = Select::new("Tracks:", labels).with_page_size(15).raw_prompt()?;
        let track = &all[picked.index];
        let action = Select::new(&format!("{}:", track.stem), vec!["Drive here", "Start new training on it…"]).prompt()?;
        if action == "Drive here" { self.drive(track) } else { self.new_training(runs, Some(&track.path)) }
    }
}

/// Which runs train on a track, or whether it's a test track.
fn track_note(track: &SavedTrack, runs: &[Run]) -> String {
    let users: Vec<&str> = runs.iter().filter(|r| r.fixed.contains(&track.path)).map(|r| r.name.as_str()).collect();
    match (track.test, users.is_empty()) {
        (false, _) => "not a course".into(),
        (true, true) => "test track".into(),
        (true, false) => format!("trained on by {} (test track for other runs)", users.join(", ")),
    }
}

fn show_log(run: &Run) -> Flow {
    print!("\x1b[2J\x1b[H");
    println!("── runs/{}/train.log (last 30 lines) ──\n", run.name);
    println!("{}", launch::tail(&run.log(), 30));
    Text::new("").with_help_message("enter to go back").prompt()?;
    Ok(String::new())
}
