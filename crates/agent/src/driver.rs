use nn::{Activation, LayerSpec, Network, NetworkSpec};
use sim::episode::{EpisodeConfig, SteeringOutput};
use sim::sensors::observation::{INPUTS, Observation};
use sim::{Controller, Controls};

/// Output heads: steer (-1..1; a position or a rate, see `SteeringOutput`),
/// then throttle, brake, handbrake, clutch (0..1).
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
    /// Most the steer can move in one decision (rate mode), or `None` to
    /// take the output as the position.
    max_steer_step: Option<f64>,
    /// The steer it asked for last (rate mode adds to it).
    steer: f64,
}

impl NnDriver {
    /// Steers as `episode.steering` says, deciding every
    /// `episode.decision_every` ticks.
    pub fn new(network: Network, episode: &EpisodeConfig) -> Self {
        assert_eq!(network.spec.inputs, INPUTS, "network inputs don't match the observation");
        assert_eq!(network.spec.outputs, OUTPUTS, "network outputs aren't driver outputs");
        let max_steer_step = match episode.steering {
            SteeringOutput::Position => None,
            // Lock to lock is a swing of 2.
            SteeringOutput::Rate { full_lock_secs } => Some(2.0 / full_lock_secs.max(1e-3) * episode.decision_secs()),
        };
        Self { network, max_steer_step, steer: 0.0 }
    }
}

impl Controller for NnDriver {
    fn decide(&mut self, observation: &Observation) -> Controls {
        let out = self.network.forward(&observation.values);
        let steer = f64::from(out[0]);
        self.steer = match self.max_steer_step {
            None => steer,
            Some(step) => (self.steer + steer * step).clamp(-1.0, 1.0),
        };
        Controls {
            steer: self.steer,
            throttle: f64::from(out[1]),
            brake: f64::from(out[2]),
            handbrake: f64::from(out[3]),
            clutch: f64::from(out[4]),
        }
    }

    fn reset(&mut self) {
        self.steer = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVATION: Observation = Observation { values: [0.0; INPUTS] };

    /// Zero weights and every output's bias 0 except steer, so the network
    /// always says `steer`.
    fn driver(steer: f32, steering: SteeringOutput) -> NnDriver {
        let spec = driver_spec(vec![]);
        let mut params = vec![0.0; spec.param_count()];
        params[INPUTS] = steer.atanh();
        let episode = EpisodeConfig {
            max_secs: 10.0,
            start_speed_kmh: 50.0,
            decision_every: 4,
            sensor_noise: 0.0,
            no_progress_secs: 4.0,
            max_backwards: 20.0,
            max_hard_hits: 3,
            end_on_spin: true,
            clutch_grace_secs: 0.4,
            smoothness: Default::default(),
            steering,
        };
        NnDriver::new(Network::new(spec, params), &episode)
    }

    #[test]
    fn position_mode_takes_the_output_as_the_steer() {
        let mut d = driver(0.6, SteeringOutput::Position);
        for _ in 0..3 {
            assert!((d.decide(&OBSERVATION).steer - 0.6).abs() < 1e-6);
        }
    }

    #[test]
    fn rate_mode_turns_the_wheel_at_the_set_speed() {
        // Full output, half a second lock to lock, deciding at 30 Hz: a
        // swing of 2 over 15 decisions.
        let mut d = driver(0.999_999, SteeringOutput::Rate { full_lock_secs: 0.5 });
        let first = d.decide(&OBSERVATION).steer;
        assert!((first - 2.0 / 15.0).abs() < 1e-3, "{first}");
        let steers: Vec<f64> = (0..20).map(|_| d.decide(&OBSERVATION).steer).collect();
        assert!(steers.windows(2).all(|w| w[1] - w[0] <= 2.0 / 15.0 + 1e-6));
        assert_eq!(*steers.last().unwrap(), 1.0, "stops at full lock");
    }

    #[test]
    fn rate_mode_holds_still_at_zero_and_reset_centres_the_wheel() {
        let mut d = driver(-0.5, SteeringOutput::Rate { full_lock_secs: 0.5 });
        for _ in 0..5 {
            d.decide(&OBSERVATION);
        }
        assert!(d.steer < -0.2);
        d.reset();
        assert_eq!(d.steer, 0.0);
        let mut still = driver(0.0, SteeringOutput::Rate { full_lock_secs: 0.5 });
        assert_eq!(still.decide(&OBSERVATION).steer, 0.0);
    }
}
