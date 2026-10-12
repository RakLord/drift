//! Neuron health: over the last few seconds of decisions, which hidden
//! neurons never change (dead) and which sit pinned at their limits
//! (saturated). Either way the neuron isn't telling the next layer anything.

use std::collections::VecDeque;

use nn::{Activation, Network};

/// Decisions remembered: 5 s at 30 Hz.
pub const WINDOW: usize = 150;
/// Don't judge a neuron on less than this (1 s).
const MIN_SAMPLES: usize = 30;
const DEAD_STD: f32 = 0.01;
const SATURATED_SHARE: f32 = 0.9;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ok,
    Dead,
    Saturated,
}

/// Whether `a` sits at the limit of `activation` (rectifiers have none).
fn at_limit(activation: Activation, a: f32) -> bool {
    match activation {
        Activation::Tanh | Activation::Softsign => a.abs() > 0.97,
        Activation::Sigmoid => !(0.03..=0.97).contains(&a),
        Activation::Relu | Activation::LeakyRelu | Activation::Identity => false,
    }
}

pub fn classify(activation: Activation, samples: &[f32]) -> Status {
    if samples.len() < MIN_SAMPLES {
        return Status::Ok;
    }
    let n = samples.len() as f32;
    let saturated = samples.iter().filter(|&&a| at_limit(activation, a)).count() as f32 / n;
    if saturated > SATURATED_SHARE {
        return Status::Saturated;
    }
    let mean = samples.iter().sum::<f32>() / n;
    let variance = samples.iter().map(|a| (a - mean).powi(2)).sum::<f32>() / n;
    if variance.sqrt() < DEAD_STD { Status::Dead } else { Status::Ok }
}

#[derive(Default)]
pub struct Health {
    /// Recent activations per hidden layer, per neuron.
    samples: Vec<Vec<VecDeque<f32>>>,
    status: Vec<Vec<Status>>,
}

impl Health {
    pub fn clear(&mut self) {
        self.samples.clear();
        self.status.clear();
    }

    /// Record one decision. `activations` is every layer's output; the last
    /// (the outputs) isn't judged.
    pub fn push(&mut self, net: &Network, activations: &[Vec<f32>]) {
        let hidden = &activations[..activations.len().saturating_sub(1)];
        let shape_changed =
            self.samples.len() != hidden.len() || self.samples.iter().zip(hidden).any(|(s, h)| s.len() != h.len());
        if shape_changed {
            self.samples = hidden.iter().map(|h| vec![VecDeque::with_capacity(WINDOW); h.len()]).collect();
        }
        let mut scratch = Vec::with_capacity(WINDOW);
        self.status = hidden
            .iter()
            .enumerate()
            .map(|(layer, values)| {
                let activation = net.layer(layer).activation(0);
                values
                    .iter()
                    .zip(&mut self.samples[layer])
                    .map(|(&a, history)| {
                        if history.len() == WINDOW {
                            history.pop_front();
                        }
                        history.push_back(a);
                        scratch.clear();
                        scratch.extend(history.iter().copied());
                        classify(activation, &scratch)
                    })
                    .collect()
            })
            .collect();
    }

    pub fn status(&self, layer: usize, index: usize) -> Status {
        self.status.get(layer).and_then(|l| l.get(index)).copied().unwrap_or(Status::Ok)
    }

    /// (dead, saturated) neurons in a hidden layer.
    pub fn counts(&self, layer: usize) -> (usize, usize) {
        let Some(l) = self.status.get(layer) else { return (0, 0) };
        let count = |s| l.iter().filter(|&&x| x == s).count();
        (count(Status::Dead), count(Status::Saturated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(n: usize, amplitude: f32, offset: f32) -> Vec<f32> {
        (0..n).map(|i| offset + amplitude * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn classifies_dead_saturated_and_healthy_neurons() {
        // Too few samples to judge.
        assert_eq!(classify(Activation::Relu, &[0.0; 10]), Status::Ok);
        // A relu stuck at zero, or any neuron that never moves, is dead.
        assert_eq!(classify(Activation::Relu, &[0.0; 60]), Status::Dead);
        assert_eq!(classify(Activation::Tanh, &wave(60, 0.001, 0.3)), Status::Dead);
        // Pinned at a limit (even if it wobbles there) is saturated.
        assert_eq!(classify(Activation::Tanh, &wave(60, 0.005, -0.99)), Status::Saturated);
        assert_eq!(classify(Activation::Sigmoid, &[0.999; 60]), Status::Saturated);
        // Rectifiers can't saturate: a constant big value is just dead.
        assert_eq!(classify(Activation::Relu, &[5.0; 60]), Status::Dead);
        // Moving through its range is healthy.
        assert_eq!(classify(Activation::Tanh, &wave(60, 0.6, 0.0)), Status::Ok);
        assert_eq!(classify(Activation::Relu, &wave(60, 1.0, 1.0)), Status::Ok);
    }
}
