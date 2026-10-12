//! Network view: the driving network drawn layer by layer, live. Nodes light
//! up with their activation, edges show what's flowing, and hovering or
//! pinning a node explains it.

mod edges;
mod health;
mod inspect;
mod layout;
mod outputs;

use macroquad::prelude::*;
use nn::{Activation, Network, NetworkSpec};
use sim::sensors::observation::{LAYOUT, label};

use crate::render::{text, theme};
use edges::Mode;
use health::{Health, Status};
use layout::{Layout, Node};
use outputs::OutputHistory;

const STRIP_H: f32 = 34.0;
const DENSITY_RANGE: (f32, f32) = (0.002, 1.0);

/// The network's state at its last decision.
pub struct Snapshot {
    spec: NetworkSpec,
    input: Vec<f32>,
    /// Every layer's output (the last is the network's output).
    activations: Vec<Vec<f32>>,
}

pub struct NetworkView {
    mode: Mode,
    /// Share of each layer's edges drawn.
    density: f32,
    collapsed: [bool; LAYOUT.len()],
    pinned: Option<Node>,
    snapshot: Option<Snapshot>,
    health: Health,
    outputs: OutputHistory,
}

impl NetworkView {
    pub fn new() -> Self {
        Self {
            mode: Mode::Contribution,
            density: 0.05,
            collapsed: [false; LAYOUT.len()],
            pinned: None,
            snapshot: None,
            health: Health::default(),
            outputs: OutputHistory::default(),
        }
    }

    /// A new run: forget the history.
    pub fn clear(&mut self) {
        self.snapshot = None;
        self.health.clear();
        self.outputs.clear();
    }

    /// The network just decided: take its state.
    pub fn on_decision(&mut self, net: &Network, input: &[f32]) {
        if self.snapshot.as_ref().is_some_and(|s| s.spec != net.spec) {
            self.clear();
            self.pinned = None;
        }
        let activations = net.activations().to_vec();
        self.health.push(net, &activations);
        self.outputs.push(activations.last().map_or(&[][..], Vec::as_slice));
        self.snapshot = Some(Snapshot { spec: net.spec.clone(), input: input.to_vec(), activations });
    }

    /// Draw into `rect`, handling the mouse as it goes. `driving`: whether
    /// the network is in control (otherwise it shows its last decision).
    pub fn draw(&mut self, net: &Network, rect: Rect, driving: bool) {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, theme::PANEL_SOLID);
        draw_line(rect.x, rect.y, rect.x, rect.y + rect.h, 2.0, theme::NEON_PINK);
        self.draw_strip(net, rect);
        let body = Rect::new(rect.x, rect.y + STRIP_H, rect.w, rect.h - STRIP_H);
        let Some(snapshot) = self.snapshot.as_ref().filter(|s| s.spec == net.spec) else {
            text("waiting for the network's first decision", body.x + 20.0, body.y + 40.0, 20.0, theme::TEXT_DIM);
            return;
        };

