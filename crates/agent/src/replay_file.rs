//! Replay files: one recorded run, in a small versioned binary format.
//!
//! `DRIFTRPL` magic, a format version, a TOML header (provenance, result,
//! the track, car and scoring settings, frame field names), then binary
//! sections: frames (f32 per field), contacts, score events, zone changes,
//! decisions (the network's inputs), and optionally the genome.
//!
//! Frames are read by field name, so older replays keep loading as fields
//! are added. Decisions and the genome only make sense with the input layout
//! they were recorded with; if that has changed, they're dropped on load (the
//! car still plays, without the network view).

use std::fmt;
use std::io::{self, Read, Write};
use std::path::Path;

use common::Vec2;
use serde::{Deserialize, Serialize};
use sim::car::CarParams;
use sim::episode::EpisodeResult;
use sim::frame::Frame;
use sim::recording::{ContactRecord, DecisionRecord, EventRecord, Recording, ZoneRecord};
use sim::scoring::{ScoreEvent, ScoreEventKind, ScoringParams};
use sim::sensors::observation::layout_hash;
use sim::track::TrackDef;
use sim::track::boundary::SegmentKind;
use sim::world::Contact;

use crate::genome_file::{self, SavedGenome};

const MAGIC: &[u8; 8] = b"DRIFTRPL";
const VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayHeader {
    /// Where it came from, e.g. "run5 gen 340 (drive)" or "manual".
    pub title: String,
    /// Run name, generation, island and stage (empty / 0 if not training).
    pub run: String,
    pub generation: u32,
    pub island: u32,
    pub stage: String,
    pub fitness: f64,
    /// Input layout the decisions were recorded with (hex of `layout_hash`).
    pub layout: String,
    pub frame_every: u32,
    pub fields: Vec<String>,
    pub result: EpisodeResult,
    pub track: TrackDef,
    pub car: CarParams,
    pub scoring: ScoringParams,
}

impl ReplayHeader {
    /// A header for a recording of `result` on `track`, with the current
    /// input layout and frame fields.
    pub fn new(title: impl Into<String>, result: EpisodeResult, track: TrackDef, car: CarParams, scoring: ScoringParams) -> Self {
        Self {
            title: title.into(),
            run: String::new(),
            generation: 0,
            island: 0,
            stage: String::new(),
            fitness: 0.0,
            layout: current_layout(),
            frame_every: sim::recording::FRAME_EVERY,
            fields: Frame::names(),
            result,
            track,
            car,
            scoring,
        }
    }
}

pub struct Replay {
    pub header: ReplayHeader,
    pub recording: Recording,
    /// The driver, if saved and still usable (same input layout).
    pub genome: Option<SavedGenome>,
    /// Why the decisions / genome were dropped on load, if they were.
    pub dropped: Option<String>,
}

#[derive(Debug)]
pub enum ReplayFileError {
    Io(io::Error),
    NotAReplay,
    Version(u32),
    Header(String),
    Corrupt(&'static str),
}

impl fmt::Display for ReplayFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::NotAReplay => write!(f, "not a replay file"),
            Self::Version(v) => write!(f, "replay file version {v}, expected {VERSION}"),
            Self::Header(e) => write!(f, "bad replay header: {e}"),
            Self::Corrupt(what) => write!(f, "corrupt replay ({what})"),
        }
    }
}

impl std::error::Error for ReplayFileError {}

impl From<io::Error> for ReplayFileError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

fn current_layout() -> String {
    format!("{:016x}", layout_hash())
}

