//! Starting weights. Uniform Xavier for squashing activations, He for
//! rectifiers; biases start at zero.

use common::Rng;

use crate::Activation;

pub fn weight_limit(fan_in: usize, fan_out: usize, activation: Activation) -> f64 {
    if activation.is_rectifier() {
        (6.0 / fan_in as f64).sqrt()
    } else {
        (6.0 / (fan_in + fan_out) as f64).sqrt()
    }
}

/// Fill one layer's rows (`inputs` weights then a bias each).
pub fn fill_layer(rows: &mut [f32], inputs: usize, outputs: usize, activation: Activation, scale: f64, rng: &mut Rng) {
    let limit = weight_limit(inputs, outputs, activation) * scale;
    for row in rows.chunks_exact_mut(inputs + 1) {
        for w in &mut row[..inputs] {
            *w = rng.range(-limit, limit) as f32;
        }
        row[inputs] = 0.0;
    }
}
