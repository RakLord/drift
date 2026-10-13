//! Making a video of a run: one of its replays, or its best driver on a
//! saved track. The viewer plays it through in its window and writes
//! `runs/<run>/videos/<replay>.mp4`.

use std::path::{Path, PathBuf};

use inquire::Select;

use crate::flows::Flow;
use crate::launch;
use crate::paths;
use crate::runs::Run;
use crate::tracks;

const A_REPLAY: &str = "One of its replays…";
const BEST_ON_TRACK: &str = "Its best driver on a saved track…";

pub fn export(run: &Run) -> Flow {
    let replays = replays(&run.dir.join("replays"));
    let best = run.dir.join("best.genome");
    let mut sources = vec![];
    if !replays.is_empty() {
        sources.push(A_REPLAY);
    }
    if best.exists() {
        sources.push(BEST_ON_TRACK);
    }
    if sources.is_empty() {
        return Ok(format!("{} has no replays or best driver yet", run.name));
    }
    let mut args: Vec<String> = match Select::new("Video of:", sources).prompt()? {
        A_REPLAY => {
            let names: Vec<String> = replays.iter().filter_map(|p| Some(p.file_name()?.to_string_lossy().into_owned())).collect();
            let name = Select::new("Replay (newest first):", names).with_page_size(15).prompt()?;
            vec!["--replay".into(), run.dir.join("replays").join(name).to_string_lossy().into_owned()]
        }
        _ => {
            let track = Select::new("Track:", tracks::all()).with_page_size(15).prompt()?;
            vec!["--genome".into(), best.to_string_lossy().into_owned(), "--track".into(), track.path]
        }
    };
    // A folder: the viewer names the file after the replay.
    args.extend(["--video".into(), format!("{}/", run.dir.join("videos").display())]);
    println!("  recording (the viewer plays the run through in its window; Esc cancels)…");
    launch::viewer_and_wait(&args)?;
    // The viewer's last line: "saved <path> (<size> MB)".
    Ok(launch::tail(&paths::target().join("viewer.log"), 1))
}

/// A folder's replays, newest first.
fn replays(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut found: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "replay"))
        .map(|e| (e.metadata().and_then(|m| m.modified()).ok(), e.path()))
        .collect();
    found.sort_by(|a, b| b.cmp(a));
    found.into_iter().map(|(_, path)| path).collect()
}
