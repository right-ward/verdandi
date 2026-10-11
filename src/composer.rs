//! Turns (seed, theme, loop index) into note events for each layer.
//!
//! Each loop, every layer independently decides to *repeat* its last pattern, *mutate* it
//! slightly, or *regenerate* it from scratch. All randomness comes from sub-seeds derived from
//! (master seed, layer, loop index), so the same seed always yields the same evolving track.

use crate::preset::{MusicPace, MusicPreset};
use crate::rng::{derive_seed, Rng};
use crate::synth::Timbre;
use crate::theory::{fold_into, Theme};

pub const STEPS_PER_BAR: u32 = 16; // sixteenth notes
pub const BARS_PER_LOOP: u32 = 4; // one pass through the chord progression
pub const STEPS_PER_LOOP: u32 = STEPS_PER_BAR * BARS_PER_LOOP;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Drums,
    Bass,
    Pad,
    Arp,
    Lead,
    RhythmGuitar,
    Strings,
    Choir,
}

impl Layer {
    pub const COUNT: usize = 8;
    pub const ALL: [Layer; 8] = [
        Layer::Drums,
        Layer::Bass,
        Layer::Pad,
        Layer::Arp,
        Layer::Lead,
        Layer::RhythmGuitar,
        Layer::Strings,
        Layer::Choir,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Layer::Drums => "Drums",
            Layer::Bass => "Bass",
            Layer::Pad => "Pad",
            Layer::Arp => "Arp",
            Layer::Lead => "Lead",
            Layer::RhythmGuitar => "Rhythm Guitar",
            Layer::Strings => "Strings",
            Layer::Choir => "Choir",
        }
    }

    /// Intensity (0..=1) at which this layer becomes audible.
    pub fn threshold(self) -> f32 {
        match self {
            Layer::Pad => 0.0,
            Layer::Bass => 0.2,
            Layer::Drums => 0.35,
            Layer::Arp => 0.55,
            Layer::Lead => 0.75,
            Layer::RhythmGuitar => 0.3,
            Layer::Strings => 0.2,
            Layer::Choir => 0.35,
        }
    }

    /// Whether this layer participates in the selected preset.
    pub fn used_by(self, preset: MusicPreset) -> bool {
        match preset {
            MusicPreset::MythicAmbient => matches!(
                self,
                Layer::Bass | Layer::Pad | Layer::Arp | Layer::Lead | Layer::Strings
            ),
            MusicPreset::HeavyMetal => matches!(
                self,
                Layer::Drums | Layer::Bass | Layer::Lead | Layer::RhythmGuitar
            ),
            MusicPreset::LegendaryMetal => true,
            MusicPreset::ClassicalAmbient => matches!(
                self,
                Layer::Bass | Layer::Pad | Layer::Arp | Layer::Lead | Layer::Strings
            ),
        }
    }

    fn tag(self) -> u64 {
        0x100 + self as u64
    }

    /// (probability of repeating, probability of mutating); the remainder regenerates.
    fn volatility(self) -> (f32, f32) {
        match self {
            Layer::Drums => (0.5, 0.35),
            Layer::Bass => (0.35, 0.4),
            Layer::Pad => (0.3, 0.0),
            Layer::Arp => (0.3, 0.4),
            Layer::Lead => (0.2, 0.4),
            Layer::RhythmGuitar => (0.45, 0.35),
            Layer::Strings => (0.45, 0.2),
            Layer::Choir => (0.5, 0.2),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    /// Sixteenth-note step within the loop, `0..STEPS_PER_LOOP`.
    pub step: u32,
    /// Length in steps (ignored by percussive timbres).
    pub len: u32,
    /// MIDI note (unused by drums).
    pub note: u8,
    pub vel: f32,
    pub timbre: Timbre,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayerAction {
    #[default]
    Regenerate,
    Repeat,
    Mutate,
}

impl LayerAction {
    pub fn to_bits(self) -> u32 {
        self as u32
    }

    pub fn from_bits(bits: u32) -> Self {
        match bits {
            1 => LayerAction::Repeat,
            2 => LayerAction::Mutate,
            _ => LayerAction::Regenerate,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LoopPlan {
    pub loop_index: u64,
    pub layers: [Vec<NoteEvent>; Layer::COUNT],
    pub actions: [LayerAction; Layer::COUNT],
}

pub struct Composer {
    seed: u64,
    theme: Theme,
    next_loop: u64,
    last: [Vec<NoteEvent>; Layer::COUNT],
    preset: Option<MusicPreset>,
    pace: MusicPace,
}

impl Composer {
    /// Create a composer with the original general-purpose procedural style.
    pub fn new(seed: u64, theme: Theme) -> Self {
        Self {
            seed,
            theme,
            next_loop: 0,
            last: Default::default(),
            preset: None,
            pace: MusicPace::Standard,
        }
    }

    /// Create a composer using one of Verdandi's built-in musical presets.
    pub fn with_preset(seed: u64, theme: Theme, preset: MusicPreset, pace: MusicPace) -> Self {
        Self {
            preset: Some(preset),
            pace,
            ..Self::new(seed, theme)
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Plan the next loop. Call once per loop, in order.
    pub fn next_loop(&mut self) -> LoopPlan {
        let loop_index = self.next_loop;
        self.next_loop += 1;
        let mut plan = LoopPlan {
            loop_index,
            ..Default::default()
        };

        for layer in Layer::ALL {
            let li = layer.index();
            let mut rng = Rng::new(derive_seed(self.seed, layer.tag(), loop_index));
            let (p_repeat, p_mutate) = layer.volatility();
            let roll = rng.next_f32();

            let mut action = if loop_index == 0 || self.last[li].is_empty() {
                LayerAction::Regenerate
            } else if roll < p_repeat {
                LayerAction::Repeat
            } else if roll < p_repeat + p_mutate {
                LayerAction::Mutate
            } else {
                LayerAction::Regenerate
            };
            if layer == Layer::Pad && action == LayerAction::Mutate {
                action = LayerAction::Regenerate;
            }

            let events = match action {
                LayerAction::Repeat => self.last[li].clone(),
                LayerAction::Mutate => mutate(&self.last[li], layer, &self.theme, &mut rng),
                LayerAction::Regenerate => {
                    generate(layer, &self.theme, &mut rng, self.preset, self.pace)
                }
            };
            self.last[li] = events.clone();
            plan.layers[li] = events;
            plan.actions[li] = action;
        }
        plan
    }
}

fn generate(
    layer: Layer,
    theme: &Theme,
    rng: &mut Rng,
    preset: Option<MusicPreset>,
    pace: MusicPace,
) -> Vec<NoteEvent> {
    if let Some(preset) = preset {
        return generate_for_preset(layer, theme, rng, preset, pace);
    }

    match layer {
        Layer::Drums => gen_drums(rng),
        Layer::Bass => gen_bass(theme, rng),
        Layer::Pad => gen_pad(theme, rng),
        Layer::Arp => gen_arp(theme, rng),
        Layer::Lead => gen_lead_with_timbre(theme, rng, Timbre::Lead),
        Layer::RhythmGuitar | Layer::Strings | Layer::Choir => Vec::new(),
    }
}

fn generate_for_preset(
    layer: Layer,
    theme: &Theme,
    rng: &mut Rng,
    preset: MusicPreset,
    pace: MusicPace,
) -> Vec<NoteEvent> {
    match layer {
        Layer::Drums => match preset {
            MusicPreset::MythicAmbient | MusicPreset::ClassicalAmbient => Vec::new(),
            MusicPreset::HeavyMetal | MusicPreset::LegendaryMetal => gen_metal_drums(rng, pace),
        },
        Layer::Bass => match preset {
            MusicPreset::MythicAmbient | MusicPreset::ClassicalAmbient => gen_ambient_bass(theme),
            MusicPreset::HeavyMetal | MusicPreset::LegendaryMetal => {
                gen_metal_bass(theme, rng, pace)
            }
        },
        Layer::Pad => match preset {
            MusicPreset::MythicAmbient
            | MusicPreset::LegendaryMetal
            | MusicPreset::ClassicalAmbient => gen_pad(theme, rng),
            MusicPreset::HeavyMetal => Vec::new(),
        },
        Layer::Arp => match preset {
            MusicPreset::MythicAmbient | MusicPreset::LegendaryMetal => gen_ambient_arp(theme, rng),
            MusicPreset::ClassicalAmbient => gen_classical_arp(theme, rng),
            MusicPreset::HeavyMetal => Vec::new(),
        },
        Layer::Lead => match preset {
            MusicPreset::MythicAmbient => gen_lead_with_timbre(theme, rng, Timbre::Strings),
            MusicPreset::ClassicalAmbient => {
                gen_lead_with_timbre(theme, rng, Timbre::ClassicalLead)
            }
            MusicPreset::HeavyMetal | MusicPreset::LegendaryMetal => {
                gen_lead_with_timbre(theme, rng, Timbre::Guitar)
            }
        },
        Layer::RhythmGuitar => match preset {
            MusicPreset::MythicAmbient | MusicPreset::ClassicalAmbient => Vec::new(),
            MusicPreset::HeavyMetal | MusicPreset::LegendaryMetal => {
                gen_rhythm_guitar(theme, rng, pace)
            }
        },
        Layer::Strings => match preset {
            MusicPreset::MythicAmbient
            | MusicPreset::LegendaryMetal
            | MusicPreset::ClassicalAmbient => {
                gen_strings(theme, rng, preset == MusicPreset::ClassicalAmbient)
            }
            MusicPreset::HeavyMetal => Vec::new(),
        },
        Layer::Choir => match preset {
            MusicPreset::LegendaryMetal => gen_choir(theme),
            MusicPreset::MythicAmbient
            | MusicPreset::HeavyMetal
            | MusicPreset::ClassicalAmbient => Vec::new(),
        },
    }
}

fn gen_ambient_bass(theme: &Theme) -> Vec<NoteEvent> {
    (0..BARS_PER_LOOP)
        .map(|bar| NoteEvent {
            step: bar * STEPS_PER_BAR,
            len: STEPS_PER_BAR,
            note: to_u8(fold_into(
                theme.midi(theme.progression[bar as usize]),
                31,
                48,
            )),
            vel: 0.32,
            timbre: Timbre::Bass,
        })
        .collect()
}

fn gen_metal_bass(theme: &Theme, rng: &mut Rng, pace: MusicPace) -> Vec<NoteEvent> {
    const STANDARD_RHYTHMS: &[&[u32]] = &[
        &[0, 4, 8, 12],
        &[0, 3, 6, 8, 11, 12],
        &[0, 2, 4, 6, 8, 10, 12, 14],
    ];
    const FAST_RHYTHMS: &[&[u32]] = &[
        &[0, 2, 4, 6, 8, 10, 12, 14],
        &[0, 1, 4, 5, 8, 9, 12, 13],
        &[0, 3, 4, 7, 8, 11, 12, 15],
    ];
    let rhythm = if pace == MusicPace::Fast {
        *rng.pick(FAST_RHYTHMS)
    } else {
        *rng.pick(STANDARD_RHYTHMS)
    };
    let mut out = Vec::new();
    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        for (i, &step) in rhythm.iter().enumerate() {
            let next = rhythm.get(i + 1).copied().unwrap_or(STEPS_PER_BAR);
            let degree = if i > 0 && rng.chance(0.22) {
                root + 4
            } else {
                root
            };
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR + step,
                len: (next - step).clamp(1, 4),
                note: to_u8(fold_into(theme.midi(degree), 28, 50)),
                vel: if step % 4 == 0 { 0.88 } else { 0.68 },
                timbre: Timbre::Bass,
            });
        }
    }
    out
}

fn gen_metal_drums(rng: &mut Rng, pace: MusicPace) -> Vec<NoteEvent> {
    const STANDARD_KICKS: &[&[u32]] = &[&[0, 4, 8, 12], &[0, 6, 8, 12], &[0, 3, 8, 11]];
    const FAST_KICKS: &[&[u32]] = &[
        &[0, 2, 4, 6, 8, 10, 12, 14],
        &[0, 2, 4, 7, 8, 10, 12, 15],
        &[0, 3, 4, 6, 8, 11, 12, 14],
    ];
    const SNARES: &[&[u32]] = &[&[4, 12], &[4, 12], &[4, 10, 12]];
    let kicks = if pace == MusicPace::Fast {
        *rng.pick(FAST_KICKS)
    } else {
        *rng.pick(STANDARD_KICKS)
    };
    let snare = *rng.pick(SNARES);
    let hat_step = if pace == MusicPace::Fast { 1 } else { 2 };
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        let base = bar * STEPS_PER_BAR;
        for &step in kicks {
            out.push(hit(
                base + step,
                Timbre::Kick,
                if step % 4 == 0 { 1.0 } else { 0.8 },
            ));
        }
        for &step in snare {
            out.push(hit(base + step, Timbre::Snare, 0.9));
        }
        if bar == BARS_PER_LOOP - 1 && rng.chance(0.75) {
            for (i, step) in [13u32, 14, 15].into_iter().enumerate() {
                if rng.chance(0.65) {
                    out.push(hit(base + step, Timbre::Snare, 0.45 + 0.12 * i as f32));
                }
            }
        }
        let mut step = 0;
        while step < STEPS_PER_BAR {
            if !rng.chance(if pace == MusicPace::Fast { 0.04 } else { 0.09 }) {
                let timbre = if step == 14 && rng.chance(0.55) {
                    Timbre::OpenHat
                } else {
                    Timbre::Hat
                };
                let velocity = if step % 4 == 0 { 0.72 } else { 0.45 };
                out.push(hit(base + step, timbre, velocity));
            }
            step += hat_step;
        }
    }
    out
}

fn gen_rhythm_guitar(theme: &Theme, rng: &mut Rng, pace: MusicPace) -> Vec<NoteEvent> {
    const STANDARD_RIFFS: &[&[u32]] = &[
        &[0, 4, 7, 8, 12],
        &[0, 3, 6, 8, 11, 12],
        &[0, 2, 4, 6, 8, 10, 12, 14],
    ];
    const FAST_RIFFS: &[&[u32]] = &[
        &[0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14],
        &[0, 2, 3, 4, 6, 7, 8, 10, 11, 12, 14, 15],
        &[0, 2, 4, 6, 8, 10, 12, 14],
    ];
    let riff = if pace == MusicPace::Fast {
        *rng.pick(FAST_RIFFS)
    } else {
        *rng.pick(STANDARD_RIFFS)
    };
    let mut out = Vec::new();
    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        for (i, &step) in riff.iter().enumerate() {
            if rng.chance(if pace == MusicPace::Fast { 0.08 } else { 0.16 }) {
                continue;
            }
            let next = riff.get(i + 1).copied().unwrap_or(STEPS_PER_BAR);
            let len = (next - step).clamp(1, 3);
            // Keep the root and perfect fifth within the guitar register as a true power chord.
            let root_note = fold_into(theme.midi(root), 36, 52);
            let fifth_note = root_note + 7;
            for (note, velocity) in [(root_note, 0.82), (fifth_note, 0.7)] {
                out.push(NoteEvent {
                    step: bar * STEPS_PER_BAR + step,
                    len,
                    note: to_u8(note),
                    vel: velocity,
                    timbre: Timbre::Guitar,
                });
            }
        }
    }
    out
}

/// A restrained, string-led ostinato for the more classical ambient preset.
fn gen_classical_arp(theme: &Theme, rng: &mut Rng) -> Vec<NoteEvent> {
    const PATTERNS: &[[i32; 4]] = &[[0, 2, 4, 2], [0, 4, 2, 4], [4, 2, 0, 2]];
    let pattern = *rng.pick(PATTERNS);
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        for (i, step) in [0u32, 4, 8, 12].into_iter().enumerate() {
            if i > 0 && rng.chance(0.12) {
                continue;
            }
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR + step,
                len: 3,
                note: to_u8(fold_into(theme.midi(7 + root + pattern[i]), 52, 76)),
                vel: if step == 0 { 0.3 } else { 0.22 },
                timbre: Timbre::Strings,
            });
        }
    }
    out
}

