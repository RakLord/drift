//! Who's on the review grid, which of them are picked, and what became of
//! the picks sent to the trainer (`agent::picks`).

use std::path::Path;

use agent::picks::{self, State};
use agent::top_file::{TopEntry, TopGenomes};

/// Tiles on a full grid.
pub const TILES: usize = 12;
/// Of those, the fittest; the rest are drifters (as top.genomes marks them).
const FITTEST: usize = 8;

/// The entries to show, as indices: the fittest by rank, then the drifters.
/// Either side fills in when the other is short.
pub fn choose(entries: &[TopEntry]) -> Vec<usize> {
    let mut fittest: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].drifter).collect();
    fittest.sort_by_key(|&i| entries[i].rank);
    let drifters: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].drifter).collect();
    let take_fit = fittest.len().min(FITTEST.max(TILES.saturating_sub(drifters.len())));
    let take_drift = drifters.len().min(TILES - take_fit);
    fittest.into_iter().take(take_fit).chain(drifters.into_iter().take(take_drift)).collect()
}

/// One send to the trainer.
struct Sent {
    file: String,
    ids: Vec<u64>,
    state: State,
}

#[derive(Default)]
pub struct Picks {
    /// Genome ids picked and not sent yet.
    pub picked: Vec<u64>,
    sent: Vec<Sent>,
}

impl Picks {
    pub fn toggle(&mut self, id: u64) {
        match self.picked.iter().position(|&p| p == id) {
            Some(i) => _ = self.picked.remove(i),
            None => self.picked.push(id),
        }
    }

    pub fn is_picked(&self, id: u64) -> bool {
        self.picked.contains(&id)
    }

    /// The latest state of a sent genome, if it was sent.
    pub fn sent_state(&self, id: u64) -> Option<State> {
        self.sent.iter().rev().find(|s| s.ids.contains(&id)).map(|s| s.state)
    }

    /// Send the picks (with their genomes from `top`) to the trainer.
    pub fn send(&mut self, run_dir: &Path, top: &TopGenomes) -> Result<String, String> {
        if self.picked.is_empty() {
            return Err("nothing picked: P or right-click a car first".to_owned());
        }
        let file = picks::send(run_dir, &picks::subset(top, &self.picked)).map_err(|e| format!("sending picks: {e}"))?;
        let ids = std::mem::take(&mut self.picked);
        let message = format!("sent {}", list(&ids));
        self.sent.push(Sent { file, ids, state: State::Waiting });
        Ok(message)
    }

    /// Re-read where each send is (it moves when the trainer applies it).
    pub fn poll(&mut self, run_dir: &Path) {
        for s in self.sent.iter_mut().filter(|s| s.state == State::Waiting) {
            s.state = picks::state(run_dir, &s.file);
        }
    }

    /// The latest send, for the status line.
    pub fn latest(&self) -> Option<String> {
        let s = self.sent.last()?;
        let what = list(&s.ids);
        Some(match s.state {
            State::Waiting => format!("{what}: waiting for the trainer"),
            State::Applied(generation) => format!("{what}: applied at gen {generation}"),
            State::Rejected => format!("{what}: the trainer couldn't read it"),
            State::Gone => format!("{what}: file gone"),
        })
    }
}

fn list(ids: &[u64]) -> String {
    ids.iter().map(|id| format!("#{id}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(fit: u32, drift: u32) -> Vec<TopEntry> {
        let entry = |i: u32, drifter| TopEntry {
            id: u64::from(i),
            island: 0,
            rank: 100 - i,
            fitness: 0.0,
            drift_quality: 0.0,
            drifter,
            born: 0,
            parents: vec![],
            sigma: 0.0,
            objectives: vec![],
            training: None,
        };
        (0..fit).map(|i| entry(i, false)).chain((fit..fit + drift).map(|i| entry(i, true))).collect()
    }

    #[test]
    fn eight_fittest_by_rank_then_four_drifters() {
        let e = entries(20, 10);
        let chosen = choose(&e);
        assert_eq!(chosen.len(), 12);
        // Rank is 100 - i, so the fittest are the highest indices.
        assert_eq!(chosen[..8], (12..20).rev().collect::<Vec<_>>()[..]);
        assert_eq!(chosen[8..], (20..24).collect::<Vec<_>>()[..]);
    }

    #[test]
    fn a_short_side_is_filled_by_the_other() {
        let few_drifters = choose(&entries(20, 2));
        assert_eq!(few_drifters.len(), 12);
        assert_eq!(few_drifters.iter().filter(|&&i| i >= 20).count(), 2, "10 fittest, then both drifters");
        assert_eq!(choose(&entries(5, 30)).iter().filter(|&&i| i >= 5).count(), 7, "all 5 fittest, then 7 drifters");
        assert_eq!(choose(&entries(3, 4)).len(), 7);
    }

    #[test]
    fn picking_toggles() {
        let mut p = Picks::default();
        p.toggle(4);
        p.toggle(9);
        p.toggle(4);
        assert_eq!(p.picked, [9]);
        assert!(p.is_picked(9) && !p.is_picked(4));
    }
}
