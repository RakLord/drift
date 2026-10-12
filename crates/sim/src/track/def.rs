//! The on-disk track format (`tracks/*.toml`).

use std::path::Path;

use common::Vec2;
use common::config::{self, ConfigError};
use serde::{Deserialize, Serialize};

use super::boundary::Side;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackDef {
    pub version: u32,
    pub name: String,
    /// Asphalt width, metres.
    pub width: f64,
    /// Width of the grass verge beyond a soft edge, metres.
    pub verge: f64,
    /// Closed loop, in driving order. Anchor `i` starts the section that runs
    /// to anchor `i + 1` (the last wraps to the first).
    pub anchors: Vec<AnchorDef>,
    /// Judged zones. Positions are section coordinates: anchor index plus a
    /// fraction of the way to the next anchor (e.g. 3.5 = halfway through the
    /// section starting at anchor 3), so zones move with their corner.
    #[serde(default)]
    pub zones: Vec<ZoneDef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum ZoneDef {
    /// Inner clipping point at the track edge, judged on the front bumper.
    Clip { at: f64, side: Side },
    /// Outer zone along the track edge, judged on the rear bumper.
    Outer { from: f64, to: f64, side: Side },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorDef {
    pub pos: [f64; 2],
    /// Outgoing Bezier handle, relative to `pos`. The incoming handle mirrors
    /// it. Omit for an automatic smooth handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<[f64; 2]>,
    /// Edges of the section starting here, relative to the driving direction.
    pub left: EdgeKind,
    pub right: EdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    /// A barrier at the track edge.
    Hard,
    /// A grass verge beyond the track edge, with a fence behind it.
    Soft,
}

impl AnchorDef {
    pub fn position(&self) -> Vec2 {
        Vec2::new(self.pos[0], self.pos[1])
    }
}

impl TrackDef {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        config::load(path)
    }
}