fn gen_ambient_arp(theme: &Theme, rng: &mut Rng) -> Vec<NoteEvent> {
    let pattern = [0, 2, 4, 7, 4, 2, 7, 4];
    let mut out = Vec::new();
    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        for (i, &step) in [0u32, 4, 8, 12].iter().enumerate() {
            if rng.chance(0.2) {
                continue;
            }
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR + step,
                len: 3,
                note: to_u8(fold_into(theme.midi(7 + root + pattern[i]), 57, 88)),
                vel: if step == 0 { 0.4 } else { 0.28 },
                timbre: Timbre::Bell,
            });
        }
    }
    out
}

fn gen_strings(theme: &Theme, rng: &mut Rng, classical: bool) -> Vec<NoteEvent> {
    let mut out = Vec::new();
    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        let mut degrees = vec![root, root + 2, root + 4];
        if rng.chance(0.45) {
            degrees.push(root + 6);
        }
        for degree in degrees {
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR,
                len: STEPS_PER_BAR,
                note: to_u8(fold_into(theme.midi(7 + degree), 50, 82)),
                vel: if bar == 0 { 0.36 } else { 0.32 },
                timbre: if classical {
                    Timbre::ClassicalStrings
                } else {
                    Timbre::Strings
                },
            });
        }
    }
    out
}

