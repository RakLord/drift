//! Where every node of the network view sits for a given screen rect: the
//! ray fan and grouped input column on the left, one column per layer after.
//!
//! The rays are shown in the fan for their values, but feed the network
//! through a single bundled node at the top of the column, so 48 bundles of
//! lines don't cover the inputs.

use macroquad::prelude::*;
use sim::sensors::observation::LAYOUT;
use sim::sensors::rays;

/// Space above the columns for their headers.
pub const HEADER_H: f32 = 40.0;
const ROW_MAX: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    Input(usize),
    /// A collapsed input group, standing in for all its inputs.
    Group(usize),
    /// Neuron `index` of weight layer `layer` (the last layer is the outputs).
    Neuron { layer: usize, index: usize },
}

/// A clickable input-group header.
pub struct Header {
    pub group: usize,
    pub rect: Rect,
}

pub struct Layout {
    /// Every input's node (a collapsed group's inputs sit on its node).
    pub inputs: Vec<Vec2>,
    /// Which group each input is in.
    pub input_group: Vec<usize>,
    /// The node of each bundled group (the rays, and any collapsed group).
    pub groups: Vec<Option<Vec2>>,
    /// Where the rays row's label goes.
    pub rays_label: Vec2,
    pub headers: Vec<Header>,
    /// Expanded inputs of the column (not rays): where their label goes.
    pub labels: Vec<(usize, Vec2)>,
    pub label_size: f32,
    pub input_r: f32,
    pub fan_center: Vec2,
    pub fan_radius: f32,
    /// Neuron positions per weight layer.
    pub neurons: Vec<Vec<Vec2>>,
    pub neuron_r: Vec<f32>,
    /// Centre x of the input nodes and of each layer's column.
    pub input_x: f32,
    pub columns: Vec<f32>,
    /// Room right of the output nodes for labels, bars and sparklines.
    pub output_area: f32,
}

/// The group of each input, from the observation layout.
pub fn input_groups() -> Vec<usize> {
    LAYOUT.iter().enumerate().flat_map(|(g, group)| std::iter::repeat_n(g, group.len)).collect()
}

impl Layout {
    /// `sizes`: neurons per weight layer (hidden layers, then outputs).
    /// `collapsed`: per input group.
    pub fn new(rect: Rect, sizes: &[usize], collapsed: &[bool]) -> Self {
        let input_group = input_groups();
        let input_w = (rect.w * 0.24).clamp(150.0, 300.0);
        let output_area = (rect.w * 0.3).clamp(170.0, 280.0);
        let top = rect.y + HEADER_H;
        let bottom = rect.y + rect.h - 8.0;
        let mut inputs = vec![Vec2::ZERO; input_group.len()];
        let mut groups = vec![None; LAYOUT.len()];
        let mut headers = vec![];
        let mut labels = vec![];

        // Rays: a fan around a mini car, nose up, at the real ray angles.
        let fan_radius = ((input_w - 50.0) / 2.0).min((bottom - top) * 0.13);
        let fan_center = vec2(rect.x + input_w / 2.0, top + 4.0 + fan_radius);
        let input_x = rect.x + input_w - 12.0;
        for (i, angle) in rays::angles().iter().enumerate() {
            // Car frame: +x forward (screen up), +y left (screen left).
            let (s, c) = (*angle as f32).sin_cos();
            inputs[i] = fan_center + fan_radius * vec2(-s, -c);
        }

        // The column: the rays' node, then one labelled block per group.
        let column_top = fan_center.y + fan_radius + 14.0;
        let rows: usize = 1 + LAYOUT
            .iter()
            .enumerate()
            .skip(1)
            .map(|(g, group)| 1 + if collapsed[g] { 0 } else { group.len })
            .sum::<usize>();
        let row_h = ((bottom - column_top) / rows as f32).min(ROW_MAX);
        groups[0] = Some(vec2(input_x, column_top + row_h / 2.0));
        let rays_label = vec2(rect.x + 4.0, column_top + row_h * 0.85);
        let mut y = column_top + row_h;
        let mut index = LAYOUT[0].len;
        for (g, group) in LAYOUT.iter().enumerate().skip(1) {
            let mid = y + row_h / 2.0;
            headers.push(Header { group: g, rect: Rect::new(rect.x + 4.0, y, input_w - 34.0, row_h) });
            y += row_h;
            if collapsed[g] {
                groups[g] = Some(vec2(input_x, mid));
                inputs[index..index + group.len].fill(vec2(input_x, mid));
            } else {
                for (i, at) in inputs.iter_mut().enumerate().skip(index).take(group.len) {
                    *at = vec2(input_x, y + row_h / 2.0);
                    labels.push((i, vec2(rect.x + 16.0, y + row_h * 0.8)));
                    y += row_h;
                }
            }
            index += group.len;
        }
        let input_r = (row_h * 0.38).clamp(1.5, 4.0);

        // Layer columns: evenly spaced from the inputs to the outputs.
        let output_x = rect.x + rect.w - output_area;
        let n = sizes.len();
        let columns: Vec<f32> = (0..n).map(|i| input_x + (output_x - input_x) * (i + 1) as f32 / n as f32).collect();
        let mut neurons = vec![];
        let mut neuron_r = vec![];
        for (i, &size) in sizes.iter().enumerate() {
            let is_output = i == n - 1;
            // Outputs sit in the upper part, leaving room for the
            // contributors list below them.
            let span = if is_output { (bottom - top) * 0.62 } else { bottom - top };
            let spacing = (span / size.max(1) as f32).min(if is_output { 90.0 } else { 28.0 });
            let start = if is_output { top + spacing / 2.0 } else { top + (span - spacing * size as f32) / 2.0 + spacing / 2.0 };
            neurons.push((0..size).map(|k| vec2(columns[i], start + k as f32 * spacing)).collect());
            neuron_r.push((spacing * 0.38).clamp(1.5, if is_output { 9.0 } else { 7.0 }));
        }

        Self {
            inputs,
            input_group,
            groups,
            rays_label,
            headers,
            labels,
            label_size: (row_h + 1.0).min(14.0),
            input_r,
            fan_center,
            fan_radius,
            neurons,
            neuron_r,
            input_x,
            columns,
            output_area,
        }
    }

