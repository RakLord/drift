use std::path::PathBuf;

/// A workspace file such as `config/car.toml`: relative to where the viewer
/// was started if it exists there, otherwise the copy in this workspace.
pub fn workspace_file(relative: &str) -> PathBuf {
    let local = PathBuf::from(relative);
    if local.exists() { local } else { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative) }
}