fn gen_choir(theme: &Theme) -> Vec<NoteEvent> {
    let mut out = Vec::new();
    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        for degree in [root, root + 2, root + 4, root + 6] {
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR,
                len: STEPS_PER_BAR,
                note: to_u8(fold_into(theme.midi(14 + degree), 60, 86)),
                vel: 0.28,
                timbre: Timbre::Choir,
            });
        }
    }
    out
}

fn to_u8(n: i32) -> u8 {
    n.clamp(0, 127) as u8
}

fn hit(step: u32, timbre: Timbre, vel: f32) -> NoteEvent {
    NoteEvent {
        step,
        len: 1,
        note: 0,
        vel: vel.clamp(0.05, 1.0),
        timbre,
    }
}

fn gen_drums(rng: &mut Rng) -> Vec<NoteEvent> {
    const KICKS: &[&[u32]] = &[
        &[0, 4, 8, 12],
        &[0, 6, 10],
        &[0, 7, 10],
        &[0, 3, 8, 11],
        &[0, 10],
    ];
    const SNARES: &[&[u32]] = &[&[4, 12], &[4, 12], &[12], &[4, 12, 15]];

    let kick = *rng.pick(KICKS);
    let snare = *rng.pick(SNARES);
    let hat_div: u32 = if rng.chance(0.5) { 1 } else { 2 };
    let open_hat = rng.chance(0.5);
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        let base = bar * STEPS_PER_BAR;

        for &s in kick {
            out.push(hit(
                base + s,
                Timbre::Kick,
                0.95 + (rng.next_f32() - 0.5) * 0.1,
            ));
        }
        if rng.chance(0.2) {
            let s = *rng.pick(&[3u32, 7, 10, 11, 15]);
            out.push(hit(base + s, Timbre::Kick, 0.6));
        }

        for &s in snare {
            out.push(hit(base + s, Timbre::Snare, 0.85));
        }
        if bar == BARS_PER_LOOP - 1 && rng.chance(0.6) {
            for (k, s) in [13u32, 14, 15].into_iter().enumerate() {
                if rng.chance(0.7) {
                    out.push(hit(base + s, Timbre::Snare, 0.5 + 0.12 * k as f32));
                }
            }
        }

        let mut s = 0;
        while s < STEPS_PER_BAR {
            if !rng.chance(0.08) {
                let accent = if s % 4 == 0 {
                    0.6
                } else if s % 2 == 0 {
                    0.45
                } else {
                    0.3
                };
                let timbre = if open_hat && s == 14 && rng.chance(0.6) {
                    Timbre::OpenHat
                } else {
                    Timbre::Hat
                };
                out.push(hit(
                    base + s,
                    timbre,
                    accent + (rng.next_f32() - 0.5) * 0.12,
                ));
            }
            s += hat_div;
        }
    }
    out
}

