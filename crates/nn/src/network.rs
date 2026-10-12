use std::ops::Range;

use common::Rng;
use serde::{Deserialize, Serialize};

use crate::Activation;
use crate::init;
use crate::matrix::AffineRows;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerSpec {
    pub size: usize,
    pub activation: Activation,
}

/// The shape of a network: inputs, hidden layers, and one activation per
/// output (outputs can differ, e.g. steering tanh, pedals sigmoid).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkSpec {
    pub inputs: usize,
    pub hidden: Vec<LayerSpec>,
    pub outputs: Vec<Activation>,
    /// Starting bias for each output (empty = all zero), and a scale on the
    /// output layer's starting weights (1 = normal). Only affect fresh
    /// networks; left neutral, a new network starts out erratic.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_bias: Vec<f32>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub output_init_scale: f64,
}

fn one() -> f64 {
    1.0
}

fn is_one(x: &f64) -> bool {
    *x == 1.0
}

/// Where one layer's rows live in the flat parameters.
#[derive(Clone, Debug)]
struct Layer {
    offset: usize,
    inputs: usize,
    outputs: usize,
}

impl Layer {
    fn len(&self) -> usize {
        self.outputs * (self.inputs + 1)
    }
}

impl NetworkSpec {
    fn layers(&self) -> Vec<Layer> {
        let mut sizes = vec![self.inputs];
        sizes.extend(self.hidden.iter().map(|l| l.size));
        sizes.push(self.outputs.len());
        let mut offset = 0;
        sizes
            .windows(2)
            .map(|w| {
                let layer = Layer { offset, inputs: w[0], outputs: w[1] };
                offset += layer.len();
                layer
            })
            .collect()
    }

    pub fn param_count(&self) -> usize {
        self.layers().iter().map(Layer::len).sum()
    }

    /// The parameter range of every neuron (its incoming weights and bias),
    /// in order: hidden layers first, then outputs.
    pub fn neuron_ranges(&self) -> Vec<Range<usize>> {
        self.layers()
            .iter()
            .flat_map(|l| (0..l.outputs).map(move |r| l.offset + r * (l.inputs + 1)..l.offset + (r + 1) * (l.inputs + 1)))
            .collect()
    }

    /// Short description, e.g. "110-32t-32t-5".
    pub fn describe(&self) -> String {
        let mut parts = vec![self.inputs.to_string()];
        for l in &self.hidden {
            let tag = match l.activation {
                Activation::Tanh => "t",
                Activation::Relu => "r",
                Activation::LeakyRelu => "l",
                Activation::Sigmoid => "s",
                Activation::Softsign => "ss",
                Activation::Identity => "i",
            };
            parts.push(format!("{}{tag}", l.size));
        }
        parts.push(self.outputs.len().to_string());
        parts.join("-")
    }

    /// Freshly initialised parameters for this shape.
    pub fn random_params(&self, rng: &mut Rng) -> Vec<f32> {
        let mut params = vec![0.0; self.param_count()];
        let layers = self.layers();
        let last = layers.len() - 1;
        for (i, l) in layers.iter().enumerate() {
            let activation = if i == last { self.outputs[0] } else { self.hidden[i].activation };
            let scale = if i == last { self.output_init_scale } else { 1.0 };
            init::fill_layer(&mut params[l.offset..l.offset + l.len()], l.inputs, l.outputs, activation, scale, rng);
        }
        let output = &layers[last];
        for (row, &bias) in self.output_bias.iter().enumerate().take(output.outputs) {
            params[output.offset + row * (output.inputs + 1) + output.inputs] = bias;
        }
        params
    }
}

/// A read-only look at one layer, for tools that draw or inspect a network.
pub struct LayerView<'a> {
    /// One row per neuron: its incoming weights, then its bias.
    pub weights: AffineRows<'a>,
    activations: &'a [Activation],
}

