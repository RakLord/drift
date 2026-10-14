use std::collections::VecDeque;

use common::Rng;
use nn::NetworkSpec;

use crate::crossover::neuron_crossover;
use crate::mutation::mutate;
use crate::config::Selection;
use crate::selection::{Objectives, pareto_order, positions, ranked, tournament};
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
    /// Pareto objectives of each genome after evaluation (driving, drift
    /// quality, judged score); used when selection is Pareto.
    pub objectives: Vec<Objectives>,
    /// Best fitness of recent generations, oldest first (see
    /// `record_best`). Not checkpointed: starts empty on resume.
    pub recent_best: VecDeque<f64>,
}

impl Island {
    pub fn new(index: usize, spec: NetworkSpec, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids) -> Self {
        let genomes = (0..cfg.population).map(|_| random_genome(&spec, cfg, rng, ids, 0)).collect();
        Self { index, spec, genomes, fitness: vec![], objectives: vec![], recent_best: VecDeque::new() }
    }

    /// Index and fitness of the fittest genome (after evaluation).
    pub fn best(&self) -> Option<(usize, f64)> {
        ranked(&self.fitness).first().map(|&i| (i, self.fitness[i]))
    }

    /// After evaluation: remember this generation's best fitness, keeping
    /// the last `window` generations.
    pub fn record_best(&mut self, window: u32) {
        if let Some((_, f)) = self.best() {
            self.recent_best.push_back(f);
        }
        while self.recent_best.len() > window.max(1) as usize {
            self.recent_best.pop_front();
        }
    }

    /// Mean of the recorded bests (what migration compares islands on).
    pub fn rolling_best(&self) -> f64 {
        if self.recent_best.is_empty() {
            return f64::MIN;
        }
        self.recent_best.iter().sum::<f64>() / self.recent_best.len() as f64
    }

    /// Genome indices best first, the way breeding ranks them: by Pareto
    /// order when selection is Pareto and objectives are in, else fitness.
    fn order(&self, cfg: &GaConfig) -> Vec<usize> {
        if cfg.selection == Selection::Pareto && self.objectives.len() == self.genomes.len() {
            pareto_order(&self.objectives)
        } else {
            ranked(&self.fitness)
        }
    }