fn gen_bass(theme: &Theme, rng: &mut Rng) -> Vec<NoteEvent> {
    const RHYTHMS: &[&[u32]] = &[
        &[0],
        &[0, 8],
        &[0, 6, 8, 14],
        &[0, 3, 6, 8, 11],
        &[0, 2, 4, 6, 8, 10, 12, 14],
        &[0, 7, 10],
    ];
    let rhythm = *rng.pick(RHYTHMS);
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        let root = theme.progression[bar as usize];
        for (i, &s) in rhythm.iter().enumerate() {
            let next = rhythm.get(i + 1).copied().unwrap_or(STEPS_PER_BAR);
            let len = (next - s).clamp(1, 8);
            let degree = if i == 0 {
                root
            } else {
                match rng.weighted(&[0.65, 0.15, 0.2]) {
                    0 => root,
                    1 => root + 4,
                    _ => root + 7,
                }
            };
            let note = fold_into(theme.midi(degree), 31, 55);
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR + s,
                len,
                note: to_u8(note),
                vel: if i == 0 { 0.85 } else { 0.65 },
                timbre: Timbre::Bass,
            });
        }
    }
    out
}

fn gen_pad(theme: &Theme, rng: &mut Rng) -> Vec<NoteEvent> {
    let add_seventh = rng.chance(0.35);
    let open_voicing = rng.chance(0.5);
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        let r = theme.progression[bar as usize];
        let mut degrees = vec![r, r + 2, r + 4];
        if add_seventh {
            degrees.push(r + 6);
        }
        if open_voicing {
            degrees[0] += 7;
        }
        for d in degrees {
            let note = fold_into(theme.midi(7 + d), 48, 76);
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR,
                len: STEPS_PER_BAR,
                note: to_u8(note),
                vel: 0.55,
                timbre: Timbre::Pad,
            });
        }
    }
    out
}