        let sizes: Vec<usize> = (0..net.layer_count()).map(|i| net.layer(i).outputs()).collect();
        let layout = Layout::new(body, &sizes, &self.collapsed);
        let mouse = Vec2::from(mouse_position());
        let in_body = body.contains(mouse);
        let hover = if in_body { layout.hit(mouse) } else { None };
        if in_body {
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                let factor = if wheel > 0.0 { 1.25 } else { 0.8 };
                self.density = (self.density * factor).clamp(DENSITY_RANGE.0, DENSITY_RANGE.1);
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                if let Some(group) = layout.header_at(mouse) {
                    self.collapsed[group] = !self.collapsed[group];
                    if self.pinned.is_some_and(|p| matches!(p, Node::Input(_) | Node::Group(_))) {
                        self.pinned = None;
                    }
                } else {
                    self.pinned = if hover == self.pinned { None } else { hover };
                }
            }
        }
        // Collapsing can leave a pin pointing at a node that's gone.
        let gone = match self.pinned {
            Some(Node::Group(g)) => layout.groups[g].is_none(),
            Some(Node::Input(i)) => !layout.shows_input(i),
            _ => false,
        };
        if gone {
            self.pinned = None;
        }
        let highlight = hover.or(self.pinned);

        for layer in 0..net.layer_count() {
            let source = if layer == 0 { &snapshot.input[..] } else { &snapshot.activations[layer - 1][..] };
            let edges = edges::layer_edges(net, layer, source, self.mode, &layout);
            edges::draw(&edges, &layout, self.density, highlight);
        }
        self.draw_inputs(snapshot, &layout);
        self.draw_layers(net, snapshot, &layout, body);
        if let Some(node) = self.pinned {
            let at = layout.pos(node);
            draw_circle_lines(at.x, at.y, 11.0, 2.0, WHITE);
        }
        if let Some(Node::Neuron { layer, index }) = self.pinned {
            let x = layout.columns[layout.columns.len() - 1] - 14.0;
            let w = layout.output_area + 6.0;
            inspect::contributors_panel(net, snapshot, layer, index, x, body.y + body.h * 0.66 + 10.0, w);
        }
        if !driving {
            text("NOT DRIVING  (Tab: hand back)", body.x + body.w / 2.0 - 140.0, body.y + body.h - 12.0, 22.0, theme::SODIUM);
        }
        if let Some(node) = hover {
            inspect::tooltip(node, net, snapshot, &self.health, mouse);
        }
    }

    /// Controls along the top: edge mode, density, expand/collapse.
    fn draw_strip(&mut self, net: &Network, rect: Rect) {
        let (mut x, y) = (rect.x + 10.0, rect.y + 6.0);
        let mode = match self.mode {
            Mode::Contribution => "edges: live contribution",
            Mode::Weight => "edges: weights",
        };
        if chip(&mut x, y, mode, true) {
            self.mode = if self.mode == Mode::Contribution { Mode::Weight } else { Mode::Contribution };
        }
        if chip(&mut x, y, "-", false) {
            self.density = (self.density * 0.8).max(DENSITY_RANGE.0);
        }
        let shown = format!("{:.1}% of edges", self.density * 100.0);
        x += text(&shown, x, y + 17.0, 16.0, theme::TEXT) + 6.0;
        if chip(&mut x, y, "+", false) {
            self.density = (self.density * 1.25).min(DENSITY_RANGE.1);
        }
        // The rays (group 0) are always bundled; only the rest collapse.
        let any_collapsed = self.collapsed[1..].iter().any(|&c| c);
        if chip(&mut x, y, if any_collapsed { "expand all" } else { "collapse all" }, false) {
            self.collapsed = [!any_collapsed; LAYOUT.len()];
            self.pinned = self.pinned.filter(|p| matches!(p, Node::Neuron { .. }));
        }
        let hint = format!("{}   hover: inspect  click: pin  wheel: density", net.spec.describe());
        let w = measure_text(&hint, None, 14, 1.0).width;
        if x + 20.0 + w < rect.x + rect.w {
            text(&hint, rect.x + rect.w - w - 10.0, y + 16.0, 14.0, theme::TEXT_DIM);
        }
    }

    fn draw_inputs(&self, snapshot: &Snapshot, layout: &Layout) {
        // Mini car in the middle of the ray fan, nose up.
        let c = layout.fan_center;
        draw_circle_lines(c.x, c.y, layout.fan_radius, 1.0, theme::GRID_MAJOR);
        draw_rectangle(c.x - 5.0, c.y - 10.0, 10.0, 20.0, theme::CAR_ROOF);
        draw_rectangle(c.x - 4.0, c.y - 7.0, 8.0, 4.0, theme::CAR_GLASS);

        for (i, &at) in layout.inputs.iter().enumerate() {
            if layout.shows_input(i) {
                node(at, layout.input_r, snapshot.input[i].clamp(-1.0, 1.0));
            }
        }
        for at in layout.groups.iter().flatten() {
            draw_circle(at.x, at.y, 6.0, theme::GRID_MAJOR);
            draw_circle_lines(at.x, at.y, 6.0, 1.5, theme::SODIUM);
        }
        let size = layout.label_size;
        if size >= 8.0 {
            for &(i, at) in &layout.labels {
                text(&label(i), at.x, at.y, size, theme::TEXT_DIM);
            }
        }
        let rays = &LAYOUT[0];
        let size = layout.label_size.max(10.0);
        text(&format!("{} ({})", rays.name, rays.len), layout.rays_label.x, layout.rays_label.y, size, theme::SODIUM);
        let mouse = Vec2::from(mouse_position());
        for h in &layout.headers {
            let group = &LAYOUT[h.group];
            let marker = if self.collapsed[h.group] { "+" } else { "-" };
            let color = if h.rect.contains(mouse) { theme::TEXT } else { theme::SODIUM };
            let size = (h.rect.h + 2.0).clamp(10.0, 15.0);
            text(&format!("{marker} {} ({})", group.name, group.len), h.rect.x, h.rect.y + h.rect.h * 0.85, size, color);
        }
    }

    fn draw_layers(&self, net: &Network, snapshot: &Snapshot, layout: &Layout, body: Rect) {
        let last = net.layer_count() - 1;
        let header_y = body.y + 16.0;
        text("INPUTS", layout.input_x - 60.0, header_y, 16.0, theme::NEON_CYAN);
        // Narrow columns get short headers ("32t", "3d 7s").
        let gap = layout.columns.first().map_or(0.0, |c| c - layout.input_x);
        let short = gap < 110.0;
        for (layer, column) in layout.neurons.iter().enumerate() {
            let view = net.layer(layer);
            let x = layout.columns[layer];
            let r = layout.neuron_r[layer];
            if layer == last {
                text("OUTPUTS", x - r, header_y, 16.0, theme::NEON_CYAN);
            } else {
                let activation = inspect::activation_name(view.activation(0));
                let title = if short {
                    format!("{}{}", column.len(), &activation[..1])
                } else {
                    format!("{} {activation}", column.len())
                };
                centred(&title, x, header_y, 16.0, theme::NEON_CYAN);
                let (dead, saturated) = self.health.counts(layer);
                if dead + saturated > 0 {
                    let health =
                        if short { format!("{dead}d {saturated}s") } else { format!("{dead} dead  {saturated} sat") };
                    centred(&health, x, header_y + 16.0, 14.0, theme::SODIUM);
                }
            }
            for (index, &at) in column.iter().enumerate() {
                let value = snapshot.activations[layer][index];
                node(at, r, display_value(view.activation(index), value));
                match self.health.status(layer, index) {
                    Status::Dead => {
                        draw_circle_lines(at.x, at.y, r + 2.0, 1.5, theme::TEXT_DIM);
                        draw_line(at.x - r, at.y - r, at.x + r, at.y + r, 1.0, theme::TEXT_DIM);
                    }
                    Status::Saturated => draw_circle_lines(at.x, at.y, r + 2.0, 1.5, theme::SODIUM),
                    Status::Ok => {}
                }
                if layer == last {
                    self.outputs.draw(index, at, r, layout.output_area, value, view.activation(index));
                }
            }
        }
    }
}

