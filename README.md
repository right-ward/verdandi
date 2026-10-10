# verdandi

Seed-driven, layered procedural music for games. Verdandi has three style presets, with every sound synthesized at runtime and no prerecorded audio assets required. The same preset and seed produce the same evolving track.

Presets change musical composition as well as timbre:

- **Mythic Ambient** — slow, spacious exploration music with bowed-string-like tones, sustained harmony, and bell-like accents.
- **Heavy Metal** — riff-driven music with synthesized distorted guitars, bass, and metal-oriented drums.
- **Legendary Metal** — heavy metal combined with orchestral-string and choir-like layers for a grand, sacred atmosphere.

Select `MusicPace::Fast` for denser, faster metal arrangements. Mythic Ambient keeps its slower tempo range. Explicit `ThemeOverrides` for key, scale, tempo, or progression always take priority over preset defaults.

The same seed, preset, pace, and overrides reproduce the same musical patterns. Layer patterns can repeat, mutate, or regenerate at four-bar loop boundaries, while intensity controls when layers become audible.

Targets **Bevy 0.20**.

## Demo
Test the core using:
```
cargo test --no-default-features
```

Then the demo. On Ubuntu, install Bevy's native build dependencies first: `sudo apt-get install build-essential pkg-config libasound2-dev libudev-dev libx11-dev libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev`.

```
cargo run --release --example demo -- 1234
```

Keys: `Left/Right` seed -/+, `N` random seed, `1-8` toggle layers, `Up/Down` intensity (switches to manual), `A` auto intensity, `Space` pause. Seed changes apply at the next loop boundary.

Choose a preset and pace with the optional arguments (seed, preset, pace):

```sh
cargo run --release --example demo -- 1234 mythic-ambient
cargo run --release --example demo -- 1234 heavy-metal fast
cargo run --release --example demo -- 1234 legendary-metal
```

## Use it in your game

```toml
[dependencies]
verdandi = { path = "../verdandi" }
```

```rust
use bevy::prelude::*;
use verdandi::prelude::*;

App::new()
    .add_plugins(DefaultPlugins) // must come before the music plugin
    .add_plugins(ProceduralMusicPlugin::new(MusicConfig { seed: 1234, ..default() }))
    .run();

// Anywhere in a system:
fn on_boss_fight(music: Res<MusicController>) {
    music.set_auto_intensity(false);
    music.set_intensity(1.0);                 // all layers
    music.set_layer_enabled(Layer::Lead, false);
}
```

Per-run seeds: `music.set_seed(run_seed)`. Volume/pause: query `AudioSink` with `MusicTrack`. Pin parts of the frame with `MusicConfig::theme` (`ThemeOverrides { bpm, scale, tonic_pitch_class, progression }`). Spawn only one player per `ProceduralMusicPlugin`.

Not using Bevy? Depend on it with `default-features = false` and call `Engine::render_interleaved` from your own audio callback.

## Layout

| File | Role |
|---|---|
| `rng.rs` | SplitMix64 + `derive_seed(master, tag, index)` so each layer/loop gets an independent, reproducible stream |
| `theory.rs` | Scales and preset-aware `Theme` (key/mode/tempo/progression from seed) |
| `preset.rs` | The three musical presets and standard/fast pacing |
| `composer.rs` | Preset-specific layer generators, motif-based melodies and riffs, mutation, repeat/mutate/regenerate policy per layer |
| `synth.rs` | Small polyphonic synth + drums + stereo delay; no allocation on the hot path |
| `engine.rs` | Sequencer, `MusicConfig`, lock-free `MusicShared` control/status block |
| `plugin.rs` | Bevy `Decodable` source, `MusicController`, plugin |

## Tuning and extending

- Feel of "how much changes per loop": `Layer::volatility` in `composer.rs`.
- When layers enter: `Layer::threshold`; auto curve: `begin_loop` in `engine.rs`.
- New preset: add its profile in `preset.rs` and `Theme::from_seed_for_preset`, then provide style-specific generators in `composer.rs`.
- New layer: add a `Layer` variant (update `COUNT`/`ALL`/`name`/`threshold`/`tag`/`volatility`), write a generator, and handle it in composition and mutation.
- Better sound: improve the procedural voices in `synth.rs`. Bowed-string, guitar, choir, and bell voices are stylized synthesis rather than recordings or physical models.

## Known limits

- Fixed 4/4, 16 steps per bar, 4-bar loops.
- Instrument voices are procedural approximations; they do not yet reproduce real acoustic violins, guitars, or choirs.
- Presets are selected through `MusicConfig`; seamless runtime transitions between presets are not implemented.
- Seed changes wait for the loop boundary (about 4.6-16.6 s across built-in preset tempo ranges).
- No crossfade or stinger system yet; intensity gating is note-level, so layers enter on the next note.
