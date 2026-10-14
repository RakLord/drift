//! `top.genomes`: the genomes the live multi-car viewer races, written by the
//! trainer every generation.
//!
//! `DRIFTTOP` magic, a format version, a TOML header (generation, stage, the
//! stage's actual episode / scoring / car rules, and why each genome is
//! there), then each genome in the genome file format. Saved via a temporary
//! file and a rename, so a reader never sees half a file.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sim::car::CarParams;
use sim::episode::EpisodeConfig;
use sim::scoring::ScoringParams;

use crate::genome_file::{self, GenomeFileError, SavedGenome};

const MAGIC: &[u8; 8] = b"DRIFTTOP";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopEntry {
    /// The genome's id in the population (stable across generations).
    pub id: u64,
    pub island: u32,
    /// Its fitness rank this generation across all islands (1 = fittest).
    pub rank: u32,
    pub fitness: f64,
    pub drift_quality: f64,
    /// Picked for drift quality rather than fitness.
    pub drifter: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopHeader {
    pub run: String,
    pub generation: u32,
    pub stage: String,
    /// The rules these genomes were scored under this generation.
    pub episode: EpisodeConfig,
    pub scoring: ScoringParams,
    pub car: CarParams,
    pub entries: Vec<TopEntry>,
}

#[derive(Clone, Debug)]
pub struct TopGenomes {
    pub header: TopHeader,
    /// One per entry, in the same order.
    pub genomes: Vec<SavedGenome>,
}

#[derive(Debug)]
pub enum TopFileError {
    Io(io::Error),
    NotATopFile,
    Version(u32),
    Header(String),
    Genome(GenomeFileError),
    Count { header: usize, genomes: usize },
}

impl fmt::Display for TopFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::NotATopFile => write!(f, "not a top.genomes file"),
            Self::Version(v) => write!(f, "top.genomes version {v}, expected {VERSION}"),
            Self::Header(e) => write!(f, "bad top.genomes header: {e}"),
            Self::Genome(e) => write!(f, "{e}"),
            Self::Count { header, genomes } => write!(f, "top.genomes lists {header} genomes but holds {genomes}"),
        }
    }
}

impl std::error::Error for TopFileError {}

impl From<io::Error> for TopFileError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub fn write(w: &mut impl Write, top: &TopGenomes) -> io::Result<()> {
    let header = toml::to_string(&top.header).map_err(io::Error::other)?;
    w.write_all(MAGIC)?;
    w.write_all(&VERSION.to_le_bytes())?;
    w.write_all(&(header.len() as u32).to_le_bytes())?;
    w.write_all(header.as_bytes())?;
    w.write_all(&(top.genomes.len() as u32).to_le_bytes())?;
    for genome in &top.genomes {
        genome_file::write(w, genome)?;
    }
    Ok(())
}

pub fn read(r: &mut impl Read) -> Result<TopGenomes, TopFileError> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(TopFileError::NotATopFile);
    }
    let version = read_u32(r)?;
    if version != VERSION {
        return Err(TopFileError::Version(version));
    }
    let mut header = vec![0u8; read_u32(r)? as usize];
    r.read_exact(&mut header)?;
    let header = String::from_utf8(header).map_err(|e| TopFileError::Header(e.to_string()))?;
    let header: TopHeader = toml::from_str(&header).map_err(|e| TopFileError::Header(e.to_string()))?;
    let count = read_u32(r)? as usize;
    if count != header.entries.len() {
        return Err(TopFileError::Count { header: header.entries.len(), genomes: count });
    }
    let genomes = (0..count).map(|_| genome_file::read(r).map_err(TopFileError::Genome)).collect::<Result<_, _>>()?;
    Ok(TopGenomes { header, genomes })
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

/// Write to a temporary file next to `path`, then rename over it.
pub fn save(path: impl AsRef<Path>, top: &TopGenomes) -> io::Result<()> {
    let path = path.as_ref();
    let temp = path.with_extension("genomes.tmp");
    {
        let mut file = io::BufWriter::new(std::fs::File::create(&temp)?);
        write(&mut file, top)?;
        file.flush()?;
    }
    std::fs::rename(&temp, path)
}

pub fn load(path: impl AsRef<Path>) -> Result<TopGenomes, TopFileError> {
    read(&mut io::BufReader::new(std::fs::File::open(path)?))
}

#[cfg(test)]
mod tests {
    use common::Rng;
    use nn::{Activation, LayerSpec};

    use super::*;
    use crate::driver_spec;
    use crate::genome_file::GenomeHeader;

    fn top() -> TopGenomes {
        let spec = driver_spec(vec![LayerSpec { size: 6, activation: Activation::Tanh }]);
        let genomes: Vec<SavedGenome> = (0..3)
            .map(|i| SavedGenome {
                header: GenomeHeader::new(spec.clone(), 9, i, f64::from(i), "t"),
                params: spec.random_params(&mut Rng::new(u64::from(i))),
            })
            .collect();
        let entries = (0..3)
            .map(|i| TopEntry { id: 100 + u64::from(i), island: i, rank: i + 1, fitness: 1.0, drift_quality: 0.2, drifter: i == 2 })
            .collect();
        let episode = EpisodeConfig {
            max_secs: 60.0,
            start_speed_kmh: 50.0,
            decision_every: 4,
            sensor_noise: 0.02,
            no_progress_secs: 4.0,
            max_backwards: 20.0,
            max_hard_hits: 3,
            end_on_spin: false,
            clutch_grace_secs: 0.4,
            smoothness: Default::default(),
        };
        let header = TopHeader {
            run: "r".into(),
            generation: 9,
            stage: "drive".into(),
            episode,
            scoring: ScoringParams::default(),
            car: CarParams::default(),
            entries,
        };
        TopGenomes { header, genomes }
    }

    #[test]
    fn round_trips_through_an_atomic_save() {
        let dir = std::env::temp_dir().join(format!("drift-top-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("top.genomes");
        let t = top();
        save(&path, &t).unwrap();
        assert!(!path.with_extension("genomes.tmp").exists(), "temporary file renamed away");
        let back = load(&path).unwrap();
        assert_eq!(back.header.entries, t.header.entries);
        assert_eq!(back.genomes, t.genomes);
        assert!(!back.header.episode.end_on_spin, "carries the stage's rules");
        std::fs::remove_dir_all(&dir).ok();
    }
}
