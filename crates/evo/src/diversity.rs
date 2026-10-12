use sim::episode::EpisodeResult;

use crate::Genome;

/// Average distance between genomes' parameters (RMS per parameter), over
/// a deterministic sample. Near zero means the population has collapsed.
pub fn diversity(genomes: &[Genome]) -> f64 {
    let sample: Vec<&Genome> = genomes.iter().step_by((genomes.len() / 16).max(1)).collect();
    let mut total = 0.0;
    let mut pairs = 0;
    for (i, a) in sample.iter().enumerate() {
        for b in &sample[i + 1..] {
            let sq: f64 = a.params.iter().zip(&b.params).map(|(x, y)| f64::from(x - y).powi(2)).sum();
            total += (sq / a.params.len().max(1) as f64).sqrt();
            pairs += 1;
        }
    }
    if pairs == 0 { 0.0 } else { total / f64::from(pairs) }
}

/// Drift angle that counts as 1 in behaviour space.
const ANGLE_SCALE_DEG: f64 = 45.0;

/// Where a genome sits in behaviour space, averaged over its runs: share of
/// a lap covered, share of time drifting, mean drift angle (/45 deg), and
/// shares of time on the handbrake and clutch. Each is roughly 0..1.
pub fn behaviour(results: &[EpisodeResult]) -> [f64; 5] {
    let n = results.len().max(1) as f64;
    let mut b = [0.0; 5];
    for r in results {
        let time = r.time.max(1e-9);
        let s = &r.stats;
        b[0] += r.laps().min(1.0);
        b[1] += s.drift_time / time;
        b[2] += if s.drift_time > 0.0 { s.average_angle() / ANGLE_SCALE_DEG } else { 0.0 };
        b[3] += s.handbrake_time / time;
        b[4] += s.clutch_time / time;
    }
    b.map(|x| x / n)
}

/// Average distance between genomes in behaviour space (normalised so it
/// stays roughly 0..1). Near zero means everyone drives the same way, even if
/// their weights differ.
pub fn behaviour_diversity(behaviours: &[[f64; 5]]) -> f64 {
    let mut total = 0.0;
    let mut pairs = 0;
    for (i, a) in behaviours.iter().enumerate() {
        for b in &behaviours[i + 1..] {
            let sq: f64 = a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum();
            total += (sq / 5.0).sqrt();
            pairs += 1;
        }
    }
    if pairs == 0 { 0.0 } else { total / f64::from(pairs) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_behaviour_has_no_diversity() {
        assert_eq!(behaviour_diversity(&[[0.5; 5]; 4]), 0.0);
        assert!(behaviour_diversity(&[[0.0; 5], [1.0; 5]]) > 0.9);
    }
}
