//! Connections: which to draw and how. Each layer shows only its strongest
//! share of edges; a hovered or pinned node shows all of its own.

use macroquad::prelude::*;
use nn::Network;

use super::layout::{Layout, Node};
use crate::render::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// weight × the source's current value: what's flowing right now.
    Contribution,
    /// The raw weight: the structure evolution has built.
    Weight,
}

pub struct Edge {
    pub from: Node,
    pub to: Node,
    pub value: f32,
}

/// Every edge into weight layer `layer`. `source` is that layer's input.
/// Inputs in collapsed groups are bundled into one edge per group (summed).
pub fn layer_edges(net: &Network, layer: usize, source: &[f32], mode: Mode, layout: &Layout) -> Vec<Edge> {
    let view = net.layer(layer);
    let mut edges = Vec::with_capacity(view.inputs() * view.outputs());
    let mut bundles = vec![0.0f32; layout.groups.len()];
    for r in 0..view.outputs() {
        let to = Node::Neuron { layer, index: r };
        let weights = &view.weights.row(r)[..view.inputs()];
        bundles.fill(0.0);
        for (c, (&w, &x)) in weights.iter().zip(source).enumerate() {
            let value = match mode {
                Mode::Contribution => w * x,
                Mode::Weight => w,
            };
            if layer == 0 {
                let g = layout.input_group[c];
                if layout.groups[g].is_some() {
                    bundles[g] += value;
                    continue;
                }
                edges.push(Edge { from: Node::Input(c), to, value });
            } else {
                edges.push(Edge { from: Node::Neuron { layer: layer - 1, index: c }, to, value });
            }
        }
        for (g, &value) in bundles.iter().enumerate() {
            if layer == 0 && layout.groups[g].is_some() {
                edges.push(Edge { from: Node::Group(g), to, value });
            }
        }
    }
    edges
}

/// Draw the strongest `density` share of `edges` (sized against the
/// layer's largest), then every edge touching `highlight` on top. Edges
/// touching a `hidden` node are left out of the ranking, unless highlighted.
pub fn draw(edges: &[Edge], layout: &Layout, density: f32, highlight: Option<Node>, hidden: &[Node]) {
    let shown = strongest(edges, density, hidden);
    let strength = |i: &usize| edges[*i].value.abs();
    // Scale against what's shown, so hiding a loud node lets the rest speak.
    let max = shown.iter().map(strength).fold(0.0f32, f32::max);
    let max = if max > 0.0 { max } else { edges.iter().map(|e| e.value.abs()).fold(0.0f32, f32::max) };
    if max <= 0.0 {
        return;
    }
    let dim = if highlight.is_some() { 0.25 } else { 1.0 };
    for &i in &shown {
        line(&edges[i], layout, max, dim, 0.0);
    }
    if let Some(h) = highlight {
        for e in edges.iter().filter(|e| e.from == h || e.to == h) {
            line(e, layout, max, 1.0, 0.25);
        }
    }
}

/// Indices of the strongest `density` share of the edges not touching a
/// `hidden` node, weakest first (so the strong ones draw on top).
fn strongest(edges: &[Edge], density: f32, hidden: &[Node]) -> Vec<usize> {
    let mut order: Vec<usize> =
        (0..edges.len()).filter(|&i| !hidden.contains(&edges[i].from) && !hidden.contains(&edges[i].to)).collect();
    if order.is_empty() {
        return order;
    }
    let keep = ((order.len() as f32 * density).ceil() as usize).clamp(1, order.len());
    order.select_nth_unstable_by(keep - 1, |&a, &b| edges[b].value.abs().total_cmp(&edges[a].value.abs()));
    order.truncate(keep);
    order.sort_unstable_by(|&a, &b| edges[a].value.abs().total_cmp(&edges[b].value.abs()));
    order
}

fn line(e: &Edge, layout: &Layout, max: f32, dim: f32, min_alpha: f32) {
    let t = (e.value.abs() / max).sqrt();
    let (a, b) = (layout.pos(e.from), layout.pos(e.to));
    let base = if e.value >= 0.0 { theme::NEON_PINK } else { theme::NEON_CYAN };
    let alpha = (0.08 + 0.8 * t).max(min_alpha) * dim;
    draw_line(a.x, a.y, b.x, b.y, 0.6 + 2.4 * t, Color { a: alpha, ..base });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nn::{Activation, LayerSpec};
    use sim::sensors::observation::{INPUTS, LAYOUT};

    #[test]
    fn collapsed_groups_bundle_their_contributions() {
        let spec = agent::driver_spec(vec![LayerSpec { size: 3, activation: Activation::Tanh }]);
        let net = Network::random(spec, &mut common::Rng::new(1));
        let input: Vec<f32> = (0..INPUTS).map(|i| (i as f32 * 0.1).sin()).collect();
        let rect = Rect::new(0.0, 0.0, 1000.0, 800.0);

        // The rays are always one bundle.
        let rays = LAYOUT[0].len;
        let open = Layout::new(rect, &[3, 5], &[false; LAYOUT.len()]);
        let all = layer_edges(&net, 0, &input, Mode::Contribution, &open);
        assert_eq!(all.len(), (INPUTS - rays + 1) * 3);

        // Collapsing wheels bundles them too.
        let mut collapsed = [false; LAYOUT.len()];
        collapsed[2] = true;
        let shut = Layout::new(rect, &[3, 5], &collapsed);
        let bundled = layer_edges(&net, 0, &input, Mode::Contribution, &shut);
        assert_eq!(bundled.len(), (INPUTS - rays - LAYOUT[2].len + 2) * 3);
        // The rays' bundle into neuron 0 is the sum of their contributions.
        let row = net.layer(0).weights.row(0);
        let expected: f32 = (0..rays).map(|c| row[c] * input[c]).sum();
        let got = bundled.iter().find(|e| e.from == Node::Group(0) && e.to == Node::Neuron { layer: 0, index: 0 });
        assert!((got.unwrap().value - expected).abs() < 1e-5);

        // Groups only bundle the inputs, never later layers.
        let hidden = vec![0.5; 3];
        let out = layer_edges(&net, 1, &hidden, Mode::Contribution, &shut);
        assert_eq!(out.len(), 3 * 5);
        assert!(out.iter().all(|e| matches!(e.from, Node::Neuron { layer: 0, .. })));
    }

    #[test]
    fn hidden_nodes_give_up_their_share() {
        let n = |index| Node::Neuron { layer: 1, index };
        let loud = Node::Group(0);
        let edges = vec![
            Edge { from: loud, to: n(0), value: 9.0 },
            Edge { from: loud, to: n(1), value: -8.0 },
            Edge { from: Node::Input(60), to: n(0), value: 0.5 },
            Edge { from: Node::Input(61), to: n(1), value: -2.0 },
        ];
        // Half the edges: the loud node takes both slots.
        assert_eq!(strongest(&edges, 0.5, &[]), vec![1, 0]);
        // Hidden, its slots go to the rest.
        assert_eq!(strongest(&edges, 0.5, &[loud]), vec![3]);
        assert_eq!(strongest(&edges, 1.0, &[loud]), vec![2, 3]);
        // Hiding a target drops its incoming edges too.
        assert_eq!(strongest(&edges, 1.0, &[loud, n(1)]), vec![2]);
        assert!(strongest(&edges, 1.0, &[loud, n(0), n(1)]).is_empty());
    }
}
