//! Human picks from the viewer's review grid (`agent::picks`), read at the
//! end of each generation: each is placed in its own island, or another
//! with the same network shape, then kept and bred from (`[review]`).

use std::path::Path;

use agent::picks;
use agent::top_file::TopGenomes;
use evo::{Genome, Island};

pub struct Pick {
    /// Index into the population's islands.
    pub island: usize,
    pub genome: Genome,
}

/// Every pick waiting in the run's inbox. Each file is moved out of the
/// inbox before its picks are used, so a file is never applied twice.
pub fn take(run_dir: &Path, islands: &[Island], generation: u32) -> Vec<Pick> {
    let mut out = vec![];
    for (file, read) in picks::waiting(run_dir) {
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let top = match read {
            Ok(top) => top,
            Err(e) => {
                println!("== picks {name}: unreadable ({e}), moved to picks/rejected");
                if let Err(e) = picks::mark_rejected(run_dir, &file) {
                    println!("== picks {name}: can't move it ({e})");
                }
                continue;
            }
        };
        if let Err(e) = picks::mark_applied(run_dir, &file, generation) {
            println!("== picks {name}: can't move it to picks/applied ({e}), skipped");
            continue;
        }
        out.extend(place(&top, islands));
    }
    out
}

/// The picks in `top`, each in its own island if the shape still matches
/// (a migration may have replaced it), otherwise the first island that
/// matches. Picks with no matching island are dropped.
fn place(top: &TopGenomes, islands: &[Island]) -> Vec<Pick> {
    let mut out = vec![];
    for (entry, saved) in top.header.entries.iter().zip(&top.genomes) {
        let fits = |i: &usize| islands[*i].spec == saved.header.spec && saved.params.len() == islands[*i].spec.param_count();
        let own = entry.island as usize;
        let Some(island) = Some(own).filter(|i| *i < islands.len()).filter(fits).or_else(|| (0..islands.len()).find(fits)) else {
            println!("== pick #{}: no island has its network shape ({}), skipped", entry.id, saved.header.spec.describe());
            continue;
        };
        let parents = [entry.parents.first().copied(), entry.parents.get(1).copied()];
        let genome = Genome { id: entry.id, params: saved.params.clone(), sigma: entry.sigma, parents, born: entry.born };
        out.push(Pick { island, genome });
    }
    out
}

#[cfg(test)]
mod tests {
    use agent::genome_file::{GenomeHeader, SavedGenome};
    use agent::top_file::{TopEntry, TopHeader};
    use common::Rng;
    use evo::{GaConfig, Ids};
    use nn::{Activation, LayerSpec, NetworkSpec};

    use super::*;
    use crate::config::TrainingConfig;

    fn spec(size: usize) -> NetworkSpec {
        agent::driver_spec(vec![LayerSpec { size, activation: Activation::Tanh }])
    }

    fn picks(of: &[(u64, u32, usize)]) -> TopGenomes {
        let cfg = TrainingConfig::default();
        let mut entries = vec![];
        let mut genomes = vec![];
        for &(id, island, size) in of {
            let spec = spec(size);
            entries.push(TopEntry {
                id,
                island,
                rank: 1,
                fitness: 1.0,
                drift_quality: 0.0,
                drifter: false,
                born: 3,
                parents: vec![1],
                sigma: 0.05,
                objectives: vec![],
                training: None,
            });
            genomes.push(SavedGenome { header: GenomeHeader::new(spec.clone(), 9, island, 1.0, "t"), params: spec.random_params(&mut Rng::new(id)) });
        }
        let header = TopHeader {
            run: "r".into(),
            generation: 9,
            stage: "drive".into(),
            episode: cfg.episode,
            scoring: Default::default(),
            car: Default::default(),
            entries,
        };
        TopGenomes { header, genomes }
    }

    #[test]
    fn picks_go_to_their_island_or_one_with_the_same_shape() {
        let cfg = GaConfig { population: 4, ..TrainingConfig::default().ga };
        let (mut rng, mut ids) = (Rng::new(1), Ids::default());
        let islands: Vec<Island> = [6, 8, 6].iter().enumerate().map(|(i, &s)| Island::new(i, spec(s), &cfg, &mut rng, &mut ids)).collect();
        // A pick stays in its own island when the shape matches; a 6-wide
        // pick from island 1 (8-wide now, say after a migration) goes to
        // island 0; an 8-wide one from island 0 goes to island 1; a 9-wide
        // one fits nowhere.
        let placed = place(&picks(&[(50, 2, 6), (51, 1, 6), (52, 0, 8), (53, 0, 9)]), &islands);
        let at: Vec<(u64, usize)> = placed.iter().map(|p| (p.genome.id, p.island)).collect();
        assert_eq!(at, [(50, 2), (51, 0), (52, 1)]);
        let g = &placed[0].genome;
        assert_eq!((g.sigma, g.born, g.parents), (0.05, 3, [Some(1), None]));
    }
}
