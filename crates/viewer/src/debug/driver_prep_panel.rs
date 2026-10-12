//! The car's driver prep: the next corners as it sees them.

use macroquad::prelude::*;
use sim::sensors::driver_prep::{self, CornerNote};
use sim::World;

use crate::render::{text, theme};

pub fn draw(world: &World, view_w: f32) {
    let notes = driver_prep::upcoming(&world.track, world.place.distance);
    let (w, h) = (view_w, screen_height());
    let (x, y, width) = ((w - 420.0).max(16.0), h - 260.0, 404.0);
    draw_rectangle(x, y, width, 94.0, theme::PANEL);
    text("DRIVER PREP", x + 12.0, y + 22.0, 20.0, theme::NEON_CYAN);
    for (i, note) in notes.iter().enumerate() {
        let line = note.map_or_else(|| "-".to_owned(), |n| describe(&n));
        let color = if i == 0 { theme::TEXT } else { theme::TEXT_DIM };
        text(&line, x + 12.0, y + 44.0 + i as f32 * 18.0, 16.0, color);
    }
}

fn describe(n: &CornerNote) -> String {
    let dir = if n.direction > 0.0 { "L" } else { "R" };
    let when = if n.distance_to > 0.0 {
        format!("in {:>3.0} m", n.distance_to)
    } else {
        format!("now {:>3.0}%", n.progress * 100.0)
    };
    let mut extras = vec![];
    if let Some(at) = n.clip {
        extras.push(format!("clip@{:.0}%", at * 100.0));
    }
    if n.outer_coverage > 0.0 {
        extras.push(format!("zone {:.0}%", n.outer_coverage * 100.0));
    }
    if n.outside_hard {
        extras.push("wall out".to_owned());
    }
    if n.inside_hard {
        extras.push("wall in".to_owned());
    }
    format!("{dir} {:>3.0}° r{:<3.0} {when}  {}", n.angle.to_degrees(), n.min_radius, extras.join(" "))
}
