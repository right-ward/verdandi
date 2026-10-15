//! Native SFZ parsing and sample playback.
//!
//! This first implementation supports common region, key/velocity, tuning, envelope,
//! random-region and looping opcodes. Sample data is loaded before rendering; the audio
//! callback only reads prepared buffers and advances bounded voice state.
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_VOICES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfzLoopMode {
    NoLoop,
    Continuous,
    Sustain,
    OneShot,
}

#[derive(Debug, Clone)]
pub struct SfzError {
    path: Option<PathBuf>,
    line: Option<usize>,
    message: String,
}

impl SfzError {
    fn message(message: impl Into<String>) -> Self {
        Self { path: None, line: None, message: message.into() }
    }

    fn at_line(line: usize, message: impl Into<String>) -> Self {
        Self { path: None, line: Some(line), message: message.into() }
    }

    fn at_path(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self { path: Some(path.into()), line: None, message: message.into() }
    }

    fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }
}

impl fmt::Display for SfzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}: ", path.display())?;
        }
        if let Some(line) = self.line {
            write!(f, "line {line}: ")?;
        }
        f.write_str(&self.message)
    }
}

impl Error for SfzError {}

#[derive(Clone, Debug)]
pub struct SfzRegion {
    /// Sample path as written in the SFZ mapping, before filesystem resolution.
    pub sample: PathBuf,
    pub low_key: u8,
    pub high_key: u8,
    pub pitch_keycenter: u8,
    pub low_velocity: u8,
    pub high_velocity: u8,
    /// SFZ random range in the normalized domain [0, 1].
    pub low_random: f32,
    pub high_random: f32,
    pub volume_db: f32,
    /// Fine tuning in cents.
    pub tune_cents: f32,
    pub transpose_semitones: f32,
    pub amp_veltrack: f32,
    pub loop_mode: SfzLoopMode,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
    pub sample_offset: usize,
    pub sample_end: Option<usize>,
    pub attack_seconds: f32,
    pub decay_seconds: f32,
    /// Linear sustain level; SFZ ampeg_sustain is expressed as a percentage.
    pub sustain_level: f32,
    pub release_seconds: f32,
}

#[derive(Clone, Debug)]
pub struct SfzDefinition {
    default_path: Option<PathBuf>,
    regions: Vec<SfzRegion>,
    unsupported_opcodes: Vec<String>,
}

impl SfzDefinition {
    pub fn regions(&self) -> &[SfzRegion] {
        &self.regions
    }

    pub fn default_path(&self) -> Option<&Path> {
        self.default_path.as_deref()
    }

