//! Building and starting the trainer and viewer in the background, and
//! stopping a trainer.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::paths;

/// Processes this launcher started. Reaped as they exit so they don't
/// linger as zombies (which would still look alive in /proc).
#[derive(Default)]
pub struct Children(Vec<Child>);

impl Children {
    pub fn reap(&mut self) {
        self.0.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }

    /// Start the trainer detached, output appended to `runs/<name>/train.log`.
    pub fn trainer(&mut self, name: &str, args: &[String]) -> Result<(), String> {
        build("train", true)?;
        let dir = paths::root().join("runs").join(name);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let log_path = dir.join("train.log");
        let mut log = OpenOptions::new().create(true).append(true).open(&log_path).map_err(|e| e.to_string())?;
        let _ = writeln!(log, "\n== drift launcher: train {}", args.join(" "));
        let mut child = spawn(&paths::target().join("release/train"), args, log)?;
        // Most setup errors (bad track, run exists) show up straight away.
        std::thread::sleep(Duration::from_millis(700));
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("trainer exited ({status}):\n{}", tail(&log_path, 8)));
        }
        self.0.push(child);
        Ok(())
    }

    /// Open the viewer in its own window; output goes to `target/viewer.log`.
    pub fn viewer(&mut self, args: &[String]) -> Result<(), String> {
        build("viewer", false)?;
        let log = File::create(paths::target().join("viewer.log")).map_err(|e| e.to_string())?;
        let child = spawn(&paths::target().join("debug/viewer"), args, log)?;
        self.0.push(child);
        Ok(())
    }
}

fn build(package: &str, release: bool) -> Result<(), String> {
    println!("  building {package}…");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.current_dir(paths::root()).args(["build", "-q", "-p", package]);
    if release {
        command.arg("--release");
    }
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("cargo build -p {package} failed ({status})")),
        Err(e) => Err(format!("couldn't run cargo: {e}")),
    }
}

/// Run `program` from the workspace root in its own process group, so it
/// keeps going after the launcher (or its terminal) closes.
fn spawn(program: &Path, args: &[String], log: File) -> Result<Child, String> {
    let err = log.try_clone().map_err(|e| e.to_string())?;
    Command::new(program)
        .current_dir(paths::root())
        .args(args)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", program.display()))
}

/// Ask a trainer to stop (SIGTERM).
pub fn stop(pid: u32) -> Result<(), String> {
    match Command::new("kill").arg(pid.to_string()).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("kill {pid} failed ({status})")),
        Err(e) => Err(format!("couldn't run kill: {e}")),
    }
}

/// The last `lines` lines of a file.
pub fn tail(path: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
