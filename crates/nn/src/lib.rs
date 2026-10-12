//! A small neural network library, written for this project.
//!
//! Feed-forward networks of dense layers, with the parameters kept in one
//! flat `Vec<f32>` so a genetic algorithm can treat a network as a list of
//! numbers. Each neuron's weights and bias are stored together as one row, so
//! "take this neuron from that parent" is a contiguous slice.

pub mod activation;
pub mod init;
pub mod matrix;
pub mod network;

pub use activation::Activation;
pub use network::{LayerSpec, LayerView, Network, NetworkSpec};
