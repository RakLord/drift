use std::ops::Range;

use common::Rng;

/// Neuron-level crossover: for every neuron, the child takes that neuron's
/// whole set of incoming weights and bias from one parent or the other.
/// Neurons stay intact, which avoids scrambling what each one has learned.
pub fn neuron_crossover(a: &[f32], b: &[f32], neurons: &[Range<usize>], rng: &mut Rng) -> Vec<f32> {
    let mut child = a.to_vec();
    for r in neurons {
        if rng.chance(0.5) {
            child[r.clone()].copy_from_slice(&b[r.clone()]);
        }
    }
    child
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_neuron_comes_whole_from_one_parent() {
        let neurons: Vec<Range<usize>> = (0..10).map(|i| i * 4..i * 4 + 4).collect();
        let a = vec![1.0; 40];
        let b = vec![2.0; 40];
        let mut rng = Rng::new(4);
        let child = neuron_crossover(&a, &b, &neurons, &mut rng);
        for r in &neurons {
            let first = child[r.start];
            assert!(child[r.clone()].iter().all(|&x| x == first), "neuron {r:?} was split");
        }
        let from_b = neurons.iter().filter(|r| child[r.start] == 2.0).count();
        assert!(from_b > 0 && from_b < 10);
    }
}