fn gen_arp(theme: &Theme, rng: &mut Rng) -> Vec<NoteEvent> {
    let interval: u32 = if rng.chance(0.5) { 1 } else { 2 };
    let pattern: [usize; 8] = match rng.range(0, 4) {
        0 => [0, 1, 2, 3, 0, 1, 2, 3],
        1 => [3, 2, 1, 0, 3, 2, 1, 0],
        2 => [0, 1, 2, 3, 2, 1, 0, 1],
        _ => [0, 2, 1, 3, 0, 2, 1, 3],
    };
    let dropout = 0.05 + 0.2 * rng.next_f32();
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        let r = theme.progression[bar as usize];
        let tones = [r, r + 2, r + 4, r + 7];
        let mut n = 0usize;
        let mut s = 0;
        while s < STEPS_PER_BAR {
            if !rng.chance(dropout) {
                let degree = 7 + tones[pattern[n % 8]];
                let note = fold_into(theme.midi(degree), 57, 84);
                out.push(NoteEvent {
                    step: bar * STEPS_PER_BAR + s,
                    len: interval,
                    note: to_u8(note),
                    vel: if s % 4 == 0 { 0.7 } else { 0.5 },
                    timbre: Timbre::Pluck,
                });
            }
            n += 1;
            s += interval;
        }
    }
    out
}