pub fn write(w: &mut impl Write, replay: &Replay) -> io::Result<()> {
    let header = toml::to_string(&replay.header).map_err(io::Error::other)?;
    w.write_all(MAGIC)?;
    put_u32(w, VERSION)?;
    put_u32(w, header.len() as u32)?;
    w.write_all(header.as_bytes())?;

    let r = &replay.recording;
    put_u64(w, r.ticks)?;
    let columns = replay.header.fields.len();
    put_u32(w, r.frames.len() as u32)?;
    put_u32(w, columns as u32)?;
    for frame in &r.frames {
        let values = frame.values();
        debug_assert_eq!(values.len(), columns, "header fields must be Frame::names()");
        for v in values {
            put_f32(w, v)?;
        }
    }

    put_u32(w, r.contacts.len() as u32)?;
    for c in &r.contacts {
        let k = &c.contact;
        put_u64(w, c.tick)?;
        for v in [k.point.x, k.point.y, k.normal.x, k.normal.y, k.impact_speed, k.scrape_speed] {
            put_f32(w, v)?;
        }
        w.write_all(&[segment_code(k.kind)])?;
    }

    put_u32(w, r.events.len() as u32)?;
    for e in &r.events {
        put_u64(w, e.tick)?;
        let (code, quality) = event_code(e.event.kind);
        w.write_all(&[code])?;
        put_f32(w, quality)?;
        put_f32(w, e.event.points)?;
    }

    put_u32(w, r.zones.len() as u32)?;
    for z in &r.zones {
        put_u64(w, z.tick)?;
        put_u32(w, z.zone)?;
        w.write_all(&[u8::from(z.lit)])?;
    }

    let inputs = r.decisions.first().map_or(0, |d| d.inputs.len());
    put_u32(w, r.decisions.len() as u32)?;
    put_u32(w, inputs as u32)?;
    for d in &r.decisions {
        put_u64(w, d.tick)?;
        for &v in &d.inputs {
            w.write_all(&v.to_le_bytes())?;
        }
    }

    match &replay.genome {
        Some(genome) => {
            w.write_all(&[1])?;
            genome_file::write(w, genome)
        }
        None => w.write_all(&[0]),
    }
}

pub fn read(r: &mut impl Read) -> Result<Replay, ReplayFileError> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(ReplayFileError::NotAReplay);
    }
    let version = get_u32(r)?;
    if version != VERSION {
        return Err(ReplayFileError::Version(version));
    }
    let mut header = vec![0u8; get_u32(r)? as usize];
    r.read_exact(&mut header)?;
    let header = String::from_utf8(header).map_err(|e| ReplayFileError::Header(e.to_string()))?;
    let header: ReplayHeader = toml::from_str(&header).map_err(|e| ReplayFileError::Header(e.to_string()))?;

    let ticks = get_u64(r)?;
    let (count, columns) = (get_u32(r)? as usize, get_u32(r)? as usize);
    if columns != header.fields.len() {
        return Err(ReplayFileError::Corrupt("frame width doesn't match the field list"));
    }
    // Where each of today's fields sits in the file (None = not recorded).
    let source: Vec<Option<usize>> = Frame::names().iter().map(|name| header.fields.iter().position(|f| f == name)).collect();
    let mut row = vec![0f64; columns];
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        for v in &mut row {
            *v = get_f32(r)?;
        }
        frames.push(Frame::from_values(source.iter().map(|s| s.map_or(0.0, |i| row[i]))));
    }

    let mut contacts = vec![];
    for _ in 0..get_u32(r)? {
        let tick = get_u64(r)?;
        let mut v = [0.0; 6];
        for x in &mut v {
            *x = get_f32(r)?;
        }
        let kind = segment_kind(get_u8(r)?).ok_or(ReplayFileError::Corrupt("unknown wall kind"))?;
        let contact =
            Contact { point: Vec2::new(v[0], v[1]), normal: Vec2::new(v[2], v[3]), impact_speed: v[4], scrape_speed: v[5], kind };
        contacts.push(ContactRecord { tick, contact });
    }

    let mut events = vec![];
    for _ in 0..get_u32(r)? {
        let tick = get_u64(r)?;
        let code = get_u8(r)?;
        let quality = get_f32(r)?;
        let points = get_f32(r)?;
        let kind = event_kind(code, quality).ok_or(ReplayFileError::Corrupt("unknown score event"))?;
        events.push(EventRecord { tick, event: ScoreEvent { kind, points } });
    }

    let mut zones = vec![];
    for _ in 0..get_u32(r)? {
        zones.push(ZoneRecord { tick: get_u64(r)?, zone: get_u32(r)?, lit: get_u8(r)? != 0 });
    }

    let (count, inputs) = (get_u32(r)? as usize, get_u32(r)? as usize);
    let mut decisions = Vec::with_capacity(count);
    let mut bytes = vec![0u8; inputs * 4];
    for _ in 0..count {
        let tick = get_u64(r)?;
        r.read_exact(&mut bytes)?;
        let inputs = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        decisions.push(DecisionRecord { tick, inputs });
    }

    let mut dropped = None;
    let mut genome = None;
    if get_u8(r)? == 1 {
        match genome_file::read(r) {
            Ok(g) => genome = Some(g),
            Err(e) => dropped = Some(format!("network view off: {e}")),
        }
    }
    if header.layout != current_layout() {
        dropped = Some("network view off: recorded with a different input layout".to_owned());
        genome = None;
        decisions.clear();
    }

    let recording = Recording { frame_every: header.frame_every, frames, ticks, contacts, events, zones, decisions };
    Ok(Replay { header, recording, genome, dropped })
}

