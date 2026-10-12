//! Genome files: a small versioned binary format.
//!
//! `DRIFTGEN` magic, a format version, a TOML header (network shape, input
//! layout hash, provenance), then the parameters as little-endian f32. A
//! file saved against a different input layout or shape refuses to load.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::Path;

use nn::NetworkSpec;
use serde::{Deserialize, Serialize};
use sim::sensors::observation::layout_hash;

const MAGIC: &[u8; 8] = b"DRIFTGEN";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GenomeHeader {
    pub spec: NetworkSpec,
    /// Input layout this genome was trained on (hex of `layout_hash`).
    pub layout: String,
    pub generation: u32,
    pub island: u32,
    pub fitness: f64,
    pub note: String,
}

impl GenomeHeader {
    pub fn new(spec: NetworkSpec, generation: u32, island: u32, fitness: f64, note: impl Into<String>) -> Self {
        Self { spec, layout: format!("{:016x}", layout_hash()), generation, island, fitness, note: note.into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedGenome {
    pub header: GenomeHeader,
    pub params: Vec<f32>,
}

#[derive(Debug)]
pub enum GenomeFileError {
    Io(io::Error),
    NotAGenome,
    Version(u32),
    Header(String),
    LayoutMismatch { file: String, expected: String },
    ParamCount { file: usize, expected: usize },
}

impl fmt::Display for GenomeFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::NotAGenome => write!(f, "not a genome file"),
            Self::Version(v) => write!(f, "genome file version {v}, expected {VERSION}"),
            Self::Header(e) => write!(f, "bad genome header: {e}"),
            Self::LayoutMismatch { file, expected } => write!(
                f,
                "genome was trained on a different input layout ({file}, current is {expected}); retrain it"
            ),
            Self::ParamCount { file, expected } => write!(f, "genome has {file} parameters, its shape needs {expected}"),
        }
    }
}

impl std::error::Error for GenomeFileError {}

impl From<io::Error> for GenomeFileError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub fn write(w: &mut impl Write, genome: &SavedGenome) -> io::Result<()> {
    let header = toml::to_string(&genome.header).map_err(io::Error::other)?;
    w.write_all(MAGIC)?;
    w.write_all(&VERSION.to_le_bytes())?;
    w.write_all(&(header.len() as u32).to_le_bytes())?;
    w.write_all(header.as_bytes())?;
    w.write_all(&(genome.params.len() as u32).to_le_bytes())?;
    for p in &genome.params {
        w.write_all(&p.to_le_bytes())?;
    }
    Ok(())
}

pub fn read(r: &mut impl Read) -> Result<SavedGenome, GenomeFileError> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(GenomeFileError::NotAGenome);
    }
    let version = read_u32(r)?;
    if version != VERSION {
        return Err(GenomeFileError::Version(version));
    }
    let mut header = vec![0u8; read_u32(r)? as usize];
    r.read_exact(&mut header)?;
    let header = String::from_utf8(header).map_err(|e| GenomeFileError::Header(e.to_string()))?;
    let header: GenomeHeader = toml::from_str(&header).map_err(|e| GenomeFileError::Header(e.to_string()))?;

    let expected_layout = format!("{:016x}", layout_hash());
    if header.layout != expected_layout {
        return Err(GenomeFileError::LayoutMismatch { file: header.layout, expected: expected_layout });
    }
    let count = read_u32(r)? as usize;
    let expected = header.spec.param_count();
    if count != expected {
        return Err(GenomeFileError::ParamCount { file: count, expected });
    }
    let mut bytes = vec![0u8; count * 4];
    r.read_exact(&mut bytes)?;
    let params = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Ok(SavedGenome { header, params })
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub fn save(path: impl AsRef<Path>, genome: &SavedGenome) -> io::Result<()> {
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    write(&mut file, genome)?;
    file.flush()
}

pub fn load(path: impl AsRef<Path>) -> Result<SavedGenome, GenomeFileError> {
    read(&mut io::BufReader::new(std::fs::File::open(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver_spec;
    use common::Rng;
    use nn::{Activation, LayerSpec};

    fn genome() -> SavedGenome {
        let spec = driver_spec(vec![LayerSpec { size: 8, activation: Activation::Tanh }]);
        let params = spec.random_params(&mut Rng::new(5));
        SavedGenome { header: GenomeHeader::new(spec, 12, 1, 345.5, "test"), params }
    }

    #[test]
    fn round_trips() {
        let g = genome();
        let mut bytes = vec![];
        write(&mut bytes, &g).unwrap();
        assert_eq!(read(&mut bytes.as_slice()).unwrap(), g);
    }

    #[test]
    fn refuses_other_layouts_and_wrong_sizes() {
        let mut g = genome();
        g.header.layout = "0000000000000000".into();
        let mut bytes = vec![];
        write(&mut bytes, &g).unwrap();
        assert!(matches!(read(&mut bytes.as_slice()), Err(GenomeFileError::LayoutMismatch { .. })));

        let mut g = genome();
        g.params.pop();
        let mut bytes = vec![];
        write(&mut bytes, &g).unwrap();
        assert!(matches!(read(&mut bytes.as_slice()), Err(GenomeFileError::ParamCount { .. })));

        assert!(matches!(read(&mut b"NOTGENOMExxxxxxxx".as_slice()), Err(GenomeFileError::NotAGenome)));
    }
}