impl LayerView<'_> {
    pub fn inputs(&self) -> usize {
        self.weights.inputs
    }

    pub fn outputs(&self) -> usize {
        self.weights.rows
    }

    /// The activation function of neuron `row`.
    pub fn activation(&self, row: usize) -> Activation {
        if self.activations.len() == 1 { self.activations[0] } else { self.activations[row] }
    }

    pub fn bias(&self, row: usize) -> f32 {
        self.weights.row(row)[self.weights.inputs]
    }

    /// Neuron `row`'s weighted sum before its activation, given this layer's
    /// input.
    pub fn pre_activation(&self, row: usize, input: &[f32]) -> f32 {
        let r = self.weights.row(row);
        r[..self.weights.inputs].iter().zip(input).map(|(w, x)| w * x).sum::<f32>() + r[self.weights.inputs]
    }
}

/// A runnable network: a shape plus its parameters, with scratch buffers so
/// `forward` doesn't allocate.
#[derive(Clone, Debug)]
pub struct Network {
    pub spec: NetworkSpec,
    params: Vec<f32>,
    layers: Vec<Layer>,
    buffers: Vec<Vec<f32>>,
}

impl Network {
    pub fn new(spec: NetworkSpec, params: Vec<f32>) -> Self {
        assert_eq!(params.len(), spec.param_count(), "parameter count doesn't match {}", spec.describe());
        let layers = spec.layers();
        let buffers = layers.iter().map(|l| vec![0.0; l.outputs]).collect();
        Self { spec, params, layers, buffers }
    }

    pub fn random(spec: NetworkSpec, rng: &mut Rng) -> Self {
        let params = spec.random_params(rng);
        Self::new(spec, params)
    }

    pub fn params(&self) -> &[f32] {
        &self.params
    }

    pub fn set_params(&mut self, params: &[f32]) {
        assert_eq!(params.len(), self.params.len());
        self.params.copy_from_slice(params);
    }

