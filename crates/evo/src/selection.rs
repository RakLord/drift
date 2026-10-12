use common::Rng;

/// Tournament selection: pick `size` at random, return the fittest.
pub fn tournament(fitness: &[f64], size: usize, rng: &mut Rng) -> usize {
    let n = fitness.len() as u32;
    let mut best = rng.below(n) as usize;
    for _ in 1..size.max(1) {
        let i = rng.below(n) as usize;
        if fitness[i] > fitness[best] {
            best = i;
        }
    }
    best
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
        let mean_pick = |size| {
            let mut rng = Rng::new(9);
            (0..2000).map(|_| tournament(&fitness, size, &mut rng) as f64).sum::<f64>() / 2000.0
        };
        assert!(mean_pick(1) < 55.0);
        assert!(mean_pick(4) > 75.0);
    }

    #[test]
    fn ranking() {
        assert_eq!(ranked(&[1.0, 5.0, 3.0]), vec![1, 2, 0]);
    }
}