/// A melodic cell: rhythm plus pitch relative to the current chord root (in scale degrees).
#[derive(Clone, Copy)]
struct MotifNote {
    step: u32,
    len: u32,
    rel: i32,
}

fn gen_motif(rng: &mut Rng) -> Vec<MotifNote> {
    const ONSETS: &[&[u32]] = &[
        &[0, 4, 8, 12],
        &[0, 2, 4, 8, 10, 12],
        &[0, 3, 6, 10, 12],
        &[0, 4, 6, 8, 12, 14],
        &[0, 6, 8, 14],
        &[0, 3, 4, 8, 11, 12],
    ];
    // Chord tones relative to the chord root (root, 3rd, 5th, plus octave neighbours).
    const CHORD_TONES: [i32; 5] = [-3, 0, 2, 4, 7];
    const MOVES: [i32; 5] = [-2, -1, 0, 1, 2];
    const MOVE_WEIGHTS: [f32; 5] = [0.15, 0.35, 0.1, 0.35, 0.15];

    let pool = *rng.pick(ONSETS);
    let mut onsets: Vec<u32> = Vec::new();
    for (i, &s) in pool.iter().enumerate() {
        if i == 0 || !rng.chance(0.15) {
            onsets.push(s);
        }
    }

    let mut motif = Vec::with_capacity(onsets.len());
    let mut prev = 0i32;
    for (i, &s) in onsets.iter().enumerate() {
        let next = onsets.get(i + 1).copied().unwrap_or(STEPS_PER_BAR);
        let len = (next - s).clamp(1, 8);
        let rel = if s % 4 == 0 {
            // Strong beat: land on a chord tone, preferring ones near the previous note.
            let mut w = [0f32; 5];
            for (k, &c) in CHORD_TONES.iter().enumerate() {
                w[k] = 1.0 / (1.0 + (c - prev).abs() as f32);
            }
            CHORD_TONES[rng.weighted(&w)]
        } else {
            // Weak beat: stepwise motion.
            (prev + MOVES[rng.weighted(&MOVE_WEIGHTS)]).clamp(-3, 8)
        };
        prev = rel;
        motif.push(MotifNote { step: s, len, rel });
    }
    motif
}

