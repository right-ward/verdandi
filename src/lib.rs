//! # verdandi
//!
//! Layered procedural music driven by a seed. The seed fixes the musical frame (key, mode, tempo,
//! chord progression); each layer (drums, bass, pad, arp, lead) then repeats, mutates or
//! regenerates its pattern every 4-bar loop. Same seed => same evolving track.
//!
//! * `rng`, `theory`, `composer`, `synth`, `engine`: engine-agnostic core (no dependencies).
//! * `plugin` (feature `bevy-plugin`, on by default): Bevy audio source + controller resource.

pub mod composer;
pub mod engine;
pub mod preset;
pub mod rng;
pub mod synth;
pub mod theory;

#[cfg(feature = "bevy-plugin")]
pub mod plugin;

pub mod prelude {
    pub use crate::composer::{Layer, LayerAction, LoopPlan, NoteEvent};
    pub use crate::engine::{Engine, MusicConfig, MusicShared, MusicStatus};
    pub use crate::preset::{MusicPace, MusicPreset};
    pub use crate::theory::{ScaleKind, Theme, ThemeOverrides};

    #[cfg(feature = "bevy-plugin")]
    pub use crate::plugin::{MusicController, MusicTrack, ProceduralMusic, ProceduralMusicPlugin};
}
