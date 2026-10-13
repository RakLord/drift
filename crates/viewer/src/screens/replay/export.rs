//! Recording a replay to video: plays it from the start at exactly one
//! recorded frame per video frame, with the HUD but none of the screen's own
//! UI, and pipes each frame to ffmpeg. The window shows a REC line that the
//! video doesn't.

use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use super::{NORMAL_SPEED, NetView, ReplayScreen};
use crate::render::camera::FollowCamera;
use crate::render::{text, theme};
use crate::video::{self, VideoWriter};

/// The last frame stays up this long, so the run's end can be seen.
const HOLD_SECS: f64 = 1.5;
/// Frames the window size must hold still before recording starts (a
/// tiling window manager resizes a new window just after it opens).
const SETTLE_FRAMES: u32 = 10;

pub(super) struct Export {
    writer: VideoWriter,
    look: Look,
    /// Frames still to show once the replay has ended.
    hold: u32,
    /// Waiting for the window size to settle: the size and how many frames
    /// it has held. Playback waits at the start meanwhile.
    settle: Option<((f32, f32), u32)>,
}

impl Export {
    pub(super) fn settling(&self) -> bool {
        self.settle.is_some()
    }
}

/// How the screen was showing things before the export took over.
struct Look {
    map_view: bool,
    net_view: NetView,
    picker: bool,
    dashboard: bool,
    effects: bool,
    telemetry: bool,
    inputs: bool,
    speed_index: usize,
}

/// Where `R` saves a replay's video: `runs/<run>/videos/<stem>.mp4`, or
/// `videos/` beside the replay when it isn't in a run's replays folder.
pub fn default_path(replay: &Path) -> PathBuf {
    let stem = replay.file_stem().map_or_else(|| "replay".into(), |s| s.to_string_lossy().into_owned());
    let dir = replay.parent().unwrap_or(Path::new("."));
    let base = if dir.file_name().is_some_and(|n| n == "replays") { dir.parent().unwrap_or(dir) } else { dir };
    base.join("videos").join(format!("{stem}.mp4"))
}

impl ReplayScreen {
    /// Record the whole replay to `out`.
    pub fn start_export(&mut self, out: PathBuf) -> Result<(), String> {
        let secs = self.end_tick() * sim::episode::TICK + HOLD_SECS;
        let writer = VideoWriter::new(out, secs)?;
        let look = Look {
            map_view: self.map_view,
            net_view: self.net_view,
            picker: self.picker.open,
            dashboard: self.dashboard.open,
            effects: self.show_effects,
            telemetry: self.show_telemetry,
            inputs: self.show_inputs,
            speed_index: self.speed_index,
        };
        (self.map_view, self.net_view, self.picker.open, self.dashboard.open) = (false, NetView::Off, false, false);
        (self.show_effects, self.show_telemetry, self.show_inputs) = (true, false, false);
        (self.speed_index, self.playing, self.seek) = (NORMAL_SPEED, true, Some(0.0));
        let start = self.replay.recording.frames.first().map_or(self.world.car.state.position, |f| f.position);
        self.camera = FollowCamera::new(start);
        self.score_hud.clear();
        self.toast = None;
        let hold = (HOLD_SECS * f64::from(video::FPS)).round() as u32;
        self.export = Some(Export { writer, look, hold, settle: Some(((0.0, 0.0), 0)) });
        Ok(())
    }

    pub fn exporting(&self) -> bool {
        self.export.is_some()
    }

    /// Esc while recording: stop and throw the partial video away.
    pub fn cancel_export(&mut self) {
        if let Some(export) = self.export.take() {
            export.writer.cancel();
            self.restore(export.look);
            self.end_export(Err("video cancelled".to_owned()));
        }
    }

    /// How the last export ended (saved file and size, or why not), once.
    pub fn take_export_result(&mut self) -> Option<Result<(PathBuf, u64), String>> {
        self.export_result.take()
    }

    /// While recording: send the frame just drawn to the video, then draw
    /// the REC line over it (in the window only).
    pub(super) fn record_frame(&mut self) {
        let (now, end) = (self.tick, self.end_tick());
        let Some(export) = &mut self.export else { return };
        if let Some((size, frames)) = &mut export.settle {
            let now = (screen_width(), screen_height());
            *frames = if now == *size { *frames + 1 } else { 0 };
            *size = now;
            if *frames < SETTLE_FRAMES {
                return;
            }
            export.settle = None;
        }
        if let Err(err) = export.writer.push(&get_screen_data()) {
            let export = self.export.take().expect("exporting");
            self.restore(export.look);
            self.end_export(Err(err));
            return;
        }
        if now >= end {
            export.hold = export.hold.saturating_sub(1);
            if export.hold == 0 {
                let export = self.export.take().expect("exporting");
                let path = export.writer.path().to_owned();
                self.restore(export.look);
                self.end_export(export.writer.finish().map_err(|e| format!("{}: {e}", path.display())));
                return;
            }
        }
        let line = format!(
            "REC  {:.1} / {:.1} s   {}   Esc: cancel",
            now * sim::episode::TICK,
            end * sim::episode::TICK,
            export.writer.path().display()
        );
        draw_rectangle(0.0, 0.0, screen_width(), 34.0, theme::PANEL_SOLID);
        text(&line, 16.0, 23.0, 18.0, theme::BRAKE_LIGHT_ON);
    }

    fn restore(&mut self, look: Look) {
        self.map_view = look.map_view;
        self.net_view = look.net_view;
        self.picker.open = look.picker;
        self.dashboard.open = look.dashboard;
        self.show_effects = look.effects;
        self.show_telemetry = look.telemetry;
        self.show_inputs = look.inputs;
        self.speed_index = look.speed_index;
    }

    fn end_export(&mut self, result: Result<(PathBuf, u64), String>) {
        let message = match &result {
            Ok((path, bytes)) => {
                let mb = *bytes as f64 / 1e6;
                let warning = if *bytes > video::LIMIT_BYTES { "  (over 10 MB!)" } else { "" };
                format!("saved {} ({mb:.1} MB){warning}", path.display())
            }
            Err(err) => err.clone(),
        };
        println!("{message}");
        self.toast = Some((message, 6.0));
        self.export_result = Some(result);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::default_path;

    #[test]
    fn videos_go_beside_the_runs_replays() {
        assert_eq!(default_path(Path::new("runs/run7/replays/g00340_t2.replay")), Path::new("runs/run7/videos/g00340_t2.mp4"));
        assert_eq!(default_path(Path::new("/tmp/x/a.replay")), Path::new("/tmp/x/videos/a.mp4"));
    }
}