pub fn save(path: impl AsRef<Path>, replay: &Replay) -> io::Result<()> {
    if let Some(dir) = path.as_ref().parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    write(&mut file, replay)?;
    file.flush()
}

pub fn load(path: impl AsRef<Path>) -> Result<Replay, ReplayFileError> {
    read(&mut io::BufReader::new(std::fs::File::open(path)?))
}

/// Just the header (cheap: for listing replays).
pub fn load_header(path: impl AsRef<Path>) -> Result<ReplayHeader, ReplayFileError> {
    let mut r = io::BufReader::new(std::fs::File::open(path)?);
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(ReplayFileError::NotAReplay);
    }
    let version = get_u32(&mut r)?;
    if version != VERSION {
        return Err(ReplayFileError::Version(version));
    }
    let mut header = vec![0u8; get_u32(&mut r)? as usize];
    r.read_exact(&mut header)?;
    let header = String::from_utf8(header).map_err(|e| ReplayFileError::Header(e.to_string()))?;
    toml::from_str(&header).map_err(|e| ReplayFileError::Header(e.to_string()))
}

fn segment_code(kind: SegmentKind) -> u8 {
    match kind {
        SegmentKind::HardWall => 0,
        SegmentKind::SoftEdge => 1,
        SegmentKind::Fence => 2,
        SegmentKind::Cap => 3,
    }
}

fn segment_kind(code: u8) -> Option<SegmentKind> {
    Some(match code {
        0 => SegmentKind::HardWall,
        1 => SegmentKind::SoftEdge,
        2 => SegmentKind::Fence,
        3 => SegmentKind::Cap,
        _ => return None,
    })
}

fn event_code(kind: ScoreEventKind) -> (u8, f64) {
    match kind {
        ScoreEventKind::DriftStart => (0, 0.0),
        ScoreEventKind::Clip { quality } => (1, quality),
        ScoreEventKind::ZoneEnter => (2, 0.0),
        ScoreEventKind::Transition => (3, 0.0),
        ScoreEventKind::WallTap => (4, 0.0),
        ScoreEventKind::Bobble => (5, 0.0),
        ScoreEventKind::Banked => (6, 0.0),
        ScoreEventKind::Spin => (7, 0.0),
        ScoreEventKind::HardHit => (8, 0.0),
    }
}

fn event_kind(code: u8, quality: f64) -> Option<ScoreEventKind> {
    Some(match code {
        0 => ScoreEventKind::DriftStart,
        1 => ScoreEventKind::Clip { quality },
        2 => ScoreEventKind::ZoneEnter,
        3 => ScoreEventKind::Transition,
        4 => ScoreEventKind::WallTap,
        5 => ScoreEventKind::Bobble,
        6 => ScoreEventKind::Banked,
        7 => ScoreEventKind::Spin,
        8 => ScoreEventKind::HardHit,
        _ => return None,
    })
}

