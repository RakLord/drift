//! Running trainer processes, found through Linux `/proc`.

use std::path::Path;

/// (pid, run name) for every `train --name <run>` process, however it was
/// started.
pub fn trainers() -> Vec<(u32, String)> {
    let Ok(entries) = std::fs::read_dir("/proc") else { return vec![] };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let cmdline = std::fs::read(e.path().join("cmdline")).ok()?;
            let args: Vec<String> = cmdline.split(|&b| b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
            if Path::new(args.first()?).file_name()? != "train" {
                return None;
            }
            let name = args.iter().position(|a| a == "--name").and_then(|i| args.get(i + 1))?;
            Some((pid, name.clone()))
        })
        .collect()
}

/// Whether a trainer is running for `run`.
pub fn is_training(run: &str) -> bool {
    trainers().iter().any(|(_, name)| name == run)
}