    pub fn pos(&self, node: Node) -> Vec2 {
        match node {
            Node::Input(i) => self.inputs[i],
            Node::Group(g) => self.groups[g].unwrap_or(Vec2::ZERO),
            Node::Neuron { layer, index } => self.neurons[layer][index],
        }
    }

    /// The node under `p`, if any (the nearest within grabbing distance).
    pub fn hit(&self, p: Vec2) -> Option<Node> {
        let mut best: Option<(f32, Node)> = None;
        let mut consider = |node: Node, at: Vec2, r: f32| {
            let d = at.distance(p);
            if d <= r.max(3.0) + 3.0 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, node));
            }
        };
        for (i, &at) in self.inputs.iter().enumerate() {
            if self.shows_input(i) {
                consider(Node::Input(i), at, self.input_r);
            }
        }
        for (g, at) in self.groups.iter().enumerate() {
            if let Some(at) = at {
                consider(Node::Group(g), *at, 6.0);
            }
        }
        for (layer, column) in self.neurons.iter().enumerate() {
            for (index, &at) in column.iter().enumerate() {
                consider(Node::Neuron { layer, index }, at, self.neuron_r[layer]);
            }
        }
        best.map(|(_, node)| node)
    }

    /// Whether input `i` has its own node (ray nodes in the fan do, though
    /// their edges are bundled).
    pub fn shows_input(&self, i: usize) -> bool {
        let g = self.input_group[i];
        g == 0 || self.groups[g].is_none()
    }

    pub fn header_at(&self, p: Vec2) -> Option<usize> {
        self.headers.iter().find(|h| h.rect.contains(p)).map(|h| h.group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::sensors::observation::INPUTS;

    const SIZES: [usize; 3] = [32, 32, 5];

    fn layout(collapsed: &[bool]) -> Layout {
        Layout::new(Rect::new(700.0, 30.0, 900.0, 870.0), &SIZES, collapsed)
    }

    #[test]
    fn every_node_is_inside_the_rect_and_hits_itself() {
        let l = layout(&[false; LAYOUT.len()]);
        assert_eq!(l.inputs.len(), INPUTS);
        let rect = Rect::new(700.0, 30.0, 900.0, 870.0);
        for i in 0..INPUTS {
            assert!(rect.contains(l.inputs[i]), "input {i} at {:?}", l.inputs[i]);
            assert_eq!(l.hit(l.inputs[i]), Some(Node::Input(i)));
        }
        for (layer, &size) in SIZES.iter().enumerate() {
            for index in 0..size {
                let node = Node::Neuron { layer, index };
                assert!(rect.contains(l.pos(node)));
                assert_eq!(l.hit(l.pos(node)), Some(node));
            }
        }
        assert_eq!(l.hit(vec2(0.0, 0.0)), None);
    }

    #[test]
    fn rays_always_bundle_and_collapsed_groups_become_one_node() {
        let mut collapsed = [false; LAYOUT.len()];
        collapsed[2] = true; // wheels
        let l = layout(&collapsed);
        let rays = l.groups[0].expect("rays are always one node");
        assert_eq!(l.hit(rays), Some(Node::Group(0)));
        let wheels = l.groups[2].unwrap();
        for i in 0..INPUTS {
            match l.input_group[i] {
                2 => assert_eq!(l.inputs[i], wheels),
                // Rays keep their own nodes in the fan.
                _ => assert_eq!(l.hit(l.inputs[i]), Some(Node::Input(i))),
            }
        }
        assert_eq!(l.hit(wheels), Some(Node::Group(2)));
        assert_eq!(l.labels.len(), INPUTS - LAYOUT[0].len - LAYOUT[2].len);
        // Clicking the wheels header finds it.
        let header = l.headers.iter().find(|h| h.group == 2).unwrap();
        assert_eq!(l.header_at(header.rect.center()), Some(2));
    }
}