    /// OpCodes or section types seen but not implemented. These are ignored, so inspect this
    /// list before relying on a patch whose sound depends on any listed feature.
    pub fn unsupported_opcodes(&self) -> &[String] {
        &self.unsupported_opcodes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Control,
    Global,
    Group,
    Region,
    Ignored,
}

const SUPPORTED_OPCODES: &[&str] = &[
    "sample", "key", "lokey", "hikey", "pitch_keycenter", "lovel", "hivel",
    "lorand", "hirand", "volume", "tune", "transpose", "amp_veltrack",
    "loop_mode", "loop_start", "loop_end", "offset", "end", "ampeg_attack",
    "ampeg_decay", "ampeg_sustain", "ampeg_release", "default_path",
];

/// Parse SFZ text without loading the referenced sample files.
pub fn parse_sfz(source: &str) -> Result<SfzDefinition, SfzError> {
    let mut section = Section::Global;
    let mut control: HashMap<String, String> = HashMap::new();
    let mut global: HashMap<String, String> = HashMap::new();
    let mut group: HashMap<String, String> = HashMap::new();
    let mut current_region: Option<(HashMap<String, String>, usize)> = None;
    let mut regions = Vec::new();
    let mut unsupported = BTreeSet::new();

    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut remaining = line;
        loop {
            if let Some(tag_start) = remaining.find('<') {
                let prefix = remaining[..tag_start].trim();
                apply_assignments(
                    prefix, line_number, section, &mut control, &mut global, &mut group,
                    &mut current_region, &mut unsupported,
                );
                let after_open = &remaining[tag_start + 1..];
                let Some(tag_end) = after_open.find('>') else {
                    return Err(SfzError::at_line(line_number, "unterminated section tag"));
                };
                let tag = after_open[..tag_end].trim().to_ascii_lowercase();

                if let Some((map, region_line)) = current_region.take() {
                    regions.push(build_region(map, region_line)?);
                }

                match tag.as_str() {
                    "control" => section = Section::Control,
                    "global" => {
                        global.clear();
                        group.clear();
                        section = Section::Global;
                    }
                    "group" => {
                        group.clear();
                        section = Section::Group;
                    }
                    "region" => {
                        let mut map = global.clone();
                        map.extend(group.clone());
                        current_region = Some((map, line_number));
                        section = Section::Region;
                    }
                    "master" | "curve" | "effect" | "effect1" | "effect2" | "midi" => {
                        unsupported.insert(format!("section:{tag}"));
                        section = Section::Ignored;
                    }
                    other => {
                        unsupported.insert(format!("section:{other}"));
                        section = Section::Ignored;
                    }
                }
                remaining = &after_open[tag_end + 1..];
                if remaining.trim().is_empty() {
                    break;
                }
            } else {
                apply_assignments(
                    remaining.trim(), line_number, section, &mut control, &mut global, &mut group,
                    &mut current_region, &mut unsupported,
                );
                break;
            }
        }
    }

    if let Some((map, region_line)) = current_region.take() {
        regions.push(build_region(map, region_line)?);
    }
    if regions.is_empty() {
        return Err(SfzError::message("SFZ contains no <region> entries"));
    }

    let default_path = control.get("default_path").map(|value| normalize_path(value));
    Ok(SfzDefinition {
        default_path,
        regions,
        unsupported_opcodes: unsupported.into_iter().collect(),
    })
}

