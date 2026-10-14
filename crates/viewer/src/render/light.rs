//! Light as additive sprites: street-lamp pools, headlights, tail lights
//! and neon halos are added onto the scene, so they brighten what's under
//! them and never cover it. No render targets: the window's MSAA, video
//! export and the review grid's per-tile viewports keep working.
//!
//! Each world view draws two passes: one on the road before the cars (lamp
//! pools, zone glow, headlight beams), so a pack's beams don't wash out
//! each other's paint, and one over the cars (tail lights, sparks). Draw
//! lights inside [`pass`]; the material and textures are made on first use.

use std::cell::OnceCell;

use common::Vec2;
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;

use super::camera::FollowCamera;
use super::to_mq;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const FRAGMENT: &str = r#"#version 100
varying lowp vec2 uv;
varying lowp vec4 color;

uniform sampler2D Texture;

void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

/// How dark the vignette gets in the corners.
const VIGNETTE_ALPHA: f64 = 0.55;

struct Kit {
    additive: Material,
    radial: Texture2D,
    cone: Texture2D,
    vignette: Texture2D,
}

thread_local! {
    static KIT: OnceCell<Kit> = const { OnceCell::new() };
}

fn with_kit<R>(f: impl FnOnce(&Kit) -> R) -> R {
    KIT.with(|cell| f(cell.get_or_init(make_kit)))
}

fn make_kit() -> Kit {
    let pipeline_params = PipelineParams {
        color_blend: Some(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::One)),
        ..Default::default()
    };
    let additive = load_material(ShaderSource::Glsl { vertex: VERTEX, fragment: FRAGMENT }, MaterialParams { pipeline_params, ..Default::default() })
        .expect("the additive light shader compiles");
    Kit { additive, radial: radial_texture(), cone: cone_texture(), vignette: vignette_texture() }
}

/// White, fading smoothly from the middle to nothing at the edge.
fn radial_texture() -> Texture2D {
    const SIZE: u16 = 128;
    texture(SIZE, SIZE, |px, py| {
        let d = (px * px + py * py).sqrt();
        (1.0 - d).max(0.0).powi(2)
    })
}

/// A beam from the middle of the left edge spreading to the right: bright
/// near the source, fading with distance and towards its sides.
fn cone_texture() -> Texture2D {
    const W: u16 = 128;
    const H: u16 = 64;
    texture(W, H, |px, py| {
        // px, py in -1..1; along = 0 at the source, 1 at the far end.
        let along = (px + 1.0) / 2.0;
        let half_width = 0.15 + 0.85 * along;
        let side = (py.abs() / half_width).min(1.0);
        let reach = (1.0 - along).powf(1.4);
        let soft_start = (along * 8.0).min(1.0);
        (1.0 - side * side) * reach * soft_start
    })
}

/// Clear in the middle, darkening towards the edges and corners.
fn vignette_texture() -> Texture2D {
    const SIZE: u16 = 128;
    texture(SIZE, SIZE, |px, py| {
        let d = (px * px + py * py).sqrt() / std::f64::consts::SQRT_2;
        ((d - 0.45) / 0.55).clamp(0.0, 1.0).powf(1.6) * VIGNETTE_ALPHA
    })
}

/// A white texture whose alpha is `alpha(px, py)`, both -1..1.
fn texture(w: u16, h: u16, alpha: impl Fn(f64, f64) -> f64) -> Texture2D {
    let mut image = Image::gen_image_color(w, h, Color::new(1.0, 1.0, 1.0, 0.0));
    for y in 0..u32::from(h) {
        for x in 0..u32::from(w) {
            let px = (f64::from(x) + 0.5) / f64::from(w) * 2.0 - 1.0;
            let py = (f64::from(y) + 0.5) / f64::from(h) * 2.0 - 1.0;
            image.set_pixel(x, y, Color::new(1.0, 1.0, 1.0, alpha(px, py).clamp(0.0, 1.0) as f32));
        }
    }
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Draw lights: everything inside `draw` is added onto the scene. Works
/// in world space (under a camera) and screen space alike.
pub fn pass(draw: impl FnOnce()) {
    with_kit(|kit| gl_use_material(&kit.additive));
    draw();
    gl_use_default_material();
}

/// A round glow `radius` metres (or pixels) across its half-width.
pub fn glow(at: Vec2, radius: f64, colour: Color) {
    let r = radius as f32;
    let p = to_mq(at);
    with_kit(|kit| {
        draw_texture_ex(&kit.radial, p.x - r, p.y - r, colour, DrawTextureParams { dest_size: Some(vec2(2.0 * r, 2.0 * r)), ..Default::default() });
    });
}

/// A beam from `origin` along `heading`, `length` long and `width` wide at
/// its far end.
pub fn cone(origin: Vec2, heading: f64, length: f64, width: f64, colour: Color) {
    let p = to_mq(origin);
    let (l, w) = (length as f32, width as f32);
    with_kit(|kit| {
        let params = DrawTextureParams { dest_size: Some(vec2(l, w)), rotation: heading as f32, pivot: Some(p), ..Default::default() };
        draw_texture_ex(&kit.cone, p.x, p.y - w / 2.0, colour, params);
    });
}

/// Whether a light reaching `reach` around `at` can show on screen.
pub fn in_view(camera: &FollowCamera, at: Vec2, reach: f64) -> bool {
    let (min, max) = camera.visible();
    at.x + reach >= min.x && at.x - reach <= max.x && at.y + reach >= min.y && at.y - reach <= max.y
}

/// Darken the edges of a screen area `w` x `h` from the top-left (screen
/// space, normal blending; not inside `pass`).
pub fn vignette(w: f32, h: f32) {
    with_kit(|kit| {
        draw_texture_ex(&kit.vignette, 0.0, 0.0, BLACK, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
    });
}
