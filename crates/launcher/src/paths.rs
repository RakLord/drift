//! Where the workspace and its build outputs are.

use std::path::PathBuf;

/// The workspace root (this crate lives in `crates/launcher`).
pub fn root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Cargo's target folder (honours CARGO_TARGET_DIR).
pub fn target() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root().join("target"), PathBuf::from)
}