fn apply_assignments(
    text: &str,
    line: usize,
    section: Section,
    control: &mut HashMap<String, String>,
    global: &mut HashMap<String, String>,
    group: &mut HashMap<String, String>,
    current_region: &mut Option<(HashMap<String, String>, usize)>,
    unsupported: &mut BTreeSet<String>,
) {
    for token in tokenize(text) {
        let Some((key, value)) = token.split_once('=') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
        if !SUPPORTED_OPCODES.contains(&key.as_str()) {
            unsupported.insert(key.clone());
        }
        if key == "loop_mode" && !matches!(
            value.to_ascii_lowercase().as_str(),
            "no_loop" | "loop_continuous" | "loop_sustain" | "one_shot"
        ) {
            unsupported.insert(format!("loop_mode={value}"));
        }
        match section {
            Section::Control => { control.insert(key, value); }
            Section::Global => { global.insert(key, value); }
            Section::Group => { group.insert(key, value); }
            Section::Region => {
                if let Some((map, _)) = current_region.as_mut() {
                    map.insert(key, value);
                } else {
                    unsupported.insert(format!("orphan_assignment_line_{line}"));
                }
            }
            Section::Ignored => {}
        }
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for ch in text.chars() {
        if ch == '"' {
            quoted = !quoted;
            token.push(ch);
        } else if ch.is_whitespace() && !quoted {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() { tokens.push(token); }
    tokens
}

fn build_region(map: HashMap<String, String>, line: usize) -> Result<SfzRegion, SfzError> {
    let sample = map.get("sample").ok_or_else(|| SfzError::at_line(line, "region has no sample= path"))?;
    let key = optional_u8(&map, "key", line)?;
    let low_key = optional_u8(&map, "lokey", line)?.or(key).unwrap_or(0);
    let high_key = optional_u8(&map, "hikey", line)?.or(key).unwrap_or(127);
    if low_key > high_key {
        return Err(SfzError::at_line(line, "lokey must not exceed hikey"));
    }
    let pitch_keycenter = optional_u8(&map, "pitch_keycenter", line)?
        .or(key)
        .unwrap_or(((u16::from(low_key) + u16::from(high_key)) / 2) as u8);
    let low_velocity = optional_u8(&map, "lovel", line)?.unwrap_or(0);
    let high_velocity = optional_u8(&map, "hivel", line)?.unwrap_or(127);
    if low_velocity > high_velocity {
        return Err(SfzError::at_line(line, "lovel must not exceed hivel"));
    }
    let low_random = optional_f32(&map, "lorand", line)?.unwrap_or(0.0).clamp(0.0, 1.0);
    let high_random = optional_f32(&map, "hirand", line)?.unwrap_or(1.0).clamp(0.0, 1.0);
    if low_random >= high_random {
        return Err(SfzError::at_line(line, "lorand must be less than hirand"));
    }
    let loop_mode = match map.get("loop_mode").map(|v| v.to_ascii_lowercase()).as_deref() {
        None | Some("no_loop") => SfzLoopMode::NoLoop,
        Some("loop_continuous") => SfzLoopMode::Continuous,
        Some("loop_sustain") => SfzLoopMode::Sustain,
        Some("one_shot") => SfzLoopMode::OneShot,
        Some(_) => SfzLoopMode::NoLoop,
    };
    let sustain_percent = optional_f32(&map, "ampeg_sustain", line)?.unwrap_or(100.0);
    let attack_seconds = optional_f32(&map, "ampeg_attack", line)?.unwrap_or(0.0);
    let decay_seconds = optional_f32(&map, "ampeg_decay", line)?.unwrap_or(0.0);
    let release_seconds = optional_f32(&map, "ampeg_release", line)?.unwrap_or(0.0);
    for (name, value) in [
        ("ampeg_attack", attack_seconds),
        ("ampeg_decay", decay_seconds),
        ("ampeg_release", release_seconds),
    ] {
        if value < 0.0 {
            return Err(SfzError::at_line(line, format!("{name} cannot be negative")));
        }
    }
    Ok(SfzRegion {
        sample: normalize_path(sample),
        low_key, high_key, pitch_keycenter, low_velocity, high_velocity,
        low_random, high_random,
        volume_db: optional_f32(&map, "volume", line)?.unwrap_or(0.0),
        tune_cents: optional_f32(&map, "tune", line)?.unwrap_or(0.0),
        transpose_semitones: optional_f32(&map, "transpose", line)?.unwrap_or(0.0),
        amp_veltrack: optional_f32(&map, "amp_veltrack", line)?.unwrap_or(100.0).max(0.0),
        loop_mode,
        loop_start: optional_usize(&map, "loop_start", line)?,
        loop_end: optional_usize(&map, "loop_end", line)?,
        sample_offset: optional_usize(&map, "offset", line)?.unwrap_or(0),
        sample_end: optional_usize(&map, "end", line)?,
        attack_seconds, decay_seconds,
        sustain_level: (sustain_percent / 100.0).clamp(0.0, 1.0),
        release_seconds,
    })
}

fn optional_u8(map: &HashMap<String, String>, key: &str, line: usize) -> Result<Option<u8>, SfzError> {
    map.get(key).map(|value| value.parse::<u8>()
        .map_err(|_| SfzError::at_line(line, format!("invalid {key} value {value:?}")))).transpose()
}

fn optional_usize(map: &HashMap<String, String>, key: &str, line: usize) -> Result<Option<usize>, SfzError> {
    map.get(key).map(|value| value.parse::<usize>()
        .map_err(|_| SfzError::at_line(line, format!("invalid {key} value {value:?}")))).transpose()
}

fn optional_f32(map: &HashMap<String, String>, key: &str, line: usize) -> Result<Option<f32>, SfzError> {
    map.get(key).map(|value| {
        let parsed = value.parse::<f32>().map_err(|_| SfzError::at_line(line, format!("invalid {key} value {value:?}")))?;
        if !parsed.is_finite() {
            return Err(SfzError::at_line(line, format!("{key} must be finite")));
        }
        Ok(parsed)
    }).transpose()
}

fn normalize_path(path: &str) -> PathBuf {
    PathBuf::from(path.replace('\\', "/"))
}

#[derive(Debug)]
struct SampleData {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
    frames: usize,
}

#[derive(Debug)]
pub struct SfzInstrument {
    path: PathBuf,
    definition: SfzDefinition,
    samples: Vec<SampleData>,
    sample_for_region: Vec<usize>,
}

impl SfzInstrument {
    /// Load an SFZ patch and every referenced WAV sample. File access and decoding happen here,
    /// before the instrument is attached to an audio engine.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SfzError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|error| SfzError::at_path(path, format!("could not read SFZ text: {error}")))?;
        let definition = parse_sfz(&text).map_err(|error| error.with_path(path))?;
        let patch_dir = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let sample_root = definition.default_path().map(|default| patch_dir.join(default)).unwrap_or_else(|| patch_dir.to_path_buf());

        let mut samples = Vec::new();
        let mut paths_to_sample = HashMap::<PathBuf, usize>::new();
        let mut sample_for_region = Vec::with_capacity(definition.regions.len());
        for region in definition.regions() {
            let sample_path = if region.sample.is_absolute() {
                region.sample.clone()
            } else {
                sample_root.join(&region.sample)
            };
            let sample_path = lexical_normalize(&sample_path);
            let sample_index = if let Some(index) = paths_to_sample.get(&sample_path) {
                *index
            } else {
                let data = decode_wav(&sample_path)?;
                let index = samples.len();
                samples.push(data);
                paths_to_sample.insert(sample_path.clone(), index);
                index
            };
            let sample = &samples[sample_index];
            let end = region.sample_end.unwrap_or(sample.frames);
            if region.sample_offset >= end || end > sample.frames {
                return Err(SfzError::at_path(&sample_path, format!(
                    "invalid sample range offset={} end={} (file has {} frames)",
                    region.sample_offset, end, sample.frames
                )));
            }
            if let (Some(start), Some(loop_end)) = (region.loop_start, region.loop_end) {
                if start < region.sample_offset || start >= loop_end || loop_end > end {
                    return Err(SfzError::at_path(&sample_path, format!(
                        "invalid loop range start={start} end={loop_end} (sample range {}..{end})",
                        region.sample_offset
                    )));
                }
            } else if region.loop_start.is_some() || region.loop_end.is_some() {
                return Err(SfzError::at_path(&sample_path, "loop_start and loop_end must both be set"));
            }
            sample_for_region.push(sample_index);
        }
        Ok(Self { path: path.to_path_buf(), definition, samples, sample_for_region })
    }

    pub fn path(&self) -> &Path { &self.path }
    pub fn regions(&self) -> &[SfzRegion] { self.definition.regions() }
    pub fn region_count(&self) -> usize { self.definition.regions.len() }
    pub fn unsupported_opcodes(&self) -> &[String] { self.definition.unsupported_opcodes() }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => { out.pop(); }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn decode_wav(path: &Path) -> Result<SampleData, SfzError> {
    let bytes = fs::read(path).map_err(|error| SfzError::at_path(path, format!("could not read sample: {error}")))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SfzError::at_path(path, "unsupported sample format: expected RIFF/WAVE; FLAC and other codecs are not supported yet"));
    }
    let mut format = None::<u16>;
    let mut channels = None::<usize>;
    let mut sample_rate = None::<u32>;
    let mut bits = None::<u16>;
    let mut audio_data = None::<&[u8]>;
    let mut cursor = 12usize;
    while cursor.checked_add(8).filter(|end| *end <= bytes.len()).is_some() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let start = cursor + 8;
        let Some(end) = start.checked_add(chunk_size) else {
            return Err(SfzError::at_path(path, "WAV chunk size overflow"));
        };
        if end > bytes.len() {
            return Err(SfzError::at_path(path, "truncated WAV chunk"));
        }
        match chunk_id {
            b"fmt " if chunk_size >= 16 => {
                format = Some(u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()));
                channels = Some(u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()) as usize);
                sample_rate = Some(u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()));
                bits = Some(u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap()));
            }
            b"data" => audio_data = Some(&bytes[start..end]),
            _ => {}
        }
        cursor = end.saturating_add(chunk_size & 1);
    }

    let format = format.ok_or_else(|| SfzError::at_path(path, "WAV file is missing a valid fmt chunk"))?;
    let channels = channels.ok_or_else(|| SfzError::at_path(path, "WAV channel count is missing"))?;
    let sample_rate = sample_rate.ok_or_else(|| SfzError::at_path(path, "WAV sample rate is missing"))?;
    let bits = bits.ok_or_else(|| SfzError::at_path(path, "WAV bit depth is missing"))?;
    let audio_data = audio_data.ok_or_else(|| SfzError::at_path(path, "WAV file is missing a data chunk"))?;
    if !(1..=2).contains(&channels) || sample_rate == 0 {
        return Err(SfzError::at_path(path, format!("unsupported WAV layout: {channels} channel(s), {sample_rate} Hz")));
    }
    if !matches!((format, bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64)) {
        return Err(SfzError::at_path(path, format!("unsupported WAV encoding: format {format}, {bits} bits (supports PCM 8/16/24/32-bit and IEEE float 32/64-bit)")));
    }
    let bytes_per_sample = usize::from(bits / 8);
    let frame_bytes = channels.checked_mul(bytes_per_sample).ok_or_else(|| SfzError::at_path(path, "WAV frame size overflow"))?;
    if frame_bytes == 0 || audio_data.len() % frame_bytes != 0 {
        return Err(SfzError::at_path(path, "WAV data length is not aligned to complete audio frames"));
    }
    let mut samples = Vec::with_capacity(audio_data.len() / bytes_per_sample);
    for raw in audio_data.chunks_exact(bytes_per_sample) {
        let value = match (format, bits) {
            (1, 8) => (f32::from(raw[0]) - 128.0) / 128.0,
            (1, 16) => i16::from_le_bytes(raw.try_into().unwrap()) as f32 / 32768.0,
            (1, 24) => {
                let mut value = i32::from(raw[0]) | (i32::from(raw[1]) << 8) | (i32::from(raw[2]) << 16);
                if value & 0x0080_0000 != 0 { value |= !0x00ff_ffff; }
                value as f32 / 8_388_608.0
            }
            (1, 32) => i32::from_le_bytes(raw.try_into().unwrap()) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes(raw.try_into().unwrap()),
            (3, 64) => f64::from_le_bytes(raw.try_into().unwrap()) as f32,
            _ => unreachable!(),
        };
        if !value.is_finite() {
            return Err(SfzError::at_path(path, "WAV contains a non-finite sample value"));
        }
        samples.push(value.clamp(-1.0, 1.0));
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return Err(SfzError::at_path(path, "WAV contains no audio frames"));
    }
    Ok(SampleData { samples, channels, sample_rate, frames })
}