    /// Number of weight layers (hidden layers + the output layer).
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Weight layer `i`: 0 reads the network's input, the last produces its
    /// outputs.
    pub fn layer(&self, i: usize) -> LayerView<'_> {
        let l = &self.layers[i];
        let activations = if i == self.layers.len() - 1 {
            &self.spec.outputs[..]
        } else {
            std::slice::from_ref(&self.spec.hidden[i].activation)
        };
        LayerView {
            weights: AffineRows { data: &self.params[l.offset..l.offset + l.len()], rows: l.outputs, inputs: l.inputs },
            activations,
        }
    }

    /// Every layer's output from the last `forward` (after its activation),
    /// hidden layers first; the last is the network's output.
    pub fn activations(&self) -> &[Vec<f32>] {
        &self.buffers
    }

    /// Run the network. The returned slice is valid until the next call.
    pub fn forward(&mut self, input: &[f32]) -> &[f32] {
        assert_eq!(input.len(), self.spec.inputs);
        let last = self.layers.len() - 1;
        for i in 0..self.layers.len() {
            let l = &self.layers[i];
            let rows = AffineRows { data: &self.params[l.offset..l.offset + l.len()], rows: l.outputs, inputs: l.inputs };
            let (before, rest) = self.buffers.split_at_mut(i);
            let out = &mut rest[0];
            let source = if i == 0 { input } else { &before[i - 1] };
            rows.apply(source, out);
            if i == last {
                for (o, a) in out.iter_mut().zip(&self.spec.outputs) {
                    *o = a.apply(*o);
                }
            } else {
                let a = self.spec.hidden[i].activation;
                for o in out.iter_mut() {
                    *o = a.apply(*o);
                }
            }
        }
        &self.buffers[last]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> NetworkSpec {
        NetworkSpec {
            inputs: 2,
            hidden: vec![LayerSpec { size: 2, activation: Activation::Relu }],
            outputs: vec![Activation::Identity, Activation::Sigmoid],
            output_bias: vec![],
            output_init_scale: 1.0,
        }
    }

    #[test]
    fn forward_matches_hand_calculation() {
        // Hidden: h0 = relu(1*x0 + 1*x1 + 0), h1 = relu(-1*x0 + 0*x1 + 1)
        // Out:    o0 = 2*h0 + 3*h1 - 1,       o1 = sigmoid(0*h0 + 0*h1 + 0)
        let params = vec![1.0, 1.0, 0.0, -1.0, 0.0, 1.0, 2.0, 3.0, -1.0, 0.0, 0.0, 0.0];
        let mut net = Network::new(tiny(), params);
        let out = net.forward(&[0.5, 0.25]);
        let (h0, h1) = (0.75f32, 0.5f32);
        assert_eq!(out, &[2.0 * h0 + 3.0 * h1 - 1.0, 0.5]);
    }

    #[test]
    fn counts_and_neuron_ranges() {
        let spec = tiny();
        assert_eq!(spec.param_count(), 2 * 3 + 2 * 3);
        let ranges = spec.neuron_ranges();
        assert_eq!(ranges, vec![0..3, 3..6, 6..9, 9..12]);
        assert_eq!(spec.describe(), "2-2r-2");
    }

    #[test]
    fn layer_views_match_the_flat_parameters_and_forward() {
        let spec = NetworkSpec {
            hidden: vec![LayerSpec { size: 3, activation: Activation::Tanh }],
            outputs: vec![Activation::Tanh, Activation::Sigmoid],
            ..tiny()
        };
        let mut net = Network::random(spec.clone(), &mut Rng::new(5));
        let input = [0.3, -0.7];
        let out = net.forward(&input).to_vec();
        assert_eq!(net.layer_count(), 2);

        // Each view's rows are the neuron ranges, in order.
        let ranges = spec.neuron_ranges();
        let rows: Vec<&[f32]> = (0..2)
            .flat_map(|i| {
                let layer = net.layer(i);
                (0..layer.outputs()).map(move |r| layer.weights.row(r))
            })
            .collect();
        for (row, range) in rows.iter().zip(&ranges) {
            assert_eq!(*row, &net.params()[range.clone()]);
        }

        // Recomputing each layer from the stored activations reproduces forward.
        let acts = net.activations();
        assert_eq!(acts.last().unwrap(), &out);
        for i in 0..2 {
            let layer = net.layer(i);
            let source: &[f32] = if i == 0 { &input } else { &acts[i - 1] };
            for (r, &stored) in acts[i].iter().enumerate() {
                let a = layer.activation(r).apply(layer.pre_activation(r, source));
                assert!((a - stored).abs() < 1e-6);
            }
        }
        assert_eq!(net.layer(1).activation(1), Activation::Sigmoid);
        assert_eq!(net.layer(0).bias(2), net.params()[ranges[2].end - 1]);
    }

    #[test]
    fn output_bias_sets_starting_behaviour() {
        let spec = NetworkSpec { output_bias: vec![0.0, 2.0], ..tiny() };
        let mut net = Network::random(spec, &mut Rng::new(3));
        // With zero inputs the hidden layer is zero, so outputs are the biases.
        let out = net.forward(&[0.0, 0.0]);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - Activation::Sigmoid.apply(2.0)).abs() < 1e-6);
    }

    #[test]
    fn params_round_trip_and_init_is_seeded() {
        let spec = NetworkSpec {
            inputs: 110,
            hidden: vec![LayerSpec { size: 32, activation: Activation::Tanh }; 2],
            outputs: vec![Activation::Tanh; 5],
            output_bias: vec![],
            output_init_scale: 1.0,
        };
        let a = Network::random(spec.clone(), &mut Rng::new(1));
        let b = Network::random(spec.clone(), &mut Rng::new(1));
        assert_eq!(a.params(), b.params());
        let mut c = Network::random(spec, &mut Rng::new(2));
        assert_ne!(a.params(), c.params());
        c.set_params(a.params());
        assert_eq!(a.params(), c.params());
    }
}