fn put_u32(w: &mut impl Write, v: u32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn put_u64(w: &mut impl Write, v: u64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn put_f32(w: &mut impl Write, v: f64) -> io::Result<()> {
    w.write_all(&(v as f32).to_le_bytes())
}

fn get_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn get_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn get_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn get_f32(r: &mut impl Read) -> io::Result<f64> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(f64::from(f32::from_le_bytes(b)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use common::Rng;
    use nn::{Activation, LayerSpec, Network};
    use sim::episode::{Episode, EpisodeConfig};
    use sim::track::Track;
    use sim::track::generator::{self, GeneratorParams};

    use super::*;
    use crate::genome_file::GenomeHeader;
    use crate::{NnDriver, driver_spec};

    fn recorded() -> Replay {
        let def = generator::generate(&GeneratorParams::default(), 3).expect("track 3 generates");
        let track = Arc::new(Track::build(def.clone()).expect("track builds"));
        let spec = driver_spec(vec![LayerSpec { size: 8, activation: Activation::Tanh }]);
        let params = spec.random_params(&mut Rng::new(2));
        let config = EpisodeConfig {
            max_secs: 8.0,
            start_speed_kmh: 50.0,
            decision_every: 4,
            sensor_noise: 0.02,
            no_progress_secs: 4.0,
            max_backwards: 20.0,
            max_hard_hits: 3,
            end_on_spin: false,
            clutch_grace_secs: 0.4,
            smoothness: Default::default(),
            steering: Default::default(),
        };
        let (car, scoring) = (CarParams::default(), ScoringParams::default());
        let mut driver = NnDriver::new(Network::new(spec.clone(), params.clone()), &config);
        let episode = Episode::new(track, car.clone(), scoring.clone(), config, 11);
        let (result, recording) = episode.run_recorded(&mut driver);
        let genome = SavedGenome { header: GenomeHeader::new(spec, 4, 1, 12.0, "test"), params };
        Replay { header: ReplayHeader::new("test", result, def, car, scoring), recording, genome: Some(genome), dropped: None }
    }

    fn round_trip(replay: &Replay) -> Replay {
        let mut bytes = vec![];
        write(&mut bytes, replay).unwrap();
        read(&mut bytes.as_slice()).unwrap()
    }

    #[test]
    fn round_trips() {
        let original = recorded();
        let once = round_trip(&original);
        let (a, b) = (&original.recording, &once.recording);
        assert_eq!((a.ticks, a.frames.len(), a.contacts.len(), a.events.len()), (b.ticks, b.frames.len(), b.contacts.len(), b.events.len()));
        assert_eq!((a.zones.clone(), a.decisions.clone()), (b.zones.clone(), b.decisions.clone()));
        assert!(!a.decisions.is_empty() && a.frames.len() > 10);
        // Frames are stored as f32: close, and stable from then on.
        let (fa, fb) = (a.frames[10], b.frames[10]);
        assert!((fa.position.x - fb.position.x).abs() < 1e-3 && (fa.progress - fb.progress).abs() < 1e-3);
        assert_eq!(round_trip(&once).recording.frames, b.frames);
        assert_eq!(once.genome, original.genome);
        assert_eq!(once.header.track.name, original.header.track.name);
        assert_eq!(once.header.result, original.header.result);
        assert!(once.dropped.is_none());
    }

    #[test]
    fn another_input_layout_drops_the_network_but_keeps_the_car() {
        let mut replay = recorded();
        replay.header.layout = "0000000000000000".into();
        let back = round_trip(&replay);
        assert!(back.genome.is_none() && back.recording.decisions.is_empty());
        assert!(back.dropped.is_some());
        assert_eq!(back.recording.frames.len(), replay.recording.frames.len());
    }

    #[test]
    fn refuses_other_files() {
        assert!(matches!(read(&mut b"DRIFTGENxxxxxxxx".as_slice()), Err(ReplayFileError::NotAReplay)));
    }
}
