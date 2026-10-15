//! First-party SFZ loading and one-shot sample playback.
//!
//! This first vertical slice supports the SFZ features needed by Verdandi's Spanish Classical
//! Guitar mapping. It reports unsupported opcodes as diagnostics; it is not yet full SFZ coverage.
//! Samples are decoded during loading, never during voice rendering.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub enum SfzError {
    Io(std::io::Error),
    Invalid(String),
}
impl std::fmt::Display for SfzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::Io(e) => write!(f, "I/O error: {e}"), Self::Invalid(s) => f.write_str(s) }
    }
}
impl std::error::Error for SfzError {}
impl From<std::io::Error> for SfzError { fn from(e: std::io::Error) -> Self { Self::Io(e) } }

#[derive(Clone, Debug)]
struct Sample { sample_rate: u32, frames: Arc<[f32]> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopMode { None, Continuous }
#[derive(Clone, Debug)]
struct Region {
    sample: Sample, low_key: u8, high_key: u8, root_key: u8,
    low_velocity: u8, high_velocity: u8, volume_db: f32, release_seconds: f32,
    loop_mode: LoopMode, loop_start: usize, loop_end: Option<usize>,
}

/// A loaded SFZ instrument. Supported opcodes: sample, key/lokey/hikey, pitch_keycenter,
/// lovel/hivel, volume, ampeg_release, loop_mode, loop_start and loop_end. WAV PCM 8/16/24/32-bit
/// and IEEE float 32/64-bit are supported. This is a deliberately partial, growing SFZ engine.
#[derive(Clone, Debug)]
pub struct SfzInstrument { regions: Vec<Region>, source: PathBuf, diagnostics: Vec<String> }

/// A single active sample voice. The audio thread only reads already-decoded samples.
#[derive(Clone, Debug)]
pub struct SfzVoice {
    sample: Sample, position: f64, increment: f64, velocity_gain: f32, volume_gain: f32,
    gate_frames: u64, release_frames: u64, age_frames: u64, loop_mode: LoopMode,
    loop_start: usize, loop_end: usize, finished: bool,
}

impl SfzInstrument {
    /// Load an SFZ file, resolving sample paths relative to its parent directory.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SfzError> {
        let path = path.as_ref();
        Self::load_with_sample_root(path, path.parent().unwrap_or_else(|| Path::new(".")))
    }

    /// Load an SFZ mapping while resolving its sample paths against an explicit library root.
    pub fn load_with_sample_root(path: impl AsRef<Path>, sample_root: impl AsRef<Path>) -> Result<Self, SfzError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        let mut global = Vec::new();
        let mut group = Vec::new();
        let mut scope = Scope::None;
        let mut pending: Option<Vec<(String, String)>> = None;
        let mut regions = Vec::new();
        let mut diagnostics = Vec::new();

