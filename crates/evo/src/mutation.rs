use common::Rng;

use crate::GaConfig;

/// Self-adaptive mutation: the genome's own step size drifts (log-normally)
/// first, then a random subset of parameters gets Gaussian noise of that
/// size. Good step sizes survive along with the genomes that use them.
pub fn mutate(params: &mut [f32], sigma: &mut f64, cfg: &GaConfig, rng: &mut Rng) {
    *sigma = (*sigma * (cfg.sigma_adapt * rng.gaussian()).exp()).clamp(cfg.sigma_min, cfg.sigma_max);
    for p in params.iter_mut() {
        if rng.chance(cfg.reset_rate) {
            *p = rng.range(-1.0, 1.0) as f32;
        } else if rng.chance(cfg.mutation_rate) {
            *p += (rng.gaussian() * *sigma) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GaConfig;

    pub fn cfg() -> GaConfig {
        GaConfig {
            population: 10,
            elite_fraction: 0.1,
            tournament_size: 3,
            crossover_rate: 0.5,
            mutation_rate: 0.1,
            sigma_start: 0.1,
            sigma_min: 0.01,
            sigma_max: 0.5,
            sigma_adapt: 0.2,
            reset_rate: 0.0,
            immigrants: 0,
            migration_every: 10,
            migration_gap: 0.8,
        }
    }

    #[test]
    fn mutates_about_the_right_share() {
        let mut params = vec![0.0f32; 10_000];
        let mut sigma = 0.1;
        mutate(&mut params, &mut sigma, &cfg(), &mut Rng::new(1));
        let changed = params.iter().filter(|&&p| p != 0.0).count();
        assert!((800..1200).contains(&changed), "{changed}");
        assert!((0.01..=0.5).contains(&sigma));
    }
}
