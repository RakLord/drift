//! `tracks.csv`: which tracks the saved driver drove each generation, and
//! how it did on each. One row per run: its training tracks, any `[best]`
//! check tracks, and the test set on test generations.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use sim::episode::EpisodeResult;

const HEADER: &str = "generation,track,role,length,progress,laps,time,end";

/// A generated track's id in `tracks.csv` (a saved track's is its path).
pub fn generated_id(seed: u64) -> String {
    format!("gen:{seed}")
}

pub struct TracksCsv {
    file: File,
}

impl TracksCsv {
    /// Open for appending from `generation` on. Rows from that generation or
    /// later (left by a run that went past its last checkpoint) are dropped,
    /// so a resumed run doesn't log a generation twice.
    pub fn open(path: &Path, generation: u32) -> io::Result<Self> {
        let old = std::fs::read_to_string(path).unwrap_or_default();
        let kept: Vec<&str> = old
            .lines()
            .skip(1)
            .filter(|line| line.split(',').next().and_then(|g| g.parse::<u32>().ok()).is_some_and(|g| g < generation))
            .collect();
        let mut text = format!("{HEADER}\n");
        for line in kept {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(path, text)?;
        Ok(Self { file: OpenOptions::new().append(true).open(path)? })
    }

    /// One row per run: `(track id, role, result)`.
    pub fn write<'a>(&mut self, generation: u32, runs: impl IntoIterator<Item = (&'a str, &'a str, &'a EpisodeResult)>) -> io::Result<()> {
        let mut rows = String::new();
        for (track, role, r) in runs {
            rows.push_str(&format!(
                "{generation},{},{role},{:.0},{:.1},{:.3},{:.2},{}\n",
                track.replace(',', "_"),
                r.track_length,
                r.progress,
                r.laps(),
                r.time,
                r.end.name()
            ));
        }
        self.file.write_all(rows.as_bytes())?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resuming_drops_rows_from_the_resumed_generation_on() {
        let path = std::env::temp_dir().join(format!("drift-tracks-csv-{}.csv", std::process::id()));
        let rows = format!("{HEADER}\n0,tracks/a.toml,fixed,1000,5.0,0.005,1.00,crashed\n1,gen:7,generated,900,9.0,0.010,2.00,time_up\n2,gen:8,generated,900,9.0,0.010,2.00,time_up\n");
        std::fs::write(&path, rows).unwrap();
        TracksCsv::open(&path, 1).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(text, format!("{HEADER}\n0,tracks/a.toml,fixed,1000,5.0,0.005,1.00,crashed\n"));
    }
}