/// An activation squashed to -1..1 for colouring, so unbounded ones stay
/// comparable.
fn display_value(activation: Activation, value: f32) -> f32 {
    match activation {
        Activation::Tanh | Activation::Softsign | Activation::Sigmoid => value,
        Activation::Relu | Activation::LeakyRelu | Activation::Identity => value.tanh(),
    }
}

/// A node filled by its value: pink for positive, cyan for negative.
fn node(at: Vec2, r: f32, value: f32) {
    let t = value.clamp(-1.0, 1.0);
    let hue = if t >= 0.0 { theme::NEON_PINK } else { theme::NEON_CYAN };
    let base = theme::GRID_MAJOR;
    let k = t.abs();
    let fill = Color::new(
        base.r + (hue.r - base.r) * k,
        base.g + (hue.g - base.g) * k,
        base.b + (hue.b - base.b) * k,
        1.0,
    );
    draw_circle(at.x, at.y, r, fill);
}

fn centred(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let w = measure_text(s, None, size as u16, 1.0).width;
    text(s, x - w / 2.0, y, size, color);
}

/// A clickable chip at `*x` (advanced past it). Returns true when clicked.
fn chip(x: &mut f32, y: f32, label: &str, primary: bool) -> bool {
    let size = 16.0;
    let w = measure_text(label, None, size as u16, 1.0).width + 16.0;
    let r = Rect::new(*x, y, w, 24.0);
    let hot = r.contains(Vec2::from(mouse_position()));
    let (fill, ink) = match (primary, hot) {
        (true, false) => (theme::NEON_PINK, WHITE),
        (true, true) => (WHITE, theme::NEON_PINK),
        (false, false) => (theme::GRID_MAJOR, theme::TEXT),
        (false, true) => (theme::NEON_CYAN, theme::ASPHALT),
    };
    draw_rectangle(r.x, r.y, r.w, r.h, fill);
    text(label, r.x + 8.0, r.y + 17.0, size, ink);
    *x += w + 6.0;
    hot && is_mouse_button_pressed(MouseButton::Left)
}
