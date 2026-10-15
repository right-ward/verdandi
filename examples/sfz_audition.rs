//! Render notes from a user-supplied SFZ patch into a WAV file.
//!
//! Usage:
//! cargo run --release --no-default-features --example sfz_audition -- patch.sfz output.wav [seed] [first-midi-note] [last-midi-note]
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use verdandi::sfz::{SfzInstrument, SfzSampler};

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u16 = 2;
const BITS_PER_SAMPLE: u16 = 16;
const SECONDS_PER_NOTE: u32 = 2;
const NOTE_GATE_SECONDS: f32 = 0.7;
const FRAMES_PER_CHUNK: usize = 4096;

fn main() {
    if let Err(error) = run() {
        eprintln!("sfz_audition: {error}");
        eprintln!("Usage: sfz_audition <patch.sfz> <output.wav> [seed] [first-midi-note] [last-midi-note] [--sample-root DIR]");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("expected an SFZ patch and output WAV".into());
    }

    let mut positional = args[..2].to_vec();
    let mut sample_root: Option<String> = None;
    let mut index = 2;
    while index < args.len() {
        if args[index] == "--sample-root" {
            if sample_root.is_some() {
                return Err("--sample-root may only be specified once".into());
            }
            let root = args
                .get(index + 1)
                .ok_or("--sample-root requires a directory")?;
            sample_root = Some(root.clone());
            index += 2;
        } else {
            positional.push(args[index].clone());
            index += 1;
        }
    }
    if positional.len() > 5 {
        return Err("expected at most seed, first MIDI note, and last MIDI note".into());
    }

    let patch = Path::new(&positional[0]);
    let output = Path::new(&positional[1]);
    let seed = parse_optional(&positional, 2, 1u64, "seed")?;
    let instrument = Arc::new(if let Some(sample_root) = sample_root {
        SfzInstrument::load_with_sample_root(patch, sample_root)?
    } else {
        SfzInstrument::load(patch)?
    });
    if !instrument.unsupported_opcodes().is_empty() {
        eprintln!(
            "Warning: these SFZ opcodes/sections are not implemented and are ignored: {}",
            instrument.unsupported_opcodes().join(", ")
        );
    }

    let inferred_first = instrument
        .regions()
        .iter()
        .map(|region| region.low_key)
        .min()
        .unwrap_or(0);
    let inferred_last = instrument
        .regions()
        .iter()
        .map(|region| region.high_key)
        .max()
        .unwrap_or(127);
    let first_note = parse_optional(&positional, 3, inferred_first, "first MIDI note")?;
    let last_note = parse_optional(&positional, 4, inferred_last, "last MIDI note")?;
    if first_note > 127 || last_note > 127 || first_note > last_note {
        return Err("MIDI note bounds must satisfy 0 <= first <= last <= 127".into());
    }

    let note_count = u64::from(last_note - first_note + 1);
    let frames_per_note = u64::from(SAMPLE_RATE) * u64::from(SECONDS_PER_NOTE);
    let total_frames = note_count
        .checked_mul(frames_per_note)
        .ok_or("requested render is too large")?;
    let data_bytes = total_frames
        .checked_mul(u64::from(CHANNELS) * u64::from(BITS_PER_SAMPLE / 8))
        .ok_or("requested WAV is too large")?;
    let data_size = u32::try_from(data_bytes)
        .map_err(|_| "requested WAV exceeds the standard RIFF size limit")?;
    let riff_size = u32::try_from(
        36u64
            .checked_add(data_bytes)
            .ok_or("requested WAV is too large")?,
    )
    .map_err(|_| "requested WAV exceeds the standard RIFF size limit")?;

    let file = File::create(output)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(&wav_header(SAMPLE_RATE, data_size, riff_size))?;

    let mut sampler = SfzSampler::new(Arc::clone(&instrument), SAMPLE_RATE, seed);
    let mut buffer = vec![0.0f32; FRAMES_PER_CHUNK * usize::from(CHANNELS)];
    let mut pcm = Vec::with_capacity(buffer.len() * 2);
    let mut silent_notes = 0usize;

    for note in first_note..=last_note {
        sampler.set_seed(seed ^ u64::from(note).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        if !sampler.note_on(note as u8, 0.8, NOTE_GATE_SECONDS) {
            silent_notes += 1;
            eprintln!("Warning: MIDI note {note} has no matching SFZ region");
        }

        let mut frames_remaining = frames_per_note;
        while frames_remaining > 0 {
            let frames = frames_remaining.min(FRAMES_PER_CHUNK as u64) as usize;
            let sample_count = frames * usize::from(CHANNELS);
            for frame in buffer[..sample_count].chunks_exact_mut(2) {
                let (left, right) = sampler.render_frame();
                frame[0] = left.clamp(-1.0, 1.0);
                frame[1] = right.clamp(-1.0, 1.0);
            }
            pcm.clear();
            for sample in &buffer[..sample_count] {
                let value = (sample * f32::from(i16::MAX)).round() as i16;
                pcm.extend_from_slice(&value.to_le_bytes());
            }
            writer.write_all(&pcm)?;
            frames_remaining -= frames as u64;
        }
    }
    writer.flush()?;

    let duration = total_frames as f64 / f64::from(SAMPLE_RATE);
    println!(
        "Rendered {} MIDI note(s) from {} SFZ region(s), {:.1}s at {} Hz to {}",
        note_count,
        instrument.region_count(),
        duration,
        SAMPLE_RATE,
        output.display()
    );
    if silent_notes > 0 {
        eprintln!("{silent_notes} note(s) had no matching region; they were rendered as silence.");
    }
    Ok(())
}

fn parse_optional<T>(
    args: &[String],
    index: usize,
    default: T,
    label: &str,
) -> Result<T, Box<dyn Error>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
{
    args.get(index)
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|_| format!("invalid {label}: {value:?}").into())
        })
        .unwrap_or(Ok(default))
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
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    header[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_size.to_le_bytes());
    header
}
