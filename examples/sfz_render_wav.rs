//! Render a note-by-note audition WAV from a user-provided SFZ library.
//!
//! cargo run --release --no-default-features --example sfz_render_wav -- -- <mapping.sfz> <sample-root> <output.wav> [first-midi] [last-midi]
//!
//! The sample root is the directory against which paths inside the SFZ are resolved.

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use verdandi::sfz::SfzInstrument;

const RATE: u32 = 44_100;
const CHANNELS: u16 = 2;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        eprintln!("Usage: sfz_render_wav <mapping.sfz> <sample-root> <output.wav> [first-midi] [last-midi]");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if !(3..=5).contains(&args.len()) {
        return Err("expected mapping, sample root, output, and optional MIDI range".into());
    }
    let first = args.get(3).map(|v| v.parse::<u8>()).transpose()?.unwrap_or(48);
    let last = args.get(4).map(|v| v.parse::<u8>()).transpose()?.unwrap_or(72);
    if first > last || last > 127 { return Err("MIDI range must satisfy 0 <= first <= last <= 127".into()); }

    let instrument = SfzInstrument::load_with_sample_root(&args[0], &args[1])?;
    eprintln!("Loaded {} region(s) from {}", instrument.region_count(), instrument.source().display());
    for diagnostic in instrument.diagnostics() { eprintln!("SFZ: {diagnostic}"); }

    let note_seconds = 1.4f32;
    let gap_seconds = 0.15f32;
    let note_frames = (note_seconds * RATE as f32) as usize;
    let gap_frames = (gap_seconds * RATE as f32) as usize;
    let total_frames = usize::from(last - first + 1) * (note_frames + gap_frames);
    let data_bytes = u32::try_from(total_frames.checked_mul(4).ok_or("output size overflow")?)
        .map_err(|_| "output exceeds the standard RIFF/WAV size limit")?;
    let mut writer = BufWriter::new(File::create(Path::new(&args[2]))?);
    writer.write_all(&wav_header(RATE, data_bytes, data_bytes + 36))?;

    let mut pcm = Vec::with_capacity(8192 * 4);
    for note in first..=last {
        let mut voice = instrument.note_on(note, 0.8, 0.65, RATE)
            .ok_or_else(|| format!("no SFZ region covers MIDI note {note} at velocity 0.8"))?;
        for _ in 0..note_frames {
            let sample = (voice.next_sample() * 0.65).clamp(-1.0, 1.0);
            let value = (sample * i16::MAX as f32).round() as i16;
            pcm.extend_from_slice(&value.to_le_bytes());
            pcm.extend_from_slice(&value.to_le_bytes());
            if pcm.len() >= 8192 * 4 { writer.write_all(&pcm)?; pcm.clear(); }
        }
        for _ in 0..gap_frames {
            pcm.extend_from_slice(&[0, 0, 0, 0]);
            if pcm.len() >= 8192 * 4 { writer.write_all(&pcm)?; pcm.clear(); }
        }
        eprintln!("Rendered MIDI {note}");
    }
    if !pcm.is_empty() { writer.write_all(&pcm)?; }
    writer.flush()?;
    println!("Wrote {} notes to {}", last - first + 1, args[2]);
    Ok(())
}

fn wav_header(rate: u32, data_size: u32, riff_size: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF"); h[4..8].copy_from_slice(&riff_size.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE"); h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes()); h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&CHANNELS.to_le_bytes()); h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * 4).to_le_bytes()); h[32..34].copy_from_slice(&4u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes()); h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_size.to_le_bytes()); h
}