fn gen_lead_with_timbre(theme: &Theme, rng: &mut Rng, timbre: Timbre) -> Vec<NoteEvent> {
    let motif = gen_motif(rng);
    let mut out = Vec::new();

    for bar in 0..BARS_PER_LOOP {
        if bar > 0 && rng.chance(0.2) {
            continue; // let it breathe
        }
        let root = theme.progression[bar as usize];
        let mut notes = motif.clone();

        if bar == 2 && notes.len() > 1 {
            let i = rng.range(1, notes.len() as u32) as usize;
            notes[i].rel += if rng.chance(0.5) { 1 } else { -1 };
        }
        if bar == BARS_PER_LOOP - 1 {
            // Cadence: keep the first half of the motif, then resolve on a long chord root.
            #[allow(clippy::manual_div_ceil)]
            let keep = (notes.len() + 1) / 2;
            notes.truncate(keep);
            if let Some(last) = notes.last_mut() {
                last.rel = 0;
                last.len = (STEPS_PER_BAR - last.step).min(12);
            }
        }

        for m in notes {
            let note = fold_into(theme.midi(9 + root + m.rel), 62, 88);
            out.push(NoteEvent {
                step: bar * STEPS_PER_BAR + m.step,
                len: m.len,
                note: to_u8(note),
                vel: 0.6 + (rng.next_f32() - 0.5) * 0.2,
                timbre,
            });
        }
    }
    out
}

