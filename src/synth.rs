//! A deliberately small subtractive synth + drum machine. No allocation after construction
//! (apart from re-sizing the delay on a tempo change), so it is safe to run on the audio thread.
//! Oscillators are naive (not band-limited); swap in PolyBLEP or samples if you hear aliasing.

use std::f32::consts::{FRAC_PI_4, SQRT_2, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timbre {
    Kick,
    Snare,
    Hat,
    OpenHat,
    Bass,
    Pad,
    Pluck,
    Lead,
    ClassicalLead,
    Guitar,
    Strings,
    ClassicalStrings,
    Choir,
    Bell,
}

pub fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

const MAX_VOICES: usize = 40;

fn level(timbre: Timbre) -> f32 {
    match timbre {
        Timbre::Kick => 0.9,
        Timbre::Snare => 0.55,
        Timbre::Hat => 0.22,
        Timbre::OpenHat => 0.2,
        Timbre::Bass => 0.55,
        Timbre::Pad => 0.2,
        Timbre::Pluck => 0.3,
        Timbre::Lead => 0.28,
        Timbre::ClassicalLead => 0.25,
        Timbre::Guitar => 0.36,
        Timbre::Strings => 0.28,
        Timbre::ClassicalStrings => 0.23,
        Timbre::Choir => 0.2,
        Timbre::Bell => 0.24,
    }
}

fn default_pan(timbre: Timbre, note: u8) -> f32 {
    // Stable per-pitch spread in -0.5..0.5 so chord tones sit at different places.
    let spread = ((note as u32 * 7) % 11) as f32 / 10.0 - 0.5;
    match timbre {
        Timbre::Kick | Timbre::Bass => 0.0,
        Timbre::Snare => 0.05,
        Timbre::Hat | Timbre::OpenHat => 0.3,
        Timbre::Pad => spread * 1.4,
        Timbre::Pluck => spread * 1.2,
        Timbre::Lead => 0.1,
        Timbre::ClassicalLead => spread * 0.45,
        Timbre::Guitar => spread * 0.7,
        Timbre::Strings => spread * 1.1,
        Timbre::ClassicalStrings => spread * 1.25,
        Timbre::Choir => spread * 1.4,
        Timbre::Bell => spread * 1.2,
    }
}

struct Voice {
    timbre: Timbre,
    freq: f32,
    phase: f32,
    phase2: f32,
    t: u32,
    gate: u32,
    total: u32,
    vel: f32,
    gain_l: f32,
    gain_r: f32,
    lp: f32,
    bow_noise: f32,
    pluck_buffer: [f32; 512],
    pluck_len: usize,
    pluck_pos: usize,
    noise: u32,
    sr: f32,
}

impl Voice {
    fn new(
        timbre: Timbre,
        note: u8,
        vel: f32,
        gate_secs: f32,
        pan: f32,
        sr: f32,
        noise_seed: u32,
    ) -> Self {
        let gate = (gate_secs.max(0.01) * sr) as u32;
        let freq = midi_to_hz(note);
        let mut pluck_buffer = [0.0; 512];
        let pluck_len = if timbre == Timbre::Pluck {
            ((sr / freq).round() as usize).clamp(2, pluck_buffer.len())
        } else {
            2
        };
        let mut pluck_noise = noise_seed | 1;
        for sample in &mut pluck_buffer[..pluck_len] {
            pluck_noise ^= pluck_noise << 13;
            pluck_noise ^= pluck_noise >> 17;
            pluck_noise ^= pluck_noise << 5;
            *sample = (pluck_noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
        }
        let total = match timbre {
            Timbre::Kick => (0.4 * sr) as u32,
            Timbre::Snare => (0.3 * sr) as u32,
            Timbre::Hat => (0.1 * sr) as u32,
            Timbre::OpenHat => (0.5 * sr) as u32,
            Timbre::Pluck => (1.25 * sr) as u32,
            Timbre::Bass => gate + (0.08 * sr) as u32,
            Timbre::Pad => gate + (0.9 * sr) as u32,
            Timbre::Lead => gate + (0.18 * sr) as u32,
            Timbre::ClassicalLead => gate + (0.24 * sr) as u32,
            Timbre::Guitar => gate + (0.12 * sr) as u32,
            Timbre::Strings | Timbre::ClassicalStrings => gate + (0.9 * sr) as u32,
            Timbre::Choir => gate + (0.9 * sr) as u32,
            Timbre::Bell => (1.6 * sr) as u32,
        };
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
        Self {
            timbre,
            freq,
            phase: 0.0,
            phase2: 0.37,
            t: 0,
            gate,
            total,
            vel,
            gain_l: angle.cos() * SQRT_2,
            gain_r: angle.sin() * SQRT_2,
            lp: 0.0,
            bow_noise: 0.0,
            pluck_buffer,
            pluck_len,
            pluck_pos: 0,
            noise: noise_seed | 1,
            sr,
        }
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Linear ADSR; `a`, `d`, `r` in seconds, `s` is the sustain level.
    fn adsr(&self, a: f32, d: f32, s: f32, r: f32) -> f32 {
        let t = self.t as f32 / self.sr;
        let gate = self.gate as f32 / self.sr;
        let a = a.max(1e-4);
        let d = d.max(1e-4);
        let level_at = |x: f32| {
            if x < a {
                x / a
            } else if x < a + d {
                1.0 - (1.0 - s) * ((x - a) / d)
            } else {
                s
            }
        };
        if t < gate {
            level_at(t)
        } else {
            let rt = t - gate;
            if rt >= r {
                0.0
            } else {
                level_at(gate) * (1.0 - rt / r)
            }
        }
    }

    fn tick(&mut self) -> f32 {
        if self.t >= self.total {
            return 0.0;
        }
        let dt = 1.0 / self.sr;
        let ts = self.t as f32 * dt;

        let out = match self.timbre {
            Timbre::Kick => {
                let f = 42.0 + 120.0 * (-ts * 28.0).exp();
                self.phase = (self.phase + f * dt).fract();
                let body = (self.phase * TAU).sin() * (-ts * 8.0).exp();
                let click = self.next_noise() * (-ts * 300.0).exp() * 0.3;
                body + click
            }
            Timbre::Snare => {
                let n = self.next_noise();
                self.phase = (self.phase + 185.0 * dt).fract();
                let tone = (self.phase * TAU).sin() * (-ts * 25.0).exp() * 0.5;
                self.lp += 0.35 * (n - self.lp);
                let hp = n - self.lp;
                tone + hp * (-ts * 16.0).exp() * 0.8
            }
            Timbre::Hat | Timbre::OpenHat => {
                let n = self.next_noise();
                self.lp += 0.6 * (n - self.lp);
                let hp = n - self.lp;
                let decay = if self.timbre == Timbre::Hat {
                    70.0
                } else {
                    10.0
                };
                hp * (-ts * decay).exp()
            }
            Timbre::Bass => {
                self.phase = (self.phase + self.freq * dt).fract();
                let saw = 2.0 * self.phase - 1.0;
                let sub = (self.phase * TAU).sin();
                let cutoff = 250.0 + 1200.0 * (-ts * 14.0).exp();
                let a = 1.0 - (-TAU * cutoff * dt).exp();
                self.lp += a * (saw - self.lp);
                (sub * 0.7 + self.lp * 0.6) * self.adsr(0.004, 0.15, 0.6, 0.08)
            }
            Timbre::Pad => {
                // A warm, stable foundation: emphasize the fundamental instead of detuned
                // sawtooths, whose beating and upper harmonics made chords sound brassy.
                self.phase = (self.phase + self.freq * dt).fract();
                let fundamental = (self.phase * TAU).sin();
                let upper = (self.phase * TAU * 2.0).sin();
                let cutoff = 760.0 + 130.0 * (TAU * 0.12 * ts).sin();
                let a = 1.0 - (-TAU * cutoff * dt).exp();
                self.lp += a * (fundamental * 0.86 + upper * 0.14 - self.lp);
                self.lp * self.adsr(0.7, 0.5, 0.82, 1.0)
            }
            Timbre::Pluck => {
                // Karplus–Strong string: a seeded noise burst circulates around a short delay
                // line, while averaging adjacent samples damps the upper partials naturally.
                let next = (self.pluck_pos + 1) % self.pluck_len;
                let string = 0.5
                    * (self.pluck_buffer[self.pluck_pos] + self.pluck_buffer[next]);
                self.pluck_buffer[self.pluck_pos] = string * 0.996;
                self.pluck_pos = next;
                let pick = self.next_noise() * (-ts * 520.0).exp() * 0.035;
                (string + pick) * (-ts * 0.12).exp()
            }
            Timbre::Lead => {
                let vib = 1.0 + 0.004 * (TAU * 5.5 * ts).sin() * (ts * 4.0).min(1.0);
                self.phase = (self.phase + self.freq * vib * dt).fract();
                let saw = 2.0 * self.phase - 1.0;
                let sine = (self.phase * TAU).sin();
                let a = 1.0 - (-TAU * 2600.0 * dt).exp();
                self.lp += a * ((saw * 0.6 + sine * 0.5) - self.lp);
                self.lp * self.adsr(0.02, 0.12, 0.7, 0.18)
            }
            Timbre::ClassicalLead => {
                // A darker, smoother melodic voice for Classical Ambient. A sine-based harmonic
                // blend keeps the high-register line clear without the bright sawtooth edge.
                let vib = 1.0 + 0.0022 * (TAU * 5.0 * ts).sin() * (ts * 3.0).min(1.0);
                self.phase = (self.phase + self.freq * vib * dt).fract();
                let fundamental = (self.phase * TAU).sin();
                let second = (self.phase * TAU * 2.0).sin();
                let third = (self.phase * TAU * 3.0).sin();
                let tone = fundamental * 0.78 + second * 0.18 + third * 0.04;
                let cutoff = 1380.0 + 100.0 * (TAU * 0.18 * ts).sin();
                let a = 1.0 - (-TAU * cutoff * dt).exp();
                self.lp += a * (tone - self.lp);
                self.lp * self.adsr(0.07, 0.16, 0.72, 0.24)
            }
            Timbre::Guitar => {
                // Keep the oscillator in phase: the old detuned second saw created a
                // pronounced beating/bubbly quality that distracted from the heavy tone.
                self.phase = (self.phase + self.freq * dt).fract();
                let saw = 2.0 * self.phase - 1.0;
                let pulse = if self.phase < 0.5 { 1.0 } else { -1.0 };
                let raw = saw * 0.78 + pulse * 0.22;
                // A darker attack and a lower sustained cutoff give the voice more weight.
                let cutoff = 240.0 + 2200.0 * (-ts * 14.0).exp();
                let a = 1.0 - (-TAU * cutoff * dt).exp();
                self.lp += a * (raw - self.lp);
                (self.lp * 5.0).tanh() * self.adsr(0.003, 0.07, 0.72, 0.08)
            }
            Timbre::Strings => {
                // Bowed-string character comes from a soft bow onset, gently blooming vibrato,
                // a changing harmonic balance, and a little deterministic bow-friction noise.
                let vibrato_depth = 0.001 + 0.0025 * (ts * 3.0).min(1.0);
                let vib = 1.0 + vibrato_depth * (TAU * 5.2 * ts).sin();
                self.phase = (self.phase + self.freq * vib * dt).fract();
                self.phase2 = (self.phase2 + self.freq * 1.0015 * dt).fract();
                let fundamental = (self.phase * TAU).sin();
                let upper = (self.phase * TAU * 2.0).sin();
                let body = 2.0 * self.phase - 1.0;
                let companion = 2.0 * self.phase2 - 1.0;
                let brightness = (-ts * 1.8).exp();
                let bowed = fundamental * 0.64
                    + body * (0.12 + 0.08 * brightness)
                    + upper * (0.10 + 0.04 * brightness)
                    + companion * 0.08;
                let a = 1.0 - (-TAU * (1250.0 + 300.0 * brightness) * dt).exp();
                self.lp += a * (bowed - self.lp);
                let envelope = self.adsr(0.2, 0.3, 0.82, 0.82);
                self.bow_noise += 0.12 * (self.next_noise() - self.bow_noise);
                self.lp * envelope + self.bow_noise * envelope * 0.018
            }
            Timbre::ClassicalStrings => {
                // Softer bowed-string approximation reserved for Classical Ambient. Keep the
                // older Strings voice intact because the arpeggio intentionally has a brighter,
                // almost reed-like quality.
                let vib = 1.0 + 0.0018 * (TAU * 5.0 * ts).sin() * (ts * 2.5).min(1.0);
                self.phase = (self.phase + self.freq * vib * dt).fract();
                self.phase2 = (self.phase2 + self.freq * 2.0 * dt).fract();
                let fundamental = (self.phase * TAU).sin();
                let second = (self.phase2 * TAU).sin();
                let third = (self.phase * TAU * 3.0).sin();
                let bowed = fundamental * 0.76 + second * 0.19 + third * 0.05;
                let cutoff = 1250.0 + 180.0 * (TAU * 0.17 * ts).sin();
                let a = 1.0 - (-TAU * cutoff * dt).exp();
                self.lp += a * (bowed - self.lp);
                self.lp * self.adsr(0.24, 0.3, 0.8, 0.86)
            }
            Timbre::Choir => {
                self.phase = (self.phase + self.freq * dt).fract();
                self.phase2 = (self.phase2 + self.freq * 2.01 * dt).fract();
                let fundamental = (self.phase * TAU).sin();
                let upper = (self.phase2 * TAU).sin();
                let formants = fundamental * 0.75 + upper * 0.2 + (fundamental * 2.0).sin() * 0.05;
                let a = 1.0 - (-TAU * 1200.0 * dt).exp();
                self.lp += a * (formants - self.lp);
                self.lp * self.adsr(0.22, 0.32, 0.82, 0.9)
            }
            Timbre::Bell => {
                self.phase = (self.phase + self.freq * dt).fract();
                self.phase2 = (self.phase2 + self.freq * 2.76 * dt).fract();
                let fundamental = (self.phase * TAU).sin();
                let partial = (self.phase2 * TAU).sin();
                (fundamental * 0.75 + partial * 0.25) * (-ts * 2.8).exp()
            }
        };

        self.t += 1;
        out * self.vel * level(self.timbre)
    }
}

/// Cross-fed stereo delay (ping-pong flavour). Melodic and sustained voices can feed it.
struct Delay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    pos_l: usize,
    pos_r: usize,
    feedback: f32,
}

impl Delay {
    fn new(len_l: usize, len_r: usize, feedback: f32) -> Self {
        Self {
            buf_l: vec![0.0; len_l.max(1)],
            buf_r: vec![0.0; len_r.max(1)],
            pos_l: 0,
            pos_r: 0,
            feedback,
        }
    }

    fn process(&mut self, send_l: f32, send_r: f32) -> (f32, f32) {
        let wet_l = self.buf_l[self.pos_l];
        let wet_r = self.buf_r[self.pos_r];
        self.buf_l[self.pos_l] = send_l + wet_r * self.feedback;
        self.buf_r[self.pos_r] = send_r + wet_l * self.feedback;
        self.pos_l = (self.pos_l + 1) % self.buf_l.len();
        self.pos_r = (self.pos_r + 1) % self.buf_r.len();
        (wet_l, wet_r)
    }
}

pub struct Synth {
    sample_rate: f32,
    voices: Vec<Voice>,
    delay: Delay,
    noise_counter: u32,
    master: f32,
}

impl Synth {
    pub fn new(sample_rate: u32, bpm: f32) -> Self {
        let mut synth = Self {
            sample_rate: sample_rate as f32,
            voices: Vec::with_capacity(MAX_VOICES),
            delay: Delay::new(1, 1, 0.35),
            noise_counter: 1,
            master: 0.6,
        };
        synth.set_bpm(bpm);
        synth
    }

    /// Re-time the delay (dotted eighth left, quarter right).
    pub fn set_bpm(&mut self, bpm: f32) {
        let beat = 60.0 / bpm.max(1.0) * self.sample_rate;
        let left = ((beat * 0.75) as usize).max(1);
        let right = (beat as usize).max(1);
        if self.delay.buf_l.len() != left || self.delay.buf_r.len() != right {
            self.delay = Delay::new(left, right, 0.35);
        }
    }

    pub fn note_on(&mut self, timbre: Timbre, note: u8, vel: f32, gate_secs: f32) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0); // steal the oldest
        }
        self.noise_counter = self
            .noise_counter
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let pan = default_pan(timbre, note);
        self.voices.push(Voice::new(
            timbre,
            note,
            vel,
            gate_secs,
            pan,
            self.sample_rate,
            self.noise_counter,
        ));
    }

    pub fn render_frame(&mut self) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        let mut send = 0.0;
        for v in self.voices.iter_mut() {
            let s = v.tick();
            l += s * v.gain_l;
            r += s * v.gain_r;
            if matches!(
                v.timbre,
                Timbre::Pluck
                    | Timbre::Lead
                    | Timbre::ClassicalLead
                    | Timbre::Strings
                    | Timbre::ClassicalStrings
                    | Timbre::Choir
                    | Timbre::Bell
            ) {
                send += s;
            }
        }
        self.voices.retain(|v| v.t < v.total);

        let (wet_l, wet_r) = self.delay.process(send * 0.25, send * 0.25);
        l += wet_l;
        r += wet_r;
        ((l * self.master).tanh(), (r * self.master).tanh())
    }
}
