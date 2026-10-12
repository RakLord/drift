use nn::{Activation, LayerSpec, Network, NetworkSpec};
use sim::sensors::observation::{INPUTS, Observation};
use sim::{Controller, Controls};

/// Output heads: steer (-1..1), then throttle, brake, handbrake, clutch
/// (0..1).
pub const OUTPUTS: [Activation; 5] =
    [Activation::Tanh, Activation::Sigmoid, Activation::Sigmoid, Activation::Sigmoid, Activation::Sigmoid];

/// A driver network shape with the given hidden layers. Starting priors
/// (output biases, output init scale) are neutral; set them on the spec to
/// give fresh networks a default behaviour.
pub fn driver_spec(hidden: Vec<LayerSpec>) -> NetworkSpec {
    NetworkSpec { inputs: INPUTS, hidden, outputs: OUTPUTS.to_vec(), output_bias: vec![], output_init_scale: 1.0 }
}

/// A network that drives: observation in, controls out.
pub struct NnDriver {
    pub network: Network,
}

impl NnDriver {
    pub fn new(network: Network) -> Self {
        assert_eq!(network.spec.inputs, INPUTS, "network inputs don't match the observation");
        assert_eq!(network.spec.outputs, OUTPUTS, "network outputs aren't driver outputs");
        Self { network }
    }
}

impl Controller for NnDriver {
    fn decide(&mut self, observation: &Observation) -> Controls {
        let out = self.network.forward(&observation.values);
        Controls {
            steer: f64::from(out[0]),
            throttle: f64::from(out[1]),
            brake: f64::from(out[2]),
            handbrake: f64::from(out[3]),
            clutch: f64::from(out[4]),
        }
    }
}