/// Small variation of an existing pattern. Strong beats (every 4th step) are left alone so the
/// groove and harmony stay recognisable.
fn mutate(events: &[NoteEvent], layer: Layer, theme: &Theme, rng: &mut Rng) -> Vec<NoteEvent> {
    let mut out = Vec::with_capacity(events.len() + 2);
    for ev in events {
        let mut e = *ev;
        let strong = ev.step % 4 == 0;
        match layer {
            Layer::Drums => {
                if ev.timbre == Timbre::Hat && !strong && rng.chance(0.2) {
                    continue;
                }
            }
            Layer::Bass | Layer::Arp | Layer::Lead | Layer::RhythmGuitar => {
                if !strong {
                    if rng.chance(0.12) {
                        continue;
                    }
                    if layer != Layer::Bass && layer != Layer::RhythmGuitar && rng.chance(0.25) {
                        let dir = if rng.chance(0.5) { 1 } else { -1 };
                        e.note = to_u8(theme.nudge(ev.note as i32, dir));
                    }
                }
            }
            Layer::Pad | Layer::Strings | Layer::Choir => {}
        }
        out.push(e);
    }

    if layer == Layer::Drums && rng.chance(0.5) {
        let bar = rng.range(0, BARS_PER_LOOP);
        let s = *rng.pick(&[7u32, 9, 10, 15]);
        out.push(hit(bar * STEPS_PER_BAR + s, Timbre::Snare, 0.3)); // ghost snare
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::ThemeOverrides;

    fn composer(seed: u64) -> Composer {
        Composer::new(seed, Theme::from_seed(seed, &ThemeOverrides::default()))
    }

    #[test]
    fn same_seed_same_music() {
        let (mut a, mut b) = (composer(7), composer(7));
        for _ in 0..8 {
            assert_eq!(a.next_loop().layers, b.next_loop().layers);
        }
    }

    #[test]
    fn different_seeds_differ() {
        let (mut a, mut b) = (composer(1), composer(2));
        assert_ne!(a.next_loop().layers, b.next_loop().layers);
    }

    #[test]
    fn loops_vary_over_time() {
        let mut c = composer(5);
        let first = c.next_loop().layers;
        assert!((0..12).any(|_| c.next_loop().layers != first));
    }

    #[test]
    fn preset_layer_membership_matches_arrangement() {
        assert!(Layer::Strings.used_by(MusicPreset::MythicAmbient));
        assert!(!Layer::Drums.used_by(MusicPreset::MythicAmbient));
        assert!(Layer::RhythmGuitar.used_by(MusicPreset::HeavyMetal));
        assert!(!Layer::Choir.used_by(MusicPreset::HeavyMetal));
        assert!(Layer::Choir.used_by(MusicPreset::LegendaryMetal));
    }

    #[test]
    fn presets_generate_distinct_layer_families() {
        let seed = 91;
        let ambient_theme = Theme::from_seed_for_preset(
            seed,
            &ThemeOverrides::default(),
            MusicPreset::MythicAmbient,
            MusicPace::Standard,
        );
        let metal_theme = Theme::from_seed_for_preset(
            seed,
            &ThemeOverrides::default(),
            MusicPreset::HeavyMetal,
            MusicPace::Standard,
        );
        let legendary_theme = Theme::from_seed_for_preset(
            seed,
            &ThemeOverrides::default(),
            MusicPreset::LegendaryMetal,
            MusicPace::Standard,
        );

        let ambient = Composer::with_preset(
            seed,
            ambient_theme,
            MusicPreset::MythicAmbient,
            MusicPace::Standard,
        )
        .next_loop();
        let metal = Composer::with_preset(
            seed,
            metal_theme,
            MusicPreset::HeavyMetal,
            MusicPace::Standard,
        )
        .next_loop();
        let legendary = Composer::with_preset(
            seed,
            legendary_theme,
            MusicPreset::LegendaryMetal,
            MusicPace::Standard,
        )
        .next_loop();

        assert!(ambient.layers[Layer::Drums.index()].is_empty());
        assert!(!ambient.layers[Layer::Strings.index()].is_empty());
        assert!(!ambient.layers[Layer::Arp.index()].is_empty());
        assert!(!ambient.layers[Layer::Lead.index()].is_empty());
        assert!(ambient.layers[Layer::Lead.index()]
            .iter()
            .all(|event| event.timbre == Timbre::Strings));
        assert!(!metal.layers[Layer::Drums.index()].is_empty());
        assert!(!metal.layers[Layer::RhythmGuitar.index()].is_empty());
        assert!(metal.layers[Layer::Choir.index()].is_empty());
        assert!(!legendary.layers[Layer::Strings.index()].is_empty());
        assert!(!legendary.layers[Layer::Choir.index()].is_empty());
        assert!(!legendary.layers[Layer::RhythmGuitar.index()].is_empty());
    }

    #[test]
    fn classical_ambient_uses_string_led_accompaniment_without_drums() {
        let seed = 93;
        let theme = Theme::from_seed_for_preset(
            seed,
            &ThemeOverrides::default(),
            MusicPreset::ClassicalAmbient,
            MusicPace::Standard,
        );
        let plan = Composer::with_preset(
            seed,
            theme,
            MusicPreset::ClassicalAmbient,
            MusicPace::Standard,
        )
        .next_loop();

        assert!(plan.layers[Layer::Drums.index()].is_empty());
        assert!(plan.layers[Layer::RhythmGuitar.index()].is_empty());
        assert!(plan.layers[Layer::Choir.index()].is_empty());
        for layer in [
            Layer::Bass,
            Layer::Pad,
            Layer::Arp,
            Layer::Lead,
            Layer::Strings,
        ] {
            assert!(
                !plan.layers[layer.index()].is_empty(),
                "{layer:?} should be used"
            );
        }
        assert!(plan.layers[Layer::Arp.index()]
            .iter()
            .all(|event| event.timbre == Timbre::Strings));
        assert!(plan.layers[Layer::Lead.index()]
            .iter()
            .all(|event| event.timbre == Timbre::ClassicalLead));
        assert!(plan.layers[Layer::Strings.index()]
            .iter()
            .all(|event| event.timbre == Timbre::ClassicalStrings));
    }

    #[test]
    fn events_stay_in_bounds() {
        let mut c = composer(99);
        for _ in 0..16 {
            let plan = c.next_loop();
            for layer in &plan.layers {
                for ev in layer {
                    assert!(ev.step < STEPS_PER_LOOP);
                    assert!(ev.len >= 1);
                    assert!(ev.note <= 127);
                    assert!((0.0..=1.0).contains(&ev.vel));
                }
            }
        }
    }
}