#[derive(Debug)]
struct Voice {
    region_index: usize,
    position: f64,
    increment: f64,
    age: u64,
    gate_frames: u64,
    release_frames: u64,
    velocity_gain: f32,
    region_gain: f32,
}

#[derive(Debug)]
pub struct SfzSampler {
    instrument: Arc<SfzInstrument>,
    sample_rate: u32,
    voices: Vec<Voice>,
    random_state: u64,
    gain: f32,
}

impl SfzSampler {
    /// Construct a realtime sampler from a fully loaded instrument. Seed controls random-region
    /// selection; identical note calls make identical sample choices.
    pub fn new(instrument: Arc<SfzInstrument>, sample_rate: u32, seed: u64) -> Self {
        Self {
            instrument,
            sample_rate: sample_rate.max(1),
            voices: Vec::with_capacity(MAX_VOICES),
            random_state: seed ^ 0x9e37_79b9_7f4a_7c15,
            gain: 0.3,
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.random_state = seed ^ 0x9e37_79b9_7f4a_7c15;
        if self.random_state == 0 { self.random_state = 1; }
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = if gain.is_finite() { gain.clamp(0.0, 4.0) } else { 0.0 };
    }

    pub fn active_voice_count(&self) -> usize { self.voices.len() }

    /// Trigger all matching regions. Returns false when the instrument has no matching zone,
    /// allowing the caller to fall back to the procedural voice.
    pub fn note_on(&mut self, note: u8, velocity: f32, gate_seconds: f32) -> bool {
        let velocity = velocity.clamp(0.0, 1.0);
        let velocity_key = (velocity * 127.0).round() as u8;
        let random = self.next_random();
        let mut triggered = false;
        for (region_index, region) in self.instrument.definition.regions.iter().enumerate() {
            if note < region.low_key || note > region.high_key ||
                velocity_key < region.low_velocity || velocity_key > region.high_velocity ||
                random < region.low_random || random >= region.high_random {
                continue;
            }
            let sample = &self.instrument.samples[self.instrument.sample_for_region[region_index]];
            let pitch_delta = f64::from(i16::from(note) - i16::from(region.pitch_keycenter))
                + f64::from(region.transpose_semitones)
                + f64::from(region.tune_cents) / 100.0;
            let increment = f64::from(sample.sample_rate) / f64::from(self.sample_rate)
                * 2.0f64.powf(pitch_delta / 12.0);
            let track = region.amp_veltrack / 100.0;
            let velocity_gain = if track == 0.0 { 1.0 } else { velocity.powf(track) };
            let gate_frames = (gate_seconds.max(0.0) * self.sample_rate as f32) as u64;
            let release_frames = (region.release_seconds * self.sample_rate as f32) as u64;
            if self.voices.len() == MAX_VOICES { self.voices.remove(0); }
            self.voices.push(Voice {
                region_index,
                position: region.sample_offset as f64,
                increment: increment.max(0.0001),
                age: 0,
                gate_frames,
                release_frames,
                velocity_gain,
                region_gain: 10.0f32.powf(region.volume_db / 20.0),
            });
            triggered = true;
        }
        triggered
    }

    /// Render one stereo frame. No file access, decoding or allocation occurs here.
    pub fn render_frame(&mut self) -> (f32, f32) {
        let mut left = 0.0;
        let mut right = 0.0;
        for voice in &mut self.voices {
            let region = &self.instrument.definition.regions[voice.region_index];
            let sample = &self.instrument.samples[self.instrument.sample_for_region[voice.region_index]];
            let sample_end = region.sample_end.unwrap_or(sample.frames).min(sample.frames);
            let loop_bounds = loop_range(region, sample_end);
            let looping = match region.loop_mode {
                SfzLoopMode::Continuous => loop_bounds.is_some(),
                SfzLoopMode::Sustain => voice.age < voice.gate_frames && loop_bounds.is_some(),
                SfzLoopMode::NoLoop | SfzLoopMode::OneShot => false,
            };
            if looping {
                if let Some((loop_start, loop_end)) = loop_bounds {
                    if voice.position >= loop_end as f64 {
                        voice.position = loop_start as f64
                            + (voice.position - loop_start as f64).rem_euclid((loop_end - loop_start) as f64);
                    }
                }
            }
            let frame = voice.position.floor() as usize;
            if frame < sample_end {
                let next_frame = if looping {
                    let (loop_start, loop_end) = loop_bounds.unwrap();
                    if frame + 1 >= loop_end { loop_start } else { (frame + 1).min(sample_end - 1) }
                } else {
                    (frame + 1).min(sample_end - 1)
                };
                let fraction = (voice.position - frame as f64) as f32;
                let channel_sample = |frame_index: usize, channel: usize| -> f32 {
                    sample.samples[frame_index * sample.channels + channel.min(sample.channels - 1)]
                };
                let mut values = [0.0f32; 2];
                for channel in 0..2 {
                    let first = channel_sample(frame, channel);
                    let second = channel_sample(next_frame, channel);
                    values[channel] = first + (second - first) * fraction;
                }
                let env = envelope(region, voice.age, voice.gate_frames, voice.release_frames, self.sample_rate);
                let gain = voice.region_gain * voice.velocity_gain * env * self.gain;
                left += values[0] * gain;
                right += values[1] * gain;
                voice.position += voice.increment;
            }
            voice.age = voice.age.saturating_add(1);
        }
        self.voices.retain(|voice| {
            let region = &self.instrument.definition.regions[voice.region_index];
            let sample = &self.instrument.samples[self.instrument.sample_for_region[voice.region_index]];
            let end = region.sample_end.unwrap_or(sample.frames).min(sample.frames);
            let natural_end = voice.position >= end as f64;
            if region.loop_mode == SfzLoopMode::OneShot { return !natural_end; }
            let release_finished = voice.age >= voice.gate_frames.saturating_add(voice.release_frames);
            if region.loop_mode == SfzLoopMode::Continuous && !release_finished { return true; }
            if region.loop_mode == SfzLoopMode::Sustain && !release_finished && !natural_end { return true; }
            !natural_end && !release_finished
        });
        (left, right)
    }

    fn next_random(&mut self) -> f32 {
        self.random_state = self.random_state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.random_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        ((z >> 40) as u32) as f32 / ((1u32 << 24) as f32)
    }
}

fn loop_range(region: &SfzRegion, sample_end: usize) -> Option<(usize, usize)> {
    if region.loop_mode == SfzLoopMode::NoLoop || region.loop_mode == SfzLoopMode::OneShot {
        return None;
    }
    let start = region.loop_start.unwrap_or(region.sample_offset);
    let end = region.loop_end.unwrap_or(sample_end);
    (start >= region.sample_offset && start < end && end <= sample_end).then_some((start, end))
}

fn envelope(region: &SfzRegion, age: u64, gate_frames: u64, release_frames: u64, sample_rate: u32) -> f32 {
    let seconds = age as f32 / sample_rate as f32;
    let gate_seconds = gate_frames as f32 / sample_rate as f32;
    let level_at = |time: f32| {
        if region.attack_seconds > 0.0 && time < region.attack_seconds {
            time / region.attack_seconds
        } else if region.decay_seconds > 0.0 && time < region.attack_seconds + region.decay_seconds {
            let decay_pos = (time - region.attack_seconds).max(0.0) / region.decay_seconds;
            1.0 + (region.sustain_level - 1.0) * decay_pos
        } else {
            region.sustain_level
        }
    };
    if region.loop_mode == SfzLoopMode::OneShot { return level_at(seconds); }
    if age < gate_frames { return level_at(seconds); }
    if release_frames == 0 || age >= gate_frames.saturating_add(release_frames) { return 0.0; }
    let start_level = level_at(gate_seconds);
    let release_pos = (age - gate_frames) as f32 / release_frames as f32;
    start_level * (1.0 - release_pos).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_committed_classical_guitar_mapping() {
        let mapping = include_str!("../samples/SpanishClassicalGuitar-20190618.sfz");
        let definition = parse_sfz(mapping).expect("classical guitar SFZ should parse");
        assert_eq!(definition.regions().len(), 48);
        let c4 = definition.regions().iter().find(|region| region.sample.ends_with("C4.wav")).unwrap();
        assert_eq!(c4.low_key, 60);
        assert_eq!(c4.high_key, 60);
        assert_eq!(c4.pitch_keycenter, 60);
        assert_eq!(c4.loop_mode, SfzLoopMode::NoLoop);
    }

    #[test]
    fn parses_every_committed_mapping_and_reports_unimplemented_opcodes() {
        let fixtures = [
            ("Dave Choir", include_str!("../samples/Dave Choir.sfz")),
            ("FSBS bridge", include_str!("../samples/EGuitarFSBS-clean bridge 20260807.sfz")),
            ("FSBS bridge small", include_str!("../samples/EGuitarFSBS-clean bridge small 20260807.sfz")),
            ("Lewis E Pyle Violin", include_str!("../samples/Lewis E Pyle Violin (Lite).sfz")),
            ("Spanish Classical Guitar", include_str!("../samples/SpanishClassicalGuitar-20190618.sfz")),
            ("ork dialogue", include_str!("../samples/ork_dialogue.sfz")),
            ("ork predrops", include_str!("../samples/ork_predrops.sfz")),
            ("ork vocals", include_str!("../samples/ork_vocals.sfz")),
        ];
        for (name, text) in fixtures {
            let definition = parse_sfz(text).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(!definition.regions().is_empty(), "{name} contains no regions");
        }
    }

    #[test]
    fn inherits_global_and_group_settings_and_supports_quoted_paths() {
        let source = r#"
<control> default_path="samples with spaces/"
<global> ampeg_release=0.8 volume=-3
<group> lokey=60 hikey=72 ampeg_attack=0.1
<region> sample="C 4.wav" key=60 pitch_keycenter=60
<region> sample=D4.wav key=62
"#;
        let definition = parse_sfz(source).unwrap();
        assert_eq!(definition.default_path(), Some(Path::new("samples with spaces")));
        assert_eq!(definition.regions().len(), 2);
        assert!((definition.regions()[1].release_seconds - 0.8).abs() < 0.0001);
        assert!((definition.regions()[1].attack_seconds - 0.1).abs() < 0.0001);
        assert_eq!(definition.regions()[0].sample, PathBuf::from("C 4.wav"));
        assert!((definition.regions()[0].volume_db + 3.0).abs() < 0.0001);
    }

    #[test]
    fn rejects_malformed_regions_with_a_useful_line_number() {
        let error = parse_sfz("<region> key=60\n").unwrap_err();
        assert!(error.to_string().contains("line 1"));
        assert!(error.to_string().contains("sample="));
    }

    #[test]
    fn deterministic_sampler_repeats_random_region_selection() {
        let root = std::env::temp_dir().join(format!("verdandi-sfz-{}", std::process::id()));
        let _ = fs::create_dir_all(&root);
        let wav_path = root.join("tone.wav");
        let sfz_path = root.join("tone.sfz");
        write_test_wav(&wav_path);
        fs::write(&sfz_path, "<global> amp_veltrack=100\n<region> sample=tone.wav key=60\n").unwrap();
        let instrument = Arc::new(SfzInstrument::load(&sfz_path).unwrap());
        let mut first = SfzSampler::new(instrument.clone(), 44_100, 9);
        let mut second = SfzSampler::new(instrument, 44_100, 9);
        assert!(first.note_on(60, 0.8, 0.1));
        assert!(second.note_on(60, 0.8, 0.1));
        let first_audio: Vec<_> = (0..256).map(|_| first.render_frame()).collect();
        let second_audio: Vec<_> = (0..256).map(|_| second.render_frame()).collect();
        assert_eq!(first_audio, second_audio);
        assert!(first_audio.iter().any(|(left, right)| left.abs() > 0.001 || right.abs() > 0.001));
        assert!(first_audio.iter().all(|(left, right)| left.is_finite() && right.is_finite()));
        let _ = fs::remove_file(wav_path);
        let _ = fs::remove_file(sfz_path);
        let _ = fs::remove_dir(root);
    }

    fn write_test_wav(path: &Path) {
        let frames = 1024u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + frames * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&44_100u32.to_le_bytes());
        bytes.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(frames * 2).to_le_bytes());
        for index in 0..frames {
            let value = if index % 2 == 0 { 10_000i16 } else { -10_000i16 };
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fs::write(path, bytes).unwrap();
    }
}
