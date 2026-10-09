//! Scales plus the "theme" (key, mode, tempo, chord progression) derived from the master seed.
//! The theme is the fixed musical frame; everything generated later stays inside it.

use crate::rng::{derive_seed, Rng};

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleKind {
    Minor,
    Dorian,
    Major,
    Mixolydian,
    Phrygian,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 5] = [
        ScaleKind::Minor,
        ScaleKind::Dorian,
        ScaleKind::Major,
        ScaleKind::Mixolydian,
        ScaleKind::Phrygian,
    ];

    pub fn intervals(self) -> [i32; 7] {
        match self {
            ScaleKind::Minor => [0, 2, 3, 5, 7, 8, 10],
            ScaleKind::Dorian => [0, 2, 3, 5, 7, 9, 10],
            ScaleKind::Major => [0, 2, 4, 5, 7, 9, 11],
            ScaleKind::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
            ScaleKind::Phrygian => [0, 1, 3, 5, 7, 8, 10],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ScaleKind::Minor => "minor",
            ScaleKind::Dorian => "dorian",
            ScaleKind::Major => "major",
            ScaleKind::Mixolydian => "mixolydian",
            ScaleKind::Phrygian => "phrygian",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % Self::ALL.len()]
    }
}

/// Diatonic chord roots (scale degrees, 0-based), one per bar of a 4-bar loop.
pub const PROGRESSIONS: [[i32; 4]; 6] = [
    [0, 5, 2, 6],
    [0, 3, 4, 0],
    [0, 6, 5, 6],
    [0, 2, 5, 4],
    [0, 4, 5, 3],
    [0, 3, 0, 4],
];

/// Pin any part of the theme instead of letting the seed choose it.
#[derive(Clone, Debug, Default)]
pub struct ThemeOverrides {
    /// Tonic pitch class, 0 = C .. 11 = B.
    pub tonic_pitch_class: Option<u8>,
    pub scale: Option<ScaleKind>,
    pub bpm: Option<f32>,
    pub progression: Option<[i32; 4]>,
}

#[derive(Clone, Debug)]
pub struct Theme {
    /// MIDI note of the tonic, always in 36..=47 (low register, used by the bass).
    pub tonic: i32,
    pub scale: ScaleKind,
    pub bpm: f32,
    pub progression: [i32; 4],
}

impl Theme {
    pub fn from_seed(seed: u64, overrides: &ThemeOverrides) -> Self {
        // Always consume the same rolls so a pinned field never shifts the others.
        let mut rng = Rng::new(derive_seed(seed, 0x7468_656D_65, 0));
        let pitch_class = rng.range(0, 12) as i32;
        let scale = ScaleKind::ALL[rng.weighted(&[0.35, 0.25, 0.2, 0.12, 0.08])];
        let bpm = 78.0 + rng.range(0, 43) as f32;
        let progression = PROGRESSIONS[rng.range(0, PROGRESSIONS.len() as u32) as usize];

        Self {
            tonic: 36
                + overrides
                    .tonic_pitch_class
                    .map(|p| (p % 12) as i32)
                    .unwrap_or(pitch_class),
            scale: overrides.scale.unwrap_or(scale),
            bpm: overrides.bpm.unwrap_or(bpm).clamp(40.0, 220.0),
            progression: overrides.progression.unwrap_or(progression),
        }
    }

    /// MIDI note for a scale degree (0 = tonic, 7 = tonic one octave up, negatives go down).
    pub fn midi(&self, degree: i32) -> i32 {
        let intervals = self.scale.intervals();
        let octave = degree.div_euclid(7);
        let idx = degree.rem_euclid(7) as usize;
        self.tonic + octave * 12 + intervals[idx]
    }

    /// Move an in-scale MIDI note by `steps` scale degrees. Out-of-scale notes are returned unchanged.
    pub fn nudge(&self, note: i32, steps: i32) -> i32 {
        for degree in -21..=42 {
            if self.midi(degree) == note {
                return self.midi(degree + steps);
            }
        }
        note
    }

    pub fn key_name(&self) -> &'static str {
        NOTE_NAMES[(self.tonic.rem_euclid(12)) as usize]
    }
}

/// Shift `n` by octaves until it lies in `lo..=hi` (needs `hi - lo >= 11`).
pub fn fold_into(mut n: i32, lo: i32, hi: i32) -> i32 {
    while n < lo {
        n += 12;
    }
    while n > hi {
        n -= 12;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrees_map_to_scale() {
        let t = Theme {
            tonic: 40,
            scale: ScaleKind::Minor,
            bpm: 100.0,
            progression: [0, 0, 0, 0],
        };
        assert_eq!(t.midi(0), 40);
        assert_eq!(t.midi(2), 43);
        assert_eq!(t.midi(7), 52);
        assert_eq!(t.midi(-1), 38);
        assert_eq!(t.nudge(43, 1), 45);
    }

    #[test]
    fn theme_is_deterministic_and_in_range() {
        let a = Theme::from_seed(77, &ThemeOverrides::default());
        let b = Theme::from_seed(77, &ThemeOverrides::default());
        assert_eq!(a.tonic, b.tonic);
        assert_eq!(a.progression, b.progression);
        assert!((36..=47).contains(&a.tonic));
        assert!((78.0..=120.0).contains(&a.bpm));
    }

    #[test]
    fn fold_lands_in_range() {
        for n in 0..128 {
            let f = fold_into(n, 48, 76);
            assert!((48..=76).contains(&f));
            assert_eq!((f - n) % 12, 0);
        }
    }
}
