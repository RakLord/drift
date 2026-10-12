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
