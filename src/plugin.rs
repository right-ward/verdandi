//! Bevy integration (written against Bevy 0.20's `Decodable` API).
//!
//! The music engine runs inside a custom audio source, i.e. on Bevy's audio thread. Game systems
//! steer it through the `MusicController` resource, which only touches atomics.
//!
//! Add `DefaultPlugins` (or at least the asset + audio plugins) *before* `ProceduralMusicPlugin`.

use std::sync::Arc;
use std::time::Duration;

use bevy::audio::{AddAudioSource, ChannelCount, SampleRate, Source};
use bevy::prelude::*;
use bevy::reflect::TypePath;

use crate::composer::Layer;
use crate::engine::{Engine, MusicConfig, MusicShared, MusicStatus};

const CHUNK_FRAMES: usize = 512;

/// The audio asset. Spawn exactly one `AudioPlayer` for it (the plugin does this for you
/// when `autoplay` is true).
#[derive(Asset, TypePath)]
pub struct ProceduralMusic {
    shared: Arc<MusicShared>,
    config: MusicConfig,
}

impl ProceduralMusic {
    pub fn new(config: MusicConfig, shared: Arc<MusicShared>) -> Self {
        Self { shared, config }
    }
}

pub struct MusicDecoder {
    engine: Engine,
    buf: Vec<f32>,
    pos: usize,
    sample_rate: u32,
}

impl Iterator for MusicDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            self.engine.render_interleaved(&mut self.buf);
            self.pos = 0;
        }
        let sample = self.buf[self.pos];
        self.pos += 1;
        Some(sample) // endless stream
    }
}

impl Source for MusicDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.sample_rate).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for ProceduralMusic {
    type Decoder = MusicDecoder;

    fn decoder(&self) -> Self::Decoder {
        let frames = CHUNK_FRAMES * 2;
        MusicDecoder {
            engine: Engine::new(self.shared.clone(), &self.config),
            buf: vec![0.0; frames],
            pos: frames, // forces a render on the first sample
            sample_rate: self.config.sample_rate,
        }
    }
}

/// Marker on the entity playing the music. Query its `AudioSink` to pause or change volume.
#[derive(Component)]
pub struct MusicTrack;

/// Game-side handle to the running music.
#[derive(Resource, Clone)]
pub struct MusicController {
    shared: Arc<MusicShared>,
}

impl MusicController {
    pub fn shared(&self) -> &Arc<MusicShared> {
        &self.shared
    }

    /// Queue a new seed; it takes over at the next loop boundary.
    pub fn set_seed(&self, seed: u64) {
        self.shared.request_seed(seed);
    }

    /// The seed playing now, or the queued one.
    pub fn seed(&self) -> u64 {
        self.shared.target_seed()
    }

    pub fn set_layer_enabled(&self, layer: Layer, on: bool) {
        self.shared.set_layer_enabled(layer, on);
    }

    pub fn layer_enabled(&self, layer: Layer) -> bool {
        self.shared.layer_enabled(layer)
    }

    pub fn toggle_layer(&self, layer: Layer) {
        self.shared
            .set_layer_enabled(layer, !self.shared.layer_enabled(layer));
    }

    /// Manual intensity 0..=1 (only used while auto-intensity is off).
    pub fn set_intensity(&self, value: f32) {
        self.shared.set_intensity(value);
    }

    pub fn intensity(&self) -> f32 {
        self.shared.intensity()
    }

    pub fn set_auto_intensity(&self, on: bool) {
        self.shared.set_auto_intensity(on);
    }

    pub fn auto_intensity(&self) -> bool {
        self.shared.auto_intensity()
    }

    pub fn status(&self) -> MusicStatus {
        self.shared.status()
    }
}

#[derive(Resource)]
struct MusicSetup {
    shared: Arc<MusicShared>,
    config: MusicConfig,
    autoplay: bool,
}

pub struct ProceduralMusicPlugin {
    pub config: MusicConfig,
    /// Spawn the `AudioPlayer` at startup. Set false to spawn `ProceduralMusic` yourself.
    pub autoplay: bool,
}

impl ProceduralMusicPlugin {
    pub fn new(config: MusicConfig) -> Self {
        Self {
            config,
            autoplay: true,
        }
    }
}

impl Default for ProceduralMusicPlugin {
    fn default() -> Self {
        Self::new(MusicConfig::default())
    }
}

impl Plugin for ProceduralMusicPlugin {
    fn build(&self, app: &mut App) {
        let shared = Arc::new(MusicShared::new(&self.config));
        app.add_audio_source::<ProceduralMusic>()
            .insert_resource(MusicController {
                shared: shared.clone(),
            })
            .insert_resource(MusicSetup {
                shared,
                config: self.config.clone(),
                autoplay: self.autoplay,
            })
            .add_systems(Startup, start_music);
    }
}

fn start_music(
    mut commands: Commands,
    mut assets: ResMut<Assets<ProceduralMusic>>,
    setup: Res<MusicSetup>,
) {
    if !setup.autoplay {
        return;
    }
    let handle = assets.add(ProceduralMusic::new(
        setup.config.clone(),
        setup.shared.clone(),
    ));
    commands.spawn((AudioPlayer(handle), MusicTrack));
}