    /// Replace the population with the next generation. Needs `fitness`.
    /// `keep` (human picks under protection) is carried over unchanged
    /// after the elites, whether or not it's still in the island.
    pub fn breed(&mut self, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids, generation: u32, keep: &[Genome]) {
        assert_eq!(self.fitness.len(), self.genomes.len(), "evaluate before breeding");
        let n = self.genomes.len();
        let order = self.order(cfg);
        let position = positions(&order);
        let elites = ((n as f64 * cfg.elite_fraction).ceil() as usize).clamp(1, n);
        let immigrants = cfg.immigrants.min(n - elites);
        let neurons = self.spec.neuron_ranges();

        let mut next: Vec<Genome> = order[..elites].iter().map(|&i| self.genomes[i].clone()).collect();
        for g in keep {
            if !next.iter().any(|e| e.id == g.id) {
                next.push(g.clone());
            }
        }
        while next.len() < n - immigrants {
            let a = &self.genomes[tournament(&position, cfg.tournament_size, rng)];
            let (mut params, mut sigma, parents) = if rng.chance(cfg.crossover_rate) {
                let b = &self.genomes[tournament(&position, cfg.tournament_size, rng)];
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
        self.objectives.clear();
    }

    /// After a curriculum stage change (call after breeding): every sigma
    /// goes back to `sigma_start`, and the last `reseed_fraction` of the
    /// island (children and immigrants, never the elites at the front) is
    /// replaced by heavily mutated copies of `best`.
    pub fn restage(&mut self, best: &[f32], cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids, generation: u32) {
        for g in &mut self.genomes {
            g.sigma = cfg.sigma_start;
        }
        let n = self.genomes.len();
        let count = ((n as f64 * cfg.reseed_fraction).round() as usize).min(n.saturating_sub(1));
        for g in &mut self.genomes[n - count..] {
            let params = best.iter().map(|&p| p + (rng.gaussian() * cfg.reseed_sigma) as f32).collect();
            *g = Genome { id: ids.fresh(), params, sigma: cfg.sigma_start, parents: [None, None], born: generation };
        }
    }

    /// After breeding: up to `fraction` of the island becomes mutated
    /// children of `picks` (human picks; taken in turn when there are
    /// several), each with its parent's sigma and lineage. Only genomes born
    /// this generation are replaced, from the tail, so elites and kept
    /// genomes stay. Returns how many children it made.
    pub fn flood(&mut self, picks: &[Genome], fraction: f64, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids, generation: u32) -> usize {
        if picks.is_empty() {
            return 0;
        }
        let count = (self.genomes.len() as f64 * fraction).round() as usize;
        let mut made = 0;
        for g in self.genomes.iter_mut().rev().filter(|g| g.born == generation).take(count) {
            let pick = &picks[made % picks.len()];
            let (mut params, mut sigma) = (pick.params.clone(), pick.sigma);
            mutate(&mut params, &mut sigma, cfg, rng);
            *g = Genome { id: ids.fresh(), params, sigma, parents: [Some(pick.id), None], born: generation };
            made += 1;
        }
        made
    }
}

fn random_genome(spec: &NetworkSpec, cfg: &GaConfig, rng: &mut Rng, ids: &mut Ids, generation: u32) -> Genome {
    Genome { id: ids.fresh(), params: spec.random_params(rng), sigma: cfg.sigma_start, parents: [None, None], born: generation }
}

/// What a migration did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Migration {
    /// Island `to` was replaced by a copy of island `from`, shape and all.
    Replaced { from: usize, to: usize },
    /// `count` of island `from`'s top genomes replaced island `to`'s worst.
    Sent { from: usize, to: usize, count: usize },
}

/// Islands compete, compared on `Island::rolling_best` (nothing happens
/// until every island has `migration_window` generations recorded).
/// - The weakest island has the strongest's shape and `migrants` > 0: the
///   strongest's top `migrants` genomes replace its worst, so ideas flow
///   between lineages without wiping one. No gap needed.
/// - Otherwise, if the weakest island not holding a human pick is well
///   behind (below `migration_gap` of the best), it's replaced by a copy of
///   the strongest, shape and all. With `migrants` > 0 that's only for an
///   island of a different shape.
pub fn migrate(islands: &mut [Island], cfg: &GaConfig, ids: &mut Ids, protected: &[u64]) -> Option<Migration> {
    if islands.len() < 2 {
        return None;
    }
    let window = cfg.migration_window.max(1) as usize;
    if islands.iter().any(|i| i.recent_best.len() < window) {
        return None;
    }
    let bests: Vec<f64> = islands.iter().map(Island::rolling_best).collect();
    let order = ranked(&bests);
    let strong = order[0];
    let weakest = order[order.len() - 1];
    if cfg.migrants > 0 && islands[weakest].spec == islands[strong].spec {
        let count = send_migrants(islands, strong, weakest, cfg, ids, protected);
        return (count > 0).then_some(Migration::Sent { from: strong, to: weakest, count });
    }
    let holds_pick = |i: usize| islands[i].genomes.iter().any(|g| protected.contains(&g.id));
    let other_shape = |i: usize| cfg.migrants == 0 || islands[i].spec != islands[strong].spec;
    let weak = *order[1..].iter().rev().find(|&&i| !holds_pick(i) && other_shape(i))?;
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
    Some(Migration::Replaced { from: strong, to: weak })
}

/// Copy island `from`'s top `cfg.migrants` genomes over island `to`'s
/// worst (never over a protected pick). Each migrant keeps its params,
/// sigma and birth generation, gets a fresh id with its source as parent,
/// and brings its fitness and objectives: every island ran the same tracks
/// this generation, so breeding can rank it straight away. Both islands must
/// be evaluated. Returns how many moved.
fn send_migrants(islands: &mut [Island], from: usize, to: usize, cfg: &GaConfig, ids: &mut Ids, protected: &[u64]) -> usize {
    let evaluated = |i: &Island| i.fitness.len() == i.genomes.len();
    if from == to || !evaluated(&islands[from]) || !evaluated(&islands[to]) {
        return 0;
    }
    let src = &islands[from];
    let movers: Vec<(Genome, f64, Option<Objectives>)> = src
        .order(cfg)
        .into_iter()
        .take(cfg.migrants)
        .map(|i| (src.genomes[i].clone(), src.fitness[i], src.objectives.get(i).copied()))
        .collect();
    let dst = &mut islands[to];
    let targets: Vec<usize> =
        dst.order(cfg).into_iter().rev().filter(|&i| !protected.contains(&dst.genomes[i].id)).take(movers.len()).collect();
    for (&slot, (g, fitness, objectives)) in targets.iter().zip(movers) {
        dst.genomes[slot] = Genome { id: ids.fresh(), parents: [Some(g.id), None], ..g };
        dst.fitness[slot] = fitness;
        if let (Some(o), Some(d)) = (objectives, dst.objectives.get_mut(slot)) {
            *d = o;
        }
    }
    targets.len()
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
            migrants: 0,
            migration_window: 1,
            selection: Selection::Pareto,
            reseed_fraction: 0.3,
            reseed_sigma: 0.3,
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

    /// Set an island's fitness and record its best, as the trainer does.
    fn score(island: &mut Island, fitness: Vec<f64>, window: u32) {
        island.fitness = fitness;
        island.record_best(window);
    }

    #[test]
    fn breeding_keeps_elites_and_size() {
        let (mut rng, mut ids) = (Rng::new(1), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        island.fitness = (0..20).map(f64::from).collect();
        let champion = island.genomes[19].clone();
        island.breed(&cfg(), &mut rng, &mut ids, 1, &[]);
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
            island.breed(&cfg(), &mut rng, &mut ids, generation + 1, &[]);
        }
        island.fitness = island.genomes.iter().map(score).collect();
        let last = island.best().unwrap().1;
        assert!(last > first * 0.2, "{first} -> {last}");
    }

    #[test]
    fn pareto_breeding_keeps_both_specialists() {
        let (mut rng, mut ids) = (Rng::new(5), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        // Genome 0 drives far, genome 1 drifts hard, the rest are mediocre.
        island.fitness = (0..20).map(|i| if i < 2 { 50.0 } else { 10.0 }).collect();
        island.objectives = (0..20).map(|i| match i {
            0 => [100.0, 0.0, 0.0],
            1 => [5.0, 0.9, 0.0],
            _ => [10.0, 0.1, 0.0],
        }).collect();
        let (driver, drifter) = (island.genomes[0].clone(), island.genomes[1].clone());
        island.breed(&GaConfig { elite_fraction: 0.1, ..cfg() }, &mut rng, &mut ids, 1, &[]);
        assert!(island.genomes.contains(&driver) && island.genomes.contains(&drifter), "both ends of the front are elites");
    }

    #[test]
    fn restage_resets_sigma_and_reseeds_the_tail() {
        let (mut rng, mut ids) = (Rng::new(6), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        for g in &mut island.genomes {
            g.sigma = 0.01;
        }
        let best = island.genomes[0].params.clone();
        let head = island.genomes[0].clone();
        island.restage(&best, &cfg(), &mut rng, &mut ids, 9);
        assert!(island.genomes.iter().all(|g| g.sigma == cfg().sigma_start));
        let reseeded = island.genomes.iter().filter(|g| g.born == 9).count();
        assert_eq!(reseeded, 6, "30% of 20");
        assert_eq!(island.genomes[0].params, head.params, "the front of the island is kept");
    }

    #[test]
    fn weak_island_takes_the_strong_island_shape() {
        let (mut rng, mut ids) = (Rng::new(3), Ids::default());
        let mut islands = vec![
            Island::new(0, spec(3), &cfg(), &mut rng, &mut ids),
            Island::new(1, spec(5), &cfg(), &mut rng, &mut ids),
        ];
        score(&mut islands[0], vec![100.0; 20], 1);
        score(&mut islands[1], vec![10.0; 20], 1);
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids, &[]), Some(Migration::Replaced { from: 0, to: 1 }));
        assert_eq!(islands[1].spec, spec(3));
        assert_eq!(islands[1].index, 1);

        // Close enough: no migration.
        score(&mut islands[1], vec![95.0; 20], 1);
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids, &[]), None);
    }

    #[test]
    fn kept_genomes_survive_breeding_even_when_gone() {
        let (mut rng, mut ids) = (Rng::new(7), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        island.fitness = (0..20).map(f64::from).collect();
        // The worst genome, and one from outside the island (bred out
        // before the pick reached the trainer).
        let worst = island.genomes[0].clone();
        let outsider = Genome { id: 999, ..island.genomes[1].clone() };
        island.breed(&cfg(), &mut rng, &mut ids, 1, &[worst.clone(), outsider.clone()]);
        assert_eq!(island.genomes.len(), 20);
        assert!(island.genomes.contains(&worst) && island.genomes.contains(&outsider));
        // A kept genome that's also an elite isn't doubled.
        island.fitness = (0..20).map(f64::from).collect();
        let champion = island.genomes[19].clone();
        island.breed(&cfg(), &mut rng, &mut ids, 2, std::slice::from_ref(&champion));
        assert_eq!(island.genomes.iter().filter(|g| g.id == champion.id).count(), 1);
    }

    #[test]
    fn flood_fills_the_tail_with_children_of_the_pick() {
        let (mut rng, mut ids) = (Rng::new(8), Ids::default());
        let mut island = Island::new(0, spec(3), &cfg(), &mut rng, &mut ids);
        island.fitness = (0..20).map(f64::from).collect();
        let pick = island.genomes[3].clone();
        island.breed(&cfg(), &mut rng, &mut ids, 1, std::slice::from_ref(&pick));
        let front: Vec<Genome> = island.genomes.iter().filter(|g| g.born != 1).cloned().collect();
        assert_eq!(island.flood(std::slice::from_ref(&pick), 0.3, &cfg(), &mut rng, &mut ids, 1), 6, "30% of 20");
        let children = island.genomes.iter().filter(|g| g.parents == [Some(pick.id), None]).count();
        assert!(children >= 6, "{children}");
        assert!(island.genomes[14..].iter().all(|g| g.parents == [Some(pick.id), None]), "the tail");
        assert!(front.iter().all(|g| island.genomes.contains(g)), "elites and the pick stay");

        // Two picks share the flood.
        let other = island.genomes[0].clone();
        island.fitness = (0..20).map(f64::from).collect();
        island.breed(&cfg(), &mut rng, &mut ids, 2, &[pick.clone(), other.clone()]);
        assert_eq!(island.flood(&[pick.clone(), other.clone()], 0.3, &cfg(), &mut rng, &mut ids, 2), 6);
        let of = |id| island.genomes[14..].iter().filter(|g| g.parents == [Some(id), None]).count();
        assert_eq!((of(pick.id), of(other.id)), (3, 3));
    }

    #[test]
    fn migration_spares_an_island_holding_a_pick() {
        let (mut rng, mut ids) = (Rng::new(9), Ids::default());
        let mut islands: Vec<Island> = (0..3).map(|i| Island::new(i, spec(3 + i), &cfg(), &mut rng, &mut ids)).collect();
        score(&mut islands[0], vec![100.0; 20], 1);
        score(&mut islands[1], vec![20.0; 20], 1);
        score(&mut islands[2], vec![10.0; 20], 1);
        let pick = islands[2].genomes[5].id;
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids, &[pick]), Some(Migration::Replaced { from: 0, to: 1 }), "next-weakest instead");
        assert!(islands[2].genomes.iter().any(|g| g.id == pick));
        // Every other island holds a pick: no migration.
        let pick1 = islands[1].genomes[0].id;
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids, &[pick, pick1]), None);
    }

    #[test]
    fn same_shape_weak_island_takes_migrants() {
        let cfg = GaConfig { migrants: 5, ..cfg() };
        let (mut rng, mut ids) = (Rng::new(10), Ids::default());
        let mut islands: Vec<Island> = (0..2).map(|i| Island::new(i, spec(3), &cfg, &mut rng, &mut ids)).collect();
        score(&mut islands[0], (0..20).map(|i| 100.0 + f64::from(i)).collect(), 1);
        score(&mut islands[1], (0..20).map(f64::from).collect(), 1);
        let top: Vec<u64> = islands[0].genomes[15..].iter().map(|g| g.id).collect();
        let (pick, champion) = (islands[1].genomes[0].clone(), islands[1].genomes[19].clone());
        let before = islands[0].genomes.clone();

        assert_eq!(migrate(&mut islands, &cfg, &mut ids, &[pick.id]), Some(Migration::Sent { from: 0, to: 1, count: 5 }));
        let dst = &islands[1];
        let migrants: Vec<usize> = (0..20).filter(|&i| dst.genomes[i].parents[0].is_some_and(|p| top.contains(&p))).collect();
        assert_eq!(migrants.len(), 5, "the strong island's top 5 arrive");
        for &i in &migrants {
            let g = &dst.genomes[i];
            assert!(!top.contains(&g.id), "fresh ids");
            let source = before.iter().find(|s| Some(s.id) == g.parents[0]).unwrap();
            assert_eq!(g.params, source.params);
            assert!(dst.fitness[i] >= 115.0, "their fitness comes with them");
        }
        assert!(dst.genomes.contains(&pick), "a protected pick is never overwritten");
        assert!(dst.genomes.contains(&champion), "the weak island's best stays");
        assert_eq!(dst.spec, spec(3));
        assert_eq!(islands[0].genomes, before, "the source island is untouched");
    }

    #[test]
    fn different_shapes_still_get_a_full_copy_with_migrants_on() {
        let cfg = GaConfig { migrants: 5, ..cfg() };
        let (mut rng, mut ids) = (Rng::new(11), Ids::default());
        let mut islands = vec![Island::new(0, spec(3), &cfg, &mut rng, &mut ids), Island::new(1, spec(5), &cfg, &mut rng, &mut ids)];
        score(&mut islands[0], vec![100.0; 20], 1);
        score(&mut islands[1], vec![10.0; 20], 1);
        assert_eq!(migrate(&mut islands, &cfg, &mut ids, &[]), Some(Migration::Replaced { from: 0, to: 1 }));
        assert_eq!(islands[1].spec, spec(3));
    }

    #[test]
    fn without_migrants_a_same_shape_island_is_copied_as_before() {
        let (mut rng, mut ids) = (Rng::new(12), Ids::default());
        let mut islands: Vec<Island> = (0..2).map(|i| Island::new(i, spec(3), &cfg(), &mut rng, &mut ids)).collect();
        score(&mut islands[0], vec![100.0; 20], 1);
        score(&mut islands[1], vec![10.0; 20], 1);
        assert_eq!(migrate(&mut islands, &cfg(), &mut ids, &[]), Some(Migration::Replaced { from: 0, to: 1 }));
    }

    #[test]
    fn migration_waits_for_a_full_window() {
        let cfg = GaConfig { migration_window: 3, ..cfg() };
        let (mut rng, mut ids) = (Rng::new(13), Ids::default());
        let mut islands = vec![Island::new(0, spec(3), &cfg, &mut rng, &mut ids), Island::new(1, spec(5), &cfg, &mut rng, &mut ids)];
        for _ in 0..2 {
            score(&mut islands[0], vec![100.0; 20], 3);
            score(&mut islands[1], vec![10.0; 20], 3);
            assert_eq!(migrate(&mut islands, &cfg, &mut ids, &[]), None);
        }
        score(&mut islands[0], vec![100.0; 20], 3);
        score(&mut islands[1], vec![10.0; 20], 3);
        assert_eq!(migrate(&mut islands, &cfg, &mut ids, &[]), Some(Migration::Replaced { from: 0, to: 1 }));
    }

    #[test]
    fn one_unlucky_generation_doesnt_wipe_an_island() {
        let (mut rng, mut ids) = (Rng::new(14), Ids::default());
        let make = |rng: &mut Rng, ids: &mut Ids| vec![Island::new(0, spec(3), &cfg(), rng, ids), Island::new(1, spec(5), &cfg(), rng, ids)];
        // Island 1 keeps up, then draws badly once: 100, 100, 50.
        let history = |islands: &mut Vec<Island>, window| {
            for f in [100.0, 100.0, 50.0] {
                score(&mut islands[0], vec![100.0; 20], window);
                score(&mut islands[1], vec![f; 20], window);
            }
        };
        let mut averaged = make(&mut rng, &mut ids);
        history(&mut averaged, 3);
        assert_eq!(averaged[1].rolling_best(), 250.0 / 3.0);
        assert_eq!(migrate(&mut averaged, &GaConfig { migration_window: 3, ..cfg() }, &mut ids, &[]), None, "mean 83 is within the gap");
        let mut single = make(&mut rng, &mut ids);
        history(&mut single, 1);
        assert_eq!(migrate(&mut single, &cfg(), &mut ids, &[]), Some(Migration::Replaced { from: 0, to: 1 }), "the old rule wipes it");
    }
}
