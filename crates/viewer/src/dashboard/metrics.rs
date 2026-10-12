//! A training run's metrics.csv, re-read whenever the trainer appends to it.

use std::path::PathBuf;
use std::time::SystemTime;

const POLL_SECS: f64 = 1.0;

pub struct Metrics {
    pub path: PathBuf,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    modified: Option<SystemTime>,
    timer: f64,
}

impl Metrics {
    pub fn new(path: PathBuf) -> Self {
        let mut m = Self { path, columns: vec![], rows: vec![], modified: None, timer: POLL_SECS };
        m.poll(POLL_SECS);
        m
    }

    /// Re-read the file if it changed (checked about once a second).
    pub fn poll(&mut self, dt: f64) {
        self.timer += dt;
        if self.timer < POLL_SECS {
            return;
        }
        self.timer = 0.0;
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified == self.modified {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&self.path) else { return };
        self.modified = modified;
        let mut lines = text.lines();
        self.columns = lines.next().unwrap_or("").split(',').map(str::to_owned).collect();
        // A half-written last line (mid-append) is skipped until next time.
        self.rows = lines
            .map(|l| l.split(',').map(str::to_owned).collect::<Vec<_>>())
            .filter(|r| r.len() == self.columns.len())
            .collect();
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    fn index(&self, column: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == column)
    }

    /// A numeric column (missing or blank values are NaN).
    pub fn series(&self, column: &str) -> Vec<f64> {
        let Some(i) = self.index(column) else { return vec![f64::NAN; self.rows.len()] };
        self.rows.iter().map(|r| r[i].parse().unwrap_or(f64::NAN)).collect()
    }

    pub fn text(&self, column: &str, row: usize) -> &str {
        self.index(column).and_then(|i| self.rows.get(row).map(|r| r[i].as_str())).unwrap_or("")
    }

    /// Rows where the curriculum stage changed, with the new stage's name.
    pub fn stage_changes(&self) -> Vec<(usize, String)> {
        let Some(i) = self.index("stage") else { return vec![] };
        let mut out = vec![];
        for (k, row) in self.rows.iter().enumerate() {
            if k == 0 || row[i] != self.rows[k - 1][i] {
                out.push((k, row[i].clone()));
            }
        }
        out
    }
}
