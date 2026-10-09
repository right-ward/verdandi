# verdandi

Seed-driven layered procedural music for Bevy. Same seed gives the same evolving track; a new seed gives a different one.

The seed fixes the **frame** (key, mode, tempo, 4-chord progression). Each loop (4 bars), every **layer** (drums, bass, pad, arp, lead) either repeats its last pattern, mutates it slightly, or regenerates it. Layers fade in and out with an intensity curve. Sound is synthesised in real time, so no audio assets are needed.

Targets **Bevy 0.20**.

## Demo
Test the core using:
```
cargo test --no-default-features
```

Then the demo (Linux needs `libasound2-dev` and `libudev-dev`):

```
cargo run --release --example demo -- 1234
```

Keys: `Left/Right` seed -/+, `N` random seed, `1-5` toggle layers, `Up/Down` intensity (switches to manual), `A` auto intensity, `Space` pause. Seed changes apply at the next loop boundary.

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
| `theory.rs` | Scales, `Theme` (key/mode/tempo/progression from seed), scale-degree to MIDI |
| `composer.rs` | Per-layer generators, motif-based lead, mutation, repeat/mutate/regenerate policy per layer |
| `synth.rs` | Small polyphonic synth + drums + stereo delay; no allocation on the hot path |
| `engine.rs` | Sequencer, `MusicConfig`, lock-free `MusicShared` control/status block |
| `plugin.rs` | Bevy `Decodable` source, `MusicController`, plugin |

## Tuning and extending

- Feel of "how much changes per loop": `Layer::volatility` in `composer.rs`.
- When layers enter: `Layer::threshold`; auto curve: `begin_loop` in `engine.rs`.
- New layer: add a `Layer` variant (update `COUNT`/`ALL`/`name`/`threshold`/`tag`/`volatility`), write a `gen_*` function, add it to `generate` and `mutate`.
- Better sound: replace `synth.rs` voices (naive saw/pulse aliasing at high notes) or have `NoteEvent`s trigger sampled instruments instead.

## Known limits

- Fixed 4/4, 16 steps per bar, 4-bar loops.
- Seed changes wait for the loop boundary (about 8-12 s).
- No crossfade or stinger system yet; intensity gating is note-level, so layers enter on the next note.
