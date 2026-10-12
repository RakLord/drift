use common::Rng;
use nn::NetworkSpec;

use crate::crossover::neuron_crossover;
use crate::mutation::mutate;
use crate::selection::{ranked, tournament};
use crate::{GaConfig, Genome};

/// Hands out unique genome ids.
#[derive(Clone, Debug, Default)]
pub struct Ids(pub u64);

impl Ids {
    pub fn fresh(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

/// A sub-population sharing one network shape. Breeding stays inside it.
#[derive(Clone, Debug)]
pub struct Island {
    pub index: usize,
    pub spec: NetworkSpec,
    pub genomes: Vec<Genome>,
    /// Fitness of each genome after evaluation (same order as `genomes`).
    pub fitness: Vec<f64>,
}

impl Island {
    pub fn new(index: usize, spec: NetworkSpec, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids) -> Self {
        let genomes = (0..cfg.population).map(|_| random_genome(&spec, cfg, rng, ids, 0)).collect();
        Self { index, spec, genomes, fitness: vec![] }
    }

    /// Index and fitness of the fittest genome (after evaluation).
    pub fn best(&self) -> Option<(usize, f64)> {
        ranked(&self.fitness).first().map(|&i| (i, self.fitness[i]))
    }

    /// Replace the population with the next generation. Needs `fitness`.
    pub fn breed(&mut self, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids, generation: u32) {
        assert_eq!(self.fitness.len(), self.genomes.len(), "evaluate before breeding");
        let n = self.genomes.len();
        let order = ranked(&self.fitness);
        let elites = ((n as f64 * cfg.elite_fraction).ceil() as usize).clamp(1, n);
        let immigrants = cfg.immigrants.min(n - elites);
        let neurons = self.spec.neuron_ranges();

        let mut next: Vec<Genome> = order[..elites].iter().map(|&i| self.genomes[i].clone()).collect();
        while next.len() < n - immigrants {
            let a = &self.genomes[tournament(&self.fitness, cfg.tournament_size, rng)];
            let (mut params, mut sigma, parents) = if rng.chance(cfg.crossover_rate) {
                let b = &self.genomes[tournament(&self.fitness, cfg.tournament_size, rng)];
                let child = neuron_crossover(&a.params, &b.params, &neurons, rng);
                (child, (a.sigma * b.sigma).sqrt(), [Some(a.id), Some(b.id)])
            } else {
                (a.params.clone(), a.sigma, [Some(a.id), None])
            };
            mutate(&mut params, &mut sigma, cfg, rng);
            next.push(Genome { id: ids.fresh(), params, sigma, parents, born: generation });
        }
        while next.len() < n {
            next.push(random_genome(&self.spec, cfg, rng, ids, generation));
        }
        self.genomes = next;
        self.fitness.clear();
    }
}

fn random_genome(spec: &NetworkSpec, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids, generation: u32) -> Genome {
    Genome { id: ids.fresh(), params: spec.random_params(rng), sigma: cfg.sigma_start, parents: [None, None], born: generation }
}

/// Islands compete: if the weakest island's best is well behind the
/// strongest's, it's replaced by a copy of the strongest (network shape
/// included). Call after evaluation, before breeding. Returns (from, to).
pub fn migrate(islands: &mut [Island], cfg: &GaConfig, ids: &mut Ids) -> Option<(usize, usize)> {
    if islands.len() < 2 {
        return None;
    }
    let bests: Vec<f64> = islands.iter().map(|i| i.best().map_or(f64::MIN, |(_, f)| f)).collect();
    let order = ranked(&bests);
    let (strong, weak) = (order[0], order[order.len() - 1]);
    let (best, worst) = (bests[strong], bests[weak]);
    // "Well behind": below `gap` of the best, measured so it also works when
    // fitness is negative.
    if worst >= best - (1.0 - cfg.migration_gap) * best.abs() {
        return None;
    }
    let mut copy = islands[strong].clone();
    copy.index = islands[weak].index;
    for g in &mut copy.genomes {
        g.id = ids.fresh();
    }
    islands[weak] = copy;
    Some((strong, weak))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nn::{Activation, LayerSpec};

    fn cfg() -> GaConfig {
        GaConfig {
            population: 20,
            elite_fraction: 0.1,
            tournament_size: 3,
            crossover_rate: 0.7,
            mutation_rate: 0.1,
            sigma_start: 0.1,
            sigma_min: 0.01,
            sigma_max: 0.5,
            sigma_adapt: 0.2,
            reset_rate: 0.001,
            immigrants: 2,
            migration_every: 10,
            migration_gap: 0.8,
        }
    }

    fn spec(size: usize) -> NetworkSpec {
        NetworkSpec {
            inputs: 4,
            hidden: vec![LayerSpec { size, activation: Activation::Tanh }],
            outputs: vec![Activation::Tanh],
            output_bias: vec![],
            output_init_scale: 1.0,
        }
    }

    #[test]
    fn breeding_keeps_elites_and_size() {
        let (mut rng, mut ids) = (Rng::new(1), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        island.fitness = (0..20).map(f64::from).collect();
        let champion = island.genomes[19].clone();
        island.breed(&cfg(), &mut rng, &mut ids, 1);
        assert_eq!(island.genomes.len(), 20);
        assert_eq!(island.genomes[0], champion, "the best genome survives unchanged");
        let fresh = island.genomes.iter().filter(|g| g.parents == [None, None] && g.born == 1).count();
        assert_eq!(fresh, 2, "immigrants");
    }

    #[test]
    fn evolution_climbs_a_simple_objective() {
        // Fitness: closeness of the params to all-0.5.
        let (mut rng, mut ids) = (Rng::new(2), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        let score = |g: &Genome| -g.params.iter().map(|p| (p - 0.5).powi(2)).sum::<f32>() as f64;
        let mut first = 0.0;
        for generation in 0..60 {
            island.fitness = island.genomes.iter().map(score).collect();
            if generation == 0 {
                first = island.best().unwrap().1;
            }
            island.breed(&cfg(), &mut rng, &mut ids, generation + 1);
        }
        island.fitness = island.genomes.iter().map(score).collect();
        let last = island.best().unwrap().1;
        assert!(last > first * 0.2, "{first} -> {last}");
    }

    #[test]
    fn weak_island_takes_the_strong_island_shape() {
        let (mut rng, mut ids) = (Rng::new(3), Ids::default());
        let mut islands = vec![
            Island::new(0, spec(3), &cfg(), &mut rng, &mut ids),
            Island::new(1, spec(5), &cfg(), &mut rng, &mut ids),
        ];
        islands[0].fitness = vec![100.0; 20];
        islands[1].fitness = vec![10.0; 20];
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids), Some((0, 1)));
        assert_eq!(islands[1].spec, spec(3));
        assert_eq!(islands[1].index, 1);

        // Close enough: no migration.
        islands[1].fitness = vec![95.0; 20];
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids), None);
    }
}
