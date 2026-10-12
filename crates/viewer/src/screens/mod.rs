pub mod library;
pub mod play;
pub mod replay;
pub mod tracks;
pub mod watch;

use std::path::{Path, PathBuf};

/// A screen asking the app to open another view.
pub enum Switch {
    /// The run's top genomes racing together.
    Watch(String),
    /// The run's replays.
    Replays(String),
    /// The track screen, showing this run's training set (or the defaults).
    Tracks(Option<String>),
    /// Drive this library track (from the track screen).
    Drive(PathBuf),
}

/// The run a file under `runs/<run>/` belongs to.
pub fn run_of(path: &Path) -> Option<String> {
    let dir = path.parent()?;
    if dir.parent()?.file_name()? != "runs" {
        return None;
    }
    Some(dir.file_name()?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::run_of;

    #[test]
    fn finds_the_run_a_file_belongs_to() {
        assert_eq!(run_of(Path::new("/w/runs/run6/best.genome")).as_deref(), Some("run6"));
        assert_eq!(run_of(Path::new("runs/run6/replays")).as_deref(), Some("run6"));
        assert_eq!(run_of(Path::new("/tmp/some/best.genome")), None);
    }
}