        for (index, original) in text.lines().enumerate() {
            let line = original.split("//").next().unwrap_or("").trim();
            if line.is_empty() { continue; }
            let mut rest = line;
            while !rest.is_empty() {
                if let Some(open) = rest.find('<') {
                    let before = rest[..open].trim();
                    if !before.is_empty() { add_ops(before, scope, &mut global, &mut group, &mut pending, &mut diagnostics, index + 1); }
                    let Some(close) = rest[open..].find('>') else {
                        return Err(SfzError::Invalid(format!("{}:{}: unterminated section tag", path.display(), index + 1)));
                    };
                    let tag = rest[open + 1..open + close].trim().to_ascii_lowercase();
                    rest = rest[open + close + 1..].trim();
                    match tag.as_str() {
                        "global" => scope = Scope::Global,
                        "group" => { group.clear(); scope = Scope::Group; }
                        "region" => {
                            if let Some(ops) = pending.take() {
                                regions.push(make_region(&global, &group, &ops, sample_root.as_ref(), path, index + 1, &mut diagnostics)?);
                            }
                            pending = Some(Vec::new());
                            scope = Scope::Region;
                        }
                        "control" | "master" | "curve" | "effect" | "midi" => {
                            diagnostics.push(format!("{}:{}: <{}> section ignored (not implemented)", path.display(), index + 1, tag));
                            scope = Scope::Ignored;
                        }
                        _ => { diagnostics.push(format!("{}:{}: unknown <{}> section ignored", path.display(), index + 1, tag)); scope = Scope::Ignored; }
                    }
                } else {
                    add_ops(rest, scope, &mut global, &mut group, &mut pending, &mut diagnostics, index + 1);
                    break;
                }
            }
        }
        if let Some(ops) = pending.take() {
            regions.push(make_region(&global, &group, &ops, sample_root.as_ref(), path, text.lines().count(), &mut diagnostics)?);
        }
        if regions.is_empty() { return Err(SfzError::Invalid(format!("{} contains no usable regions", path.display()))); }
        Ok(Self { regions, source: path.to_path_buf(), diagnostics })
    }

    pub fn source(&self) -> &Path { &self.source }
    pub fn region_count(&self) -> usize { self.regions.len() }
    pub fn diagnostics(&self) -> &[String] { &self.diagnostics }

    /// Start a note. Velocity is normalized to 0..=1; duration is the held-note time in seconds.
    pub fn note_on(&self, note: u8, velocity: f32, gate_seconds: f32, output_sample_rate: u32) -> Option<SfzVoice> {
        if note > 127 || output_sample_rate == 0 || !gate_seconds.is_finite() { return None; }
        let velocity = velocity.clamp(0.0, 1.0);
        let midi_velocity = (velocity * 127.0).round() as u8;
        let r = self.regions.iter().find(|r| (r.low_key..=r.high_key).contains(&note)
            && (r.low_velocity..=r.high_velocity).contains(&midi_velocity))?;
        let increment = f64::from(r.sample.sample_rate) / f64::from(output_sample_rate)
            * 2.0_f64.powf((f64::from(note) - f64::from(r.root_key)) / 12.0);
        let loop_start = r.loop_start.min(r.sample.frames.len().saturating_sub(1));
        let loop_end = r.loop_end.unwrap_or(r.sample.frames.len()).min(r.sample.frames.len());
        let loop_mode = if loop_end > loop_start + 1 { r.loop_mode } else { LoopMode::None };
        Some(SfzVoice {
            sample: r.sample.clone(), position: 0.0, increment, velocity_gain: velocity,
            volume_gain: 10.0_f32.powf(r.volume_db / 20.0),
            gate_frames: (gate_seconds.max(0.0) * output_sample_rate as f32) as u64,
            release_frames: (r.release_seconds * output_sample_rate as f32) as u64,
            age_frames: 0, loop_mode, loop_start, loop_end, finished: false,
        })
    }
}

impl SfzVoice {
    /// Generate the next mono sample; returns zero after this voice has finished.
    pub fn next_sample(&mut self) -> f32 {
        if self.finished || self.sample.frames.is_empty() { return 0.0; }
        if self.loop_mode == LoopMode::Continuous && self.loop_end > self.loop_start
            && self.position >= self.loop_end as f64
        {
            let length = (self.loop_end - self.loop_start) as f64;
            self.position = self.loop_start as f64 + (self.position - self.loop_start as f64).rem_euclid(length);
        } else if self.position >= self.sample.frames.len() as f64 {
            self.finished = true;
            return 0.0;
        }
        let index = self.position as usize;
        let fraction = (self.position - index as f64) as f32;
        let next = if self.loop_mode == LoopMode::Continuous && index + 1 >= self.loop_end {
            self.loop_start
        } else { (index + 1).min(self.sample.frames.len() - 1) };
        let a = self.sample.frames[index];
        let b = self.sample.frames[next];
        let release = if self.age_frames < self.gate_frames {
            1.0
        } else if self.release_frames == 0 {
            if self.loop_mode == LoopMode::Continuous {
                self.finished = true;
                return 0.0;
            }
            1.0
        } else {
            let elapsed = self.age_frames - self.gate_frames;
            if elapsed >= self.release_frames { self.finished = true; return 0.0; }
            1.0 - elapsed as f32 / self.release_frames as f32
        };
        self.position += self.increment;
        self.age_frames = self.age_frames.saturating_add(1);
        let value = (a + (b - a) * fraction) * self.velocity_gain * self.volume_gain * release;
        if value.is_finite() { value } else { 0.0 }
    }
    pub fn is_finished(&self) -> bool { self.finished }
    /// Add mono voice output to an interleaved stereo buffer without clearing existing contents.
    pub fn mix_stereo(&mut self, output: &mut [f32], gain: f32) {
        for frame in output.chunks_exact_mut(2) {
            let value = self.next_sample() * gain;
            frame[0] += value; frame[1] += value;
            if self.finished { break; }
        }
    }
}

#[derive(Clone, Copy)]
enum Scope { None, Global, Group, Region, Ignored }

