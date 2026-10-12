use common::Rng;

/// Tournament selection: pick `size` at random, return the one that comes
/// first in a best-first ordering. `position[i]` is genome `i`'s place in
/// that ordering (see `positions`).
pub fn tournament(position: &[usize], size: usize, rng: &mut Rng) -> usize {
    let n = position.len() as u32;
    let mut best = rng.below(n) as usize;
    for _ in 1..size.max(1) {
        let i = rng.below(n) as usize;
        if position[i] < position[best] {
            best = i;
        }
    }
    best
}

/// Inverse of a best-first ordering: where each genome sits in it.
pub fn positions(order: &[usize]) -> Vec<usize> {
    let mut position = vec![0; order.len()];
    for (place, &i) in order.iter().enumerate() {
        position[i] = place;
    }
    position
}

fn dominates(a: &[f64; 2], b: &[f64; 2]) -> bool {
    a.iter().zip(b).all(|(x, y)| x >= y) && a.iter().zip(b).any(|(x, y)| x > y)
}

/// Pareto ranking, maximising both objectives: each genome's front (0 = not
/// beaten on both objectives by anyone) and crowding distance within its
/// front (bigger = more unusual, infinite at the front's ends).
pub fn pareto(objectives: &[[f64; 2]]) -> Vec<(usize, f64)> {
    let n = objectives.len();
    let mut front = vec![usize::MAX; n];
    let mut remaining: Vec<usize> = (0..n).collect();
    let mut rank = 0;
    while !remaining.is_empty() {
        let current: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&i| !remaining.iter().any(|&j| dominates(&objectives[j], &objectives[i])))
            .collect();
        for &i in &current {
            front[i] = rank;
        }
        remaining.retain(|i| front[*i] == usize::MAX);
        rank += 1;
    }

    let mut crowding = vec![0.0; n];
    for r in 0..rank {
        let members: Vec<usize> = (0..n).filter(|&i| front[i] == r).collect();
        for k in [0, 1] {
            let mut sorted = members.clone();
            sorted.sort_by(|&a, &b| objectives[a][k].total_cmp(&objectives[b][k]).then(a.cmp(&b)));
            let (lo, hi) = (objectives[sorted[0]][k], objectives[sorted[sorted.len() - 1]][k]);
            crowding[sorted[0]] = f64::INFINITY;
            crowding[sorted[sorted.len() - 1]] = f64::INFINITY;
            if hi > lo {
                for w in sorted.windows(3) {
                    crowding[w[1]] += (objectives[w[2]][k] - objectives[w[0]][k]) / (hi - lo);
                }
            }
        }
    }
    front.into_iter().zip(crowding).collect()
}

/// Best first by Pareto front, then by crowding (most unusual first).
pub fn pareto_order(objectives: &[[f64; 2]]) -> Vec<usize> {
    let ranks = pareto(objectives);
    let mut order: Vec<usize> = (0..objectives.len()).collect();
    order.sort_by(|&a, &b| ranks[a].0.cmp(&ranks[b].0).then(ranks[b].1.total_cmp(&ranks[a].1)).then(a.cmp(&b)));
    order
}

/// Indices sorted fittest first (ties keep their order, so it's deterministic).
pub fn ranked(fitness: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..fitness.len()).collect();
    order.sort_by(|&a, &b| fitness[b].total_cmp(&fitness[a]));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigger_tournaments_favour_the_fittest() {
        let fitness: Vec<f64> = (0..100).map(f64::from).collect();
        let position = positions(&ranked(&fitness));
        let mean_pick = |size| {
            let mut rng = Rng::new(9);
            (0..2000).map(|_| tournament(&position, size, &mut rng) as f64).sum::<f64>() / 2000.0
        };
        assert!(mean_pick(1) < 55.0);
        assert!(mean_pick(4) > 75.0);
    }

    #[test]
    fn ranking() {
        assert_eq!(ranked(&[1.0, 5.0, 3.0]), vec![1, 2, 0]);
        assert_eq!(positions(&[1, 2, 0]), vec![2, 0, 1]);
    }

    #[test]
    fn pareto_fronts_and_crowding() {
        // 0: fast grip driver, 1: slow drifter, 2: both middling, 3: worse
        // than 2 at everything, 4: a copy of 2's neighbour space.
        let points = [[10.0, 0.0], [2.0, 0.8], [6.0, 0.4], [5.0, 0.3], [7.0, 0.35]];
        let ranks = pareto(&points);
        assert_eq!(ranks[0].0, 0);
        assert_eq!(ranks[1].0, 0, "the slow drifter survives on the drift axis");
        assert_eq!(ranks[2].0, 0);
        assert_eq!(ranks[4].0, 0);
        assert_eq!(ranks[3].0, 1, "beaten on both by genome 2");
        assert!(ranks[0].1.is_infinite() && ranks[1].1.is_infinite());
        let order = pareto_order(&points);
        assert_eq!(order[order.len() - 1], 3);
    }
}
