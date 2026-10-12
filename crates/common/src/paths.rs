use std::path::PathBuf;

/// A workspace file or folder such as `config/car.toml`: relative to the
/// current directory if it exists there, otherwise the copy in this
/// workspace (so binaries work from any directory during development).
pub fn workspace_path(relative: &str) -> PathBuf {
    let local = PathBuf::from(relative);
    if local.exists() { local } else { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative) }
}
