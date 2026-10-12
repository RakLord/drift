use crate::Controls;
use crate::sensors::observation::Observation;

/// Anything that can drive a car: a neural network, a script, a human.
pub trait Controller {
    /// Choose controls for what the car currently senses.
    fn decide(&mut self, observation: &Observation) -> Controls;

    /// Forget any per-run state before a new run.
    fn reset(&mut self) {}
}
