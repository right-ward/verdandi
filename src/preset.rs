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
    /// Calm, classical soundtrack-inspired ambience with restrained string-led motion.
    ClassicalAmbient,
}

impl MusicPreset {
    // Preserve the indices of the original three presets for seed stability.
    pub const ALL: [Self; 4] = [
        Self::MythicAmbient,
        Self::HeavyMetal,
        Self::LegendaryMetal,
        Self::ClassicalAmbient,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::MythicAmbient => "Mythic Ambient",
            Self::HeavyMetal => "Heavy Metal",
            Self::LegendaryMetal => "Legendary Metal",
            Self::ClassicalAmbient => "Classical Ambient",
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::MythicAmbient => 0,
            Self::HeavyMetal => 1,
            Self::LegendaryMetal => 2,
            Self::ClassicalAmbient => 3,
        }
    }

    pub(crate) const fn from_index(index: usize) -> Self {
        match index {
            1 => Self::HeavyMetal,
            2 => Self::LegendaryMetal,
            3 => Self::ClassicalAmbient,
            _ => Self::MythicAmbient,
        }
    }
}

/// Arrangement pacing. Fast mode is intended for the metal presets; both ambient
/// presets keep their slower tempo ranges regardless of this setting.
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

    pub(crate) const fn index(self) -> usize {
        match self { Self::Standard => 0, Self::Fast => 1 }
    }

    pub(crate) const fn from_index(index: usize) -> Self {
        match index { 1 => Self::Fast, _ => Self::Standard }
    }
}
