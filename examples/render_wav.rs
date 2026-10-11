//! Render a deterministic Verdandi arrangement to a standalone stereo WAV file.
//!
//! cargo run --release --no-default-features --example render_wav -- 1234 mythic-ambient 8 mythic.wav
//! cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 pad.wav standard pad
//! cargo run --release --no-default-features --example render_wav -- 1234 heavy-metal 8 heavy.wav fast

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use verdandi::composer::STEPS_PER_LOOP;
use verdandi::prelude::{Engine, Layer, MusicConfig, MusicPace, MusicPreset, MusicShared};

const CHANNELS: u16 = 2;
const BITS_PER_SAMPLE: u16 = 16;
const SAMPLES_PER_CHUNK: usize = 8_192;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        eprintln!("Usage: render_wav <seed> <preset> <loops> <output.wav> [standard|fast] [layer]");
        eprintln!("Layers: drums, bass, pad, arp, lead, rhythm-guitar, strings, choir");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if !(4..=6).contains(&args.len()) {
        return Err(
            "expected seed, preset, loop count, output path, and optional pace and layer".into(),
        );
    }

    let seed = args[0].parse::<u64>().map_err(|_| {
        format!(
            "invalid seed {:?}: expected an unsigned 64-bit integer",
            args[0]
        )
    })?;

    let preset = match args[1].as_str() {
        "ethereal" | "mythic-ambient" | "ambient" => MusicPreset::MythicAmbient,
        "contemplative" | "classical-ambient" | "classical" => MusicPreset::ClassicalAmbient,
        "heavy-metal" | "metal" => MusicPreset::HeavyMetal,
        "legendary-metal" | "legendary" => MusicPreset::LegendaryMetal,
        other => {
            return Err(format!(
                "unknown preset {other:?}; use mythic-ambient, classical-ambient, heavy-metal, or legendary-metal"
            )
            .into());
        }
    };

    let loops = args[2].parse::<u32>().map_err(|_| {
        format!(
            "invalid loop count {:?}: expected a positive integer",
            args[2]
        )
    })?;
    if loops == 0 {
        return Err("loop count must be greater than zero".into());
    }

    let pace = match args.get(4).map(String::as_str).unwrap_or("standard") {
        "standard" => MusicPace::Standard,
        "fast" => MusicPace::Fast,
        other => return Err(format!("unknown pace {other:?}; use standard or fast").into()),
    };

    let isolated_layer = args.get(5).map(|name| parse_layer(name)).transpose()?;
    if let Some(layer) = isolated_layer {
        if !layer.used_by(preset) {
            return Err(
                format!("layer {:?} is not used by the {} preset", args[5], args[1]).into(),
            );
        }
    }

    let mut layers_enabled = [true; Layer::COUNT];
    if let Some(layer) = isolated_layer {
        layers_enabled.fill(false);
        layers_enabled[layer.index()] = true;
    }

    let config = MusicConfig {
        seed,
        preset,
        pace,
        layers_enabled,
        auto_intensity: isolated_layer.is_none(),
        intensity: 1.0,
        ..MusicConfig::default()
    };
    let shared = Arc::new(MusicShared::new(&config));
    let mut engine = Engine::new(shared.clone(), &config);
    let bpm = shared.status().bpm;
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err(format!("engine returned invalid tempo: {bpm}").into());
    }

    // Engine schedules sixteenth-note steps and carries fractional-sample timing
    // between steps. Ceil the total duration so the file ends at the loop boundary.
    let frames_per_step = 60.0 / f64::from(bpm) / 4.0 * f64::from(config.sample_rate);
    let total_frames_f64 = (frames_per_step * f64::from(STEPS_PER_LOOP) * f64::from(loops)).ceil();
    if !total_frames_f64.is_finite() || total_frames_f64 < 1.0 || total_frames_f64 > u64::MAX as f64
    {
        return Err("requested render length is out of range".into());
    }

    let total_frames = total_frames_f64 as u64;
    let bytes_per_frame = u64::from(CHANNELS) * u64::from(BITS_PER_SAMPLE / 8);
    let data_bytes = total_frames
        .checked_mul(bytes_per_frame)
        .ok_or("requested WAV is too large")?;
    let data_size = u32::try_from(data_bytes)
        .map_err(|_| "requested WAV exceeds the standard RIFF size limit")?;
    let riff_size = u32::try_from(
        36u64
            .checked_add(data_bytes)
            .ok_or("requested WAV is too large")?,
    )
    .map_err(|_| "requested WAV exceeds the standard RIFF size limit")?;

    let output_path = Path::new(&args[3]);
    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(&wav_header(config.sample_rate, data_size, riff_size))?;

    let frames_per_chunk = SAMPLES_PER_CHUNK / usize::from(CHANNELS);
    let mut samples = vec![0.0_f32; SAMPLES_PER_CHUNK];
    let mut pcm_bytes = Vec::with_capacity(SAMPLES_PER_CHUNK * 2);
    let mut frames_remaining = total_frames;

    while frames_remaining > 0 {
        let frames = frames_remaining.min(frames_per_chunk as u64) as usize;
        let sample_count = frames * usize::from(CHANNELS);
        engine.render_interleaved(&mut samples[..sample_count]);

        pcm_bytes.clear();
        for sample in &samples[..sample_count] {
            let pcm = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            pcm_bytes.extend_from_slice(&pcm.to_le_bytes());
        }
        writer.write_all(&pcm_bytes)?;
        frames_remaining -= frames as u64;
    }
    writer.flush()?;

    let duration_secs = total_frames as f64 / f64::from(config.sample_rate);
    if let Some(layer) = isolated_layer {
        println!(
            "Wrote {loops} loop(s) of the {layer:?} layer, {duration_secs:.1}s at {bpm:.1} BPM, to {}",
            output_path.display()
        );
    } else {
        println!(
            "Wrote {loops} loop(s), {duration_secs:.1}s at {bpm:.1} BPM, to {}",
            output_path.display()
        );
    }
    Ok(())
}

fn parse_layer(name: &str) -> Result<Layer, Box<dyn Error>> {
    match name {
        "drums" => Ok(Layer::Drums),
        "bass" => Ok(Layer::Bass),
        "pad" => Ok(Layer::Pad),
        "arp" => Ok(Layer::Arp),
        "lead" => Ok(Layer::Lead),
        "rhythm-guitar" => Ok(Layer::RhythmGuitar),
        "strings" => Ok(Layer::Strings),
        "choir" => Ok(Layer::Choir),
        other => Err(format!(
            "unknown layer {other:?}; use drums, bass, pad, arp, lead, rhythm-guitar, strings, or choir"
        )
        .into()),
    }
}

fn wav_header(sample_rate: u32, data_size: u32, riff_size: u32) -> [u8; 44] {
    let mut header = [0u8; 44];
    let byte_rate = sample_rate * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;

    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&riff_size.to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM format
    header[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    header[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_size.to_le_bytes());
    header
}
