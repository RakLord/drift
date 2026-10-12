use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Tanh,
    Relu,
    LeakyRelu,
    Sigmoid,
    Softsign,
    Identity,
}

impl Activation {
    pub fn apply(self, x: f32) -> f32 {
        match self {
            Self::Tanh => x.tanh(),
            Self::Relu => x.max(0.0),
            Self::LeakyRelu => {
                if x > 0.0 {
                    x
                } else {
                    0.01 * x
                }
            }
            Self::Sigmoid => 1.0 / (1.0 + (-x).exp()),
            Self::Softsign => x / (1.0 + x.abs()),
            Self::Identity => x,
        }
    }

    /// Whether weights should be initialised for a rectifier (He) rather
    /// than a squashing function (Xavier).
    pub fn is_rectifier(self) -> bool {
        matches!(self, Self::Relu | Self::LeakyRelu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(Activation::Relu.apply(-2.0), 0.0);
        assert_eq!(Activation::LeakyRelu.apply(-2.0), -0.02);
        assert_eq!(Activation::Sigmoid.apply(0.0), 0.5);
        assert_eq!(Activation::Softsign.apply(1.0), 0.5);
        assert!((Activation::Tanh.apply(1.0) - 0.761_594_2).abs() < 1e-6);
    }
}
