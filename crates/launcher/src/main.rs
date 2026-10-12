//! `cargo drift`: a menu for everything around training. Start, watch,
//! resume and stop runs, and drive saved tracks, without remembering
//! commands. It only reads `runs/` and `tracks/` and starts the `train` and
//! `viewer` binaries; trainers run in the background with output in
//! `runs/<name>/train.log`.

mod flows;
mod launch;
mod paths;
mod runs;
mod tracks;

use std::fmt;

use inquire::{InquireError, Select};

use flows::App;
use runs::Run;

enum Item {
    Watch(String, bool),
    Stop(String),
    New,
    Resume,
    Runs,
    Drive,
    Tracks,
    Quit,
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Item::Watch(run, true) => write!(f, "Watch {run} live"),
            Item::Watch(run, false) => write!(f, "Watch {run}'s best driver"),
            Item::Stop(run) => write!(f, "Stop {run}"),
            Item::New => write!(f, "Start new training…"),
            Item::Resume => write!(f, "Resume a run…"),
            Item::Runs => write!(f, "Runs…"),
            Item::Drive => write!(f, "Drive…"),
            Item::Tracks => write!(f, "Tracks…"),
            Item::Quit => write!(f, "Quit"),
        }
    }
}

fn main() {
    let mut app = App::default();
    loop {
        app.children.reap();
        let runs = runs::all();
        print!("\x1b[2J\x1b[H");
        header(&runs);
        if let Some(notice) = app.notice.take() {
            println!("  {notice}\n");
        }
        let menu = Select::new("DRIFT", items(&runs))
            .with_page_size(12)
            .with_help_message("↑↓ move · enter select · esc refresh · ctrl-c quit")
            .prompt();
        match menu {
            Ok(Item::Quit) | Err(InquireError::OperationInterrupted) => break,
            Ok(item) => app.handle(item, &runs),
            Err(InquireError::OperationCanceled) => {}
            Err(e) => {
                eprintln!("{e}");
                break;
            }
        }
    }
    if runs::all().iter().any(Run::running) {
        println!("Training keeps going in the background. Run `cargo drift` again to check on it.");
    }
}

/// Running runs, or the latest one when nothing is training.
fn header(runs: &[Run]) {
    let line = "─".repeat(78);
    println!(" DRIFT {line}");
    let running: Vec<&Run> = runs.iter().filter(|r| r.running()).collect();
    match (running.is_empty(), runs.first()) {
        (false, _) => running.iter().for_each(|r| println!("  {r}")),
        (true, Some(latest)) => println!("  {latest}"),
        (true, None) => println!("  no runs yet"),
    }
    println!(" {line}──────\n");
}

fn items(runs: &[Run]) -> Vec<Item> {
    let mut items: Vec<Item> = runs.iter().filter(|r| r.running()).map(|r| Item::Watch(r.name.clone(), true)).collect();
    if items.is_empty()
        && let Some(latest) = runs.first().filter(|r| r.dir.join("best.genome").exists())
    {
        items.push(Item::Watch(latest.name.clone(), false));
    }
    items.push(Item::New);
    if runs.iter().any(|r| !r.running()) {
        items.push(Item::Resume);
    }
    items.extend(runs.iter().filter(|r| r.running()).map(|r| Item::Stop(r.name.clone())));
    items.extend([Item::Runs, Item::Drive, Item::Tracks, Item::Quit]);
    items
}

impl App {
    fn handle(&mut self, item: Item, runs: &[Run]) {
        let find = |name: &str| runs.iter().find(|r| r.name == name);
        let result = match item {
            Item::Watch(name, _) => find(&name).map_or(Ok(String::new()), |run| self.watch(run)),
            Item::Stop(name) => find(&name).map_or(Ok(String::new()), |run| self.stop(run)),
            Item::New => self.new_training(runs, None),
            Item::Resume => self.pick_resume(runs),
            Item::Runs => self.runs_menu(runs),
            Item::Drive => self.drive_menu(),
            Item::Tracks => self.tracks_menu(runs),
            Item::Quit => Ok(String::new()),
        };
        self.finish(result);
    }
}
