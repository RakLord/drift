//! Video export: frames piped to the system `ffmpeg` as raw RGBA, encoded to
//! a small H.264 MP4 (720p, 30 fps), sized to stay under 10 MB.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use macroquad::texture::Image;

pub const FPS: u32 = 30;
/// Output height; the width follows the window's aspect.
const HEIGHT: u32 = 720;
/// The encoder's bit budget for the whole file, leaving headroom under 10 MB.
const BUDGET_BYTES: f64 = 9.0e6;
/// No point going above this even for a short clip.
const MAX_BITRATE: f64 = 6.0e6;
pub const LIMIT_BYTES: u64 = 10_000_000;

pub struct VideoWriter {
    out: PathBuf,
    secs: f64,
    ffmpeg: Option<(Child, ChildStdin)>,
    size: (u16, u16),
}

impl VideoWriter {
    /// Get ready to write `secs` of video to `out` (ffmpeg starts with the
    /// first frame, once the frame size is known).
    pub fn new(out: PathBuf, secs: f64) -> Result<Self, String> {
        let found = Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status();
        if !found.is_ok_and(|s| s.success()) {
            return Err("video export needs ffmpeg on the PATH".to_owned());
        }
        if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        Ok(Self { out, secs, ffmpeg: None, size: (0, 0) })
    }

    pub fn path(&self) -> &Path {
        &self.out
    }

    /// Add a frame (as `get_screen_data` returns it: bottom-up RGBA).
    pub fn push(&mut self, frame: &Image) -> Result<(), String> {
        let size = (frame.width, frame.height);
        if self.ffmpeg.is_none() {
            let mut child = Command::new("ffmpeg")
                .args(ffmpeg_args(size.0.into(), size.1.into(), self.secs, &self.out))
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .map_err(|e| format!("ffmpeg: {e}"))?;
            let stdin = child.stdin.take().expect("piped stdin");
            self.ffmpeg = Some((child, stdin));
            self.size = size;
        }
        if size != self.size {
            return Err("the window changed size during the export".to_owned());
        }
        let (_, stdin) = self.ffmpeg.as_mut().expect("started above");
        stdin.write_all(&frame.bytes).map_err(|e| format!("ffmpeg stopped: {e}"))
    }

    /// Finish encoding: the file and its size in bytes.
    pub fn finish(mut self) -> Result<(PathBuf, u64), String> {
        let (mut child, stdin) = self.ffmpeg.take().ok_or("no frames were recorded")?;
        drop(stdin);
        let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
        if !status.success() {
            return Err(format!("ffmpeg failed ({status})"));
        }
        let bytes = std::fs::metadata(&self.out).map_err(|e| format!("{}: {e}", self.out.display()))?.len();
        Ok((std::mem::take(&mut self.out), bytes))
    }

    /// Stop and delete the partial file.
    pub fn cancel(mut self) {
        if let Some((mut child, stdin)) = self.ffmpeg.take() {
            drop(stdin);
            child.kill().ok();
            child.wait().ok();
        }
        std::fs::remove_file(&self.out).ok();
    }
}

impl Drop for VideoWriter {
    fn drop(&mut self) {
        // Dropped mid-export (an error, or the app closing): don't leave a
        // half-written file or a stray ffmpeg.
        if let Some((mut child, stdin)) = self.ffmpeg.take() {
            drop(stdin);
            child.kill().ok();
            child.wait().ok();
            std::fs::remove_file(&self.out).ok();
        }
    }
}

/// Peak bitrate (bits/s) that keeps `secs` of video inside the budget.
fn max_bitrate(secs: f64) -> f64 {
    (BUDGET_BYTES * 8.0 / secs.max(1.0)).min(MAX_BITRATE)
}

fn ffmpeg_args(width: u32, height: u32, secs: f64, out: &Path) -> Vec<String> {
    let rate = max_bitrate(secs).round();
    let args = [
        "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{width}x{height}"), "-r",
        &FPS.to_string(), "-i", "-", "-vf", &format!("vflip,scale=-2:{HEIGHT}:flags=lanczos"), "-c:v", "libx264",
        "-preset", "slow", "-crf", "26", "-maxrate", &format!("{rate}"), "-bufsize", &format!("{}", rate * 2.0),
        "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an",
    ];
    let mut args: Vec<String> = args.iter().map(|&a| a.to_owned()).collect();
    args.push(out.to_string_lossy().into_owned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_keeps_long_videos_under_budget_and_short_ones_capped() {
        // A minute of video: 9 MB * 8 / 60 s = 1.2 Mbit/s.
        assert!((max_bitrate(60.0) - 1.2e6).abs() < 1.0);
        assert_eq!(max_bitrate(2.0), MAX_BITRATE);
        assert_eq!(max_bitrate(0.0), MAX_BITRATE);
    }

    #[test]
    fn args_describe_the_frames_and_flip_them() {
        let args = ffmpeg_args(1600, 900, 60.0, Path::new("out.mp4"));
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(after("-s"), "1600x900");
        assert_eq!(after("-r"), "30");
        assert!(after("-vf").starts_with("vflip,"));
        assert_eq!(after("-maxrate"), "1200000");
        assert_eq!(args.last().unwrap(), "out.mp4");
    }
}