fn add_ops(text: &str, scope: Scope, global: &mut Vec<(String, String)>, group: &mut Vec<(String, String)>,
    region: &mut Option<Vec<(String, String)>>, diagnostics: &mut Vec<String>, line: usize) {
    for token in text.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            diagnostics.push(format!("line {line}: ignored malformed token {token:?}")); continue;
        };
        let pair = (key.trim().to_ascii_lowercase(), value.trim().trim_matches('"').to_string());
        match scope {
            Scope::Global => global.push(pair), Scope::Group => group.push(pair),
            Scope::Region => if let Some(ops) = region { ops.push(pair); },
            Scope::None | Scope::Ignored => {}
        }
    }
}
fn get<'a>(ops: &'a [(String, String)], key: &str) -> Option<&'a str> {
    ops.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}
fn inherited<'a>(global: &'a [(String, String)], group: &'a [(String, String)], region: &'a [(String, String)], key: &str) -> Option<&'a str> {
    get(region, key).or_else(|| get(group, key)).or_else(|| get(global, key))
}
fn number<T: std::str::FromStr>(value: Option<&str>, default: T, key: &str) -> Result<T, SfzError> {
    match value { Some(v) => v.parse().map_err(|_| SfzError::Invalid(format!("invalid {key} value {v:?}"))), None => Ok(default) }
}
fn make_region(global: &[(String, String)], group: &[(String, String)], ops: &[(String, String)],
    root: &Path, sfz: &Path, line: usize, diagnostics: &mut Vec<String>) -> Result<Region, SfzError> {
    let val = |key| inherited(global, group, ops, key);
    let sample_value = val("sample").ok_or_else(|| SfzError::Invalid(format!("{}:{}: region missing sample=", sfz.display(), line)))?;
    let relative = PathBuf::from(sample_value.replace('\\', "/"));
    let sample_path = if relative.is_absolute() { relative } else { root.join(relative) };
    let (sample_rate, frames) = read_wav(&sample_path).map_err(|e| SfzError::Invalid(format!("{}:{}: cannot load {}: {e}", sfz.display(), line, sample_path.display())))?;
    let key = val("key").map(|v| number(Some(v), 60u8, "key")).transpose()?;
    let low_key = val("lokey").map(|v| number(Some(v), 0u8, "lokey")).transpose()?.or(key).unwrap_or(0);
    let high_key = val("hikey").map(|v| number(Some(v), 127u8, "hikey")).transpose()?.or(key).unwrap_or(127);
    let root_key = val("pitch_keycenter").map(|v| number(Some(v), 60u8, "pitch_keycenter")).transpose()?
        .or(key).unwrap_or((low_key + high_key) / 2);
    let low_velocity = number(val("lovel"), 0u8, "lovel")?;
    let high_velocity = number(val("hivel"), 127u8, "hivel")?;
    if low_key > high_key || low_velocity > high_velocity
        || high_key > 127 || root_key > 127 || high_velocity > 127
    {
        return Err(SfzError::Invalid(format!("{}:{}: invalid key or velocity range (MIDI values must be 0..=127)", sfz.display(), line)));
    }
    let volume_db = number(val("volume"), 0.0f32, "volume")?;
    let release_seconds = number(val("ampeg_release"), 0.0f32, "ampeg_release")?.clamp(0.0, 30.0);
    let loop_mode = match val("loop_mode").unwrap_or("no_loop") {
        "no_loop" | "one_shot" => LoopMode::None,
        "loop_continuous" | "loop_sustain" | "loop_until_release" => LoopMode::Continuous,
        other => { diagnostics.push(format!("unsupported loop_mode={other}; using no_loop")); LoopMode::None }
    };
    let loop_start = number(val("loop_start"), 0usize, "loop_start")?;
    let loop_end = val("loop_end").map(|v| number(Some(v), 0usize, "loop_end")).transpose()?;
    let known = ["sample", "key", "lokey", "hikey", "pitch_keycenter", "lovel", "hivel", "volume", "ampeg_release", "loop_mode", "loop_start", "loop_end"];
    for (k, _) in global.iter().chain(group).chain(ops) {
        if !known.contains(&k.as_str()) && !diagnostics.iter().any(|d| d == &format!("unsupported opcode {k} (currently ignored)")) {
            diagnostics.push(format!("unsupported opcode {k} (currently ignored)"));
        }
    }
    Ok(Region { sample: Sample { sample_rate, frames: Arc::from(frames) }, low_key, high_key, root_key,
        low_velocity, high_velocity, volume_db, release_seconds, loop_mode, loop_start, loop_end })
}

