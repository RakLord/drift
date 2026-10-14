//! Saving and restoring a run: the best genome every generation, and the
//! whole population every few generations so a run can be resumed.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use evo::{Genome, Ids, Island};
use nn::NetworkSpec;
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"DRIFTPOP";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Header {
    /// The generation this population is about to be evaluated as.
    generation: u32,
    stage: usize,
    next_id: u64,
    layout: String,
    islands: Vec<IslandHeader>,
    /// Human picks still under protection (none in older files).
    #[serde(default)]
    protected: Vec<Protected>,
}

/// A human pick kept unchanged in its island until `until` (exclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Protected {
    pub id: u64,
    pub island: usize,
    pub until: u32,
}

#[derive(Serialize, Deserialize)]
struct IslandHeader {
    index: usize,
    spec: NetworkSpec,
    genomes: Vec<GenomeHeader>,
}

#[derive(Serialize, Deserialize)]
struct GenomeHeader {
    id: u64,
    sigma: f64,
    parents: Vec<u64>,
    born: u32,
}

pub struct Population {
    pub generation: u32,
    pub stage: usize,
    pub ids: Ids,
    pub islands: Vec<Island>,
    pub protected: Vec<Protected>,
}

pub fn path(run_dir: &Path, generation: u32) -> PathBuf {
    run_dir.join("checkpoints").join(format!("gen_{generation:05}.pop"))
}

/// The most recent population checkpoint in a run, if any.
pub fn latest(run_dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(run_dir.join("checkpoints"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pop"))
        .collect();
    files.sort();
    files.pop()
}

pub fn save(path: &Path, pop: &Population) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let header = Header {
        generation: pop.generation,
        stage: pop.stage,
        next_id: pop.ids.0,
        layout: format!("{:016x}", sim::sensors::observation::layout_hash()),
        islands: pop
            .islands
            .iter()
            .map(|i| IslandHeader {
                index: i.index,
                spec: i.spec.clone(),
                genomes: i
                    .genomes
                    .iter()
                    .map(|g| GenomeHeader { id: g.id, sigma: g.sigma, parents: g.parents.iter().flatten().copied().collect(), born: g.born })
                    .collect(),
            })
            .collect(),
        protected: pop.protected.clone(),
    };
    let text = toml::to_string(&header).map_err(io::Error::other)?;
    let mut w = io::BufWriter::new(std::fs::File::create(path)?);
    w.write_all(MAGIC)?;
    w.write_all(&VERSION.to_le_bytes())?;
    w.write_all(&(text.len() as u64).to_le_bytes())?;
    w.write_all(text.as_bytes())?;
    for g in pop.islands.iter().flat_map(|i| &i.genomes) {
        for p in &g.params {
            w.write_all(&p.to_le_bytes())?;
        }
    }
    w.flush()
}

pub fn load(path: &Path) -> io::Result<Population> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {m}", path.display()));
    let mut r = io::BufReader::new(std::fs::File::open(path)?);
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(bad("not a population checkpoint"));
    }
    let mut word = [0u8; 4];
    r.read_exact(&mut word)?;
    if u32::from_le_bytes(word) != VERSION {
        return Err(bad("unsupported checkpoint version"));
    }
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let mut text = vec![0u8; u64::from_le_bytes(len) as usize];
    r.read_exact(&mut text)?;
    let header: Header = toml::from_str(&String::from_utf8(text).map_err(|_| bad("header isn't text"))?)
        .map_err(|e| bad(&e.to_string()))?;
    if header.layout != format!("{:016x}", sim::sensors::observation::layout_hash()) {
        return Err(bad("saved with a different input layout"));
    }

    let mut islands = vec![];
    for ih in header.islands {
        let count = ih.spec.param_count();
        let mut genomes = vec![];
        for gh in ih.genomes {
            let mut bytes = vec![0u8; count * 4];
            r.read_exact(&mut bytes)?;
            let params = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            let parents = [gh.parents.first().copied(), gh.parents.get(1).copied()];
            genomes.push(Genome { id: gh.id, params, sigma: gh.sigma, parents, born: gh.born });
        }
        islands.push(Island { index: ih.index, spec: ih.spec, genomes, fitness: vec![], objectives: vec![], recent_best: Default::default() });
    }
    Ok(Population {
        generation: header.generation,
        stage: header.stage,
        ids: Ids(header.next_id),
        islands,
        protected: header.protected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Rng;
    use evo::GaConfig;
    use nn::{Activation, LayerSpec};

    #[test]
    fn population_round_trips() {
        let cfg: GaConfig = crate::config::TrainingConfig::default().ga;
        let cfg = GaConfig { population: 5, ..cfg };
        let (mut rng, mut ids) = (Rng::new(1), Ids::default());
        let spec = agent::driver_spec(vec![LayerSpec { size: 6, activation: Activation::Tanh }]);
        let islands = vec![Island::new(0, spec.clone(), &cfg, &mut rng, &mut ids), Island::new(1, spec, &cfg, &mut rng, &mut ids)];
        let protected = vec![Protected { id: 3, island: 1, until: 57 }];
        let pop = Population { generation: 7, stage: 1, ids, islands, protected };
        let dir = std::env::temp_dir().join(format!("drift-pop-{}", std::process::id()));
        let file = path(&dir, 7);
        save(&file, &pop).unwrap();
        let back = load(&file).unwrap();
        assert_eq!(back.generation, 7);
        assert_eq!(back.ids.0, pop.ids.0);
        assert_eq!(back.protected, pop.protected);
        for (a, b) in back.islands.iter().zip(&pop.islands) {
            assert_eq!(a.genomes, b.genomes);
            assert_eq!(a.spec, b.spec);
        }
        assert_eq!(latest(&dir), Some(file));
        std::fs::remove_dir_all(&dir).ok();
    }
}
