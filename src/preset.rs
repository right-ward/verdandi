//! Musical identities and arrangement pacing for procedural tracks.
//!
//! Presets change composition rules and synthesis timbres; they never load audio assets.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MusicPreset {
    /// Spacious, string-led music intended for exploration.
    #[default]
    MythicAmbient,
    /// Riff-led, aggressive metal suitable for combat.
    HeavyMetal,
    /// Heavy metal with orchestral and choir-like layers.
    LegendaryMetal,
}

impl MusicPreset {
    pub const ALL: [Self; 3] = [Self::MythicAmbient, Self::HeavyMetal, Self::LegendaryMetal];

    pub const fn name(self) -> &'static str {
        match self {
            Self::MythicAmbient => "Mythic Ambient",
            Self::HeavyMetal => "Heavy Metal",
            Self::LegendaryMetal => "Legendary Metal",
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::MythicAmbient => 0,
            Self::HeavyMetal => 1,
            Self::LegendaryMetal => 2,
        }
    }
}

/// Arrangement pacing. Fast mode is intended for the metal presets; Mythic Ambient
/// keeps its own slower tempo range regardless of this setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MusicPace {
    #[default]
    Standard,
    Fast,
}

impl MusicPace {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fast => "fast",
        }
    }
}