fn read_wav(path: &Path) -> Result<(u32, Vec<f32>), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" { return Err("not a RIFF/WAVE file".into()); }
    let mut format = 0u16; let mut channels = 0u16; let mut rate = 0u32; let mut bits = 0u16;
    let mut data: Option<&[u8]> = None; let mut cursor = 12usize;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let start = cursor + 8; let end = start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && end >= start + 16 {
            format = u16::from_le_bytes([bytes[start], bytes[start + 1]]);
            channels = u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]);
            rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
            bits = u16::from_le_bytes([bytes[start + 14], bytes[start + 15]]);
        } else if id == b"data" { data = Some(&bytes[start..end]); }
        cursor = start.saturating_add(size).saturating_add(size & 1);
    }
    let data = data.ok_or("missing data chunk")?;
    if channels == 0 || rate == 0 { return Err("invalid channel count or sample rate".into()); }
    let width = usize::from(bits / 8); let frame_width = width.checked_mul(usize::from(channels)).ok_or("invalid frame size")?;
    if width == 0 || frame_width == 0 || data.len() < frame_width { return Err("WAV has no complete frames".into()); }
    if !matches!((format, bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64)) { return Err(format!("unsupported WAV encoding {format}, {bits} bits")); }
    let mut mono = Vec::with_capacity(data.len() / frame_width);
    for frame in data.chunks_exact(frame_width) {
        let mut sum = 0.0f32;
        for channel in 0..usize::from(channels) {
            let i = channel * width;
            let s = match (format, bits) {
                (1, 8) => (f32::from(frame[i]) - 128.0) / 128.0,
                (1, 16) => i16::from_le_bytes([frame[i], frame[i+1]]) as f32 / 32768.0,
                (1, 24) => { let n = i32::from(frame[i]) | (i32::from(frame[i+1]) << 8) | (i32::from(frame[i+2]) << 16); (if n & 0x800000 != 0 { n | !0xFFFFFF } else { n }) as f32 / 8_388_608.0 },
                (1, 32) => i32::from_le_bytes(frame[i..i+4].try_into().unwrap()) as f32 / 2_147_483_648.0,
                (3, 32) => f32::from_le_bytes(frame[i..i+4].try_into().unwrap()),
                (3, 64) => f64::from_le_bytes(frame[i..i+8].try_into().unwrap()) as f32,
                _ => unreachable!(),
            };
            sum += if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        }
        mono.push(sum / f32::from(channels));
    }
    Ok((rate, mono))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn instrument() -> SfzInstrument {
        let sample = Sample { sample_rate: 44_100, frames: Arc::from(vec![0.0f32, 1.0, 0.0, -1.0]) };
        SfzInstrument { regions: vec![Region { sample, low_key: 60, high_key: 60, root_key: 60,
            low_velocity: 1, high_velocity: 127, volume_db: 0.0, release_seconds: 0.1,
            loop_mode: LoopMode::None, loop_start: 0, loop_end: None }],
            source: PathBuf::from("fixture.sfz"), diagnostics: Vec::new() }
    }
    #[test] fn selects_matching_key_and_velocity() {
        let sfz = instrument();
        assert!(sfz.note_on(60, 1.0, 0.1, 44_100).is_some());
        assert!(sfz.note_on(59, 1.0, 0.1, 44_100).is_none());
        assert!(sfz.note_on(60, 0.0, 0.1, 44_100).is_none());
    }
    #[test] fn voices_produce_finite_samples_and_end() {
        let sfz = instrument();
        let mut voice = sfz.note_on(60, 0.8, 0.0, 44_100).unwrap();
        for _ in 0..16 { assert!(voice.next_sample().is_finite()); }
        assert!(voice.is_finished());
    }

    #[test]
    fn loads_region_and_resolves_sample_root() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("verdandi-sfz-{}-{unique}", std::process::id()));
        let samples = root.join("samples");
        fs::create_dir_all(&samples).unwrap();
        let wav_path = samples.join("test.wav");
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&44u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&88_200u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8u32.to_le_bytes());
        for sample in [0i16, 12000, -12000, 0] { wav.extend_from_slice(&sample.to_le_bytes()); }
        fs::write(&wav_path, wav).unwrap();

        let mapping = root.join("instrument.sfz");
        fs::write(&mapping, "<global>\nampeg_release=0.1\n<region> sample=samples/test.wav key=60\n").unwrap();
        let loaded = SfzInstrument::load_with_sample_root(&mapping, &root).unwrap();
        assert_eq!(loaded.region_count(), 1);
        let mut voice = loaded.note_on(60, 0.8, 0.01, 44_100).unwrap();
        assert!(voice.next_sample().is_finite());
        fs::remove_dir_all(root).unwrap();
    }
}
