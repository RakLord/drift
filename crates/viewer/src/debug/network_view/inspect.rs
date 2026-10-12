//! Inspecting one node: a tooltip with its numbers, and for a pinned neuron
//! the inputs that push it hardest right now.

use macroquad::prelude::*;
use nn::{Activation, Network};
use sim::sensors::observation::{LAYOUT, label};

use super::Snapshot;
use super::health::{Health, Status};
use super::layout::Node;
use super::outputs;
use crate::render::{text, theme};

const CONTRIBUTORS: usize = 8;

pub fn activation_name(a: Activation) -> &'static str {
    match a {
        Activation::Tanh => "tanh",
        Activation::Relu => "relu",
        Activation::LeakyRelu => "leaky relu",
        Activation::Sigmoid => "sigmoid",
        Activation::Softsign => "softsign",
        Activation::Identity => "linear",
    }
}

pub fn neuron_name(net: &Network, layer: usize, index: usize) -> String {
    if layer == net.layer_count() - 1 { outputs::name(index) } else { format!("L{} #{index}", layer + 1) }
}

/// The input to weight layer `layer` at the last decision.
fn source(snapshot: &Snapshot, layer: usize) -> &[f32] {
    if layer == 0 { &snapshot.input } else { &snapshot.activations[layer - 1] }
}

fn source_name(net: &Network, layer: usize, c: usize) -> String {
    if layer == 0 { label(c) } else { neuron_name(net, layer - 1, c) }
}

pub fn tooltip(node: Node, net: &Network, snapshot: &Snapshot, health: &Health, at: Vec2) {
    let lines = match node {
        Node::Input(i) => vec![label(i), format!("value {:+.3}", snapshot.input[i])],
        Node::Group(0) => {
            vec![format!("{} ({} inputs)", LAYOUT[0].name, LAYOUT[0].len), "bundled; each ray's value is in the fan".to_owned()]
        }
        Node::Group(g) => {
            vec![format!("{} ({} inputs, collapsed)", LAYOUT[g].name, LAYOUT[g].len), "click its header to expand".to_owned()]
        }
        Node::Neuron { layer, index } => {
            let view = net.layer(layer);
            let activation = view.activation(index);
            let pre = view.pre_activation(index, source(snapshot, layer));
            let mut lines = vec![
                neuron_name(net, layer, index),
                format!("{} {:+.3}", activation_name(activation), snapshot.activations[layer][index]),
                format!("pre {pre:+.3}   bias {:+.3}", view.bias(index)),
            ];
            match health.status(layer, index) {
                Status::Dead => lines.push("dead: hasn't changed in 5 s".to_owned()),
                Status::Saturated => lines.push("saturated: pinned at its limit".to_owned()),
                Status::Ok => {}
            }
            lines
        }
    };
    let size = 16.0;
    let w = lines.iter().map(|l| measure_text(l, None, size as u16, 1.0).width).fold(0.0, f32::max) + 16.0;
    let h = lines.len() as f32 * 18.0 + 10.0;
    let x = (at.x + 14.0).min(screen_width() - w - 4.0);
    let y = (at.y + 14.0).min(screen_height() - h - 4.0);
    draw_rectangle(x, y, w, h, theme::PANEL_SOLID);
    draw_rectangle_lines(x, y, w, h, 1.0, theme::NEON_CYAN);
    for (i, line) in lines.iter().enumerate() {
        let color = if i == 0 { theme::NEON_CYAN } else { theme::TEXT };
        text(line, x + 8.0, y + 20.0 + i as f32 * 18.0, size, color);
    }
}

/// The biggest `weight × value` terms feeding a neuron, largest first.
pub fn top_contributors(net: &Network, snapshot: &Snapshot, layer: usize, index: usize) -> Vec<(usize, f32)> {
    let view = net.layer(layer);
    let row = view.weights.row(index);
    let mut terms: Vec<(usize, f32)> = source(snapshot, layer).iter().enumerate().map(|(c, &x)| (c, row[c] * x)).collect();
    terms.sort_unstable_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
    terms.truncate(CONTRIBUTORS);
    terms
}

/// List of what drives the pinned neuron, in the box `(x, y, w)`.
pub fn contributors_panel(net: &Network, snapshot: &Snapshot, layer: usize, index: usize, x: f32, y: f32, w: f32) {
    let terms = top_contributors(net, snapshot, layer, index);
    let view = net.layer(layer);
    let h = 52.0 + (terms.len() + 1) as f32 * 17.0;
    draw_rectangle(x, y, w, h, theme::PANEL);
    let name = neuron_name(net, layer, index);
    text(&format!("{name}  <-"), x + 8.0, y + 20.0, 18.0, theme::NEON_CYAN);
    let pre = view.pre_activation(index, source(snapshot, layer));
    let value = snapshot.activations[layer][index];
    text(&format!("pre {pre:+.2} -> {value:+.2}"), x + 8.0, y + 38.0, 14.0, theme::TEXT_DIM);
    let rows = terms.iter().map(|&(c, v)| (source_name(net, layer, c), v)).chain([("bias".to_owned(), view.bias(index))]);
    for (i, (label, v)) in rows.enumerate() {
        let ly = y + 58.0 + i as f32 * 17.0;
        let color = if v >= 0.0 { theme::NEON_PINK } else { theme::NEON_CYAN };
        text(&label, x + 8.0, ly, 15.0, theme::TEXT);
        let shown = format!("{v:+.2}");
        let tw = measure_text(&shown, None, 15, 1.0).width;
        text(&shown, x + w - tw - 8.0, ly, 15.0, color);
    }
}
