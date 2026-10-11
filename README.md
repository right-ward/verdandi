# verdandi

Seed-driven, layered procedural music for games. Verdandi has four style presets, with every sound synthesized at runtime and no prerecorded audio assets required. The same preset and seed produce the same evolving track.

Presets change musical composition as well as timbre:

- **Mythic Ambient** — slow, spacious exploration music with bowed-string-like tones, sustained harmony, and bell-like accents.
- **Heavy Metal** — riff-driven music with synthesized distorted guitars, bass, and metal-oriented drums.
- **Legendary Metal** — heavy metal combined with orchestral-string and choir-like layers for a grand, sacred atmosphere.
- **Classical Ambient** — restrained, string-led ambience with a stately, historical-soundtrack character, a warm sustained pad, softer bowed-string-like harmony, and a gentle ostinato.

Select `MusicPace::Fast` for denser, faster metal arrangements. Both ambient presets keep their slower tempo ranges. Explicit `ThemeOverrides` for key, scale, tempo, or progression always take priority over preset defaults.

The same seed, preset, pace, and overrides reproduce the same musical patterns. Layer patterns can repeat, mutate, or regenerate at four-bar loop boundaries, while intensity controls when layers become audible.

Targets **Bevy 0.20**.

## Demo
Test the core using:
```
cargo test --no-default-features
```

Then the demo. On Ubuntu, install Bevy's native build dependencies first:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libasound2-dev libx11-dev libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev
```

```
cargo run --release --no-default-features --features demo --example demo -- 1234
```

Keys: `Left/Right` seed -/+, `N` random seed, `1-8` toggle layers, `Up/Down` intensity (switches to manual), `A` auto intensity, `Space` pause. Seed changes apply at the next loop boundary.

Choose a preset and pace with the optional arguments (seed, preset, pace):

```sh
cargo run --release --no-default-features --features demo --example demo -- 1234 mythic-ambient
cargo run --release --no-default-features --features demo --example demo -- 1234 classical-ambient
cargo run --release --no-default-features --features demo --example demo -- 1234 heavy-metal fast
cargo run --release --no-default-features --features demo --example demo -- 1234 legendary-metal
```

## Offline WAV renderer

The `render_wav` example generates a standalone stereo, 16-bit PCM WAV file. It uses the engine directly, with no Bevy window, audio device, or extra dependencies, so it can also run in Termux. The requested loop count is rendered to the file; each loop is four bars. To diagnose a preset's sound, append a layer name (`drums`, `bass`, `pad`, `arp`, `lead`, `rhythm-guitar`, `strings`, or `choir`) after the optional pace. Isolated renders disable automatic intensity and hold intensity at 1.0, so the chosen layer stays audible throughout.

```sh
cargo run --release --no-default-features --example render_wav -- 1234 mythic-ambient 8 mythic.wav
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 8 classical.wav
cargo run --release --no-default-features --example render_wav -- 1234 heavy-metal 8 heavy.wav fast
cargo run --release --no-default-features --example render_wav -- 1234 legendary-metal 12 legendary.wav
```

Arguments are `seed preset loops output.wav [standard|fast] [layer]`. The layer argument requires the pace argument to come first. A layer not used by the selected preset is rejected. Presets also accept the short names `ambient`, `metal`, and `legendary`. WAV output is 44.1 kHz; transfer the generated file to Android or open it in any WAV-capable player.

For example, render the Classical Ambient pad, strings, lead, and arpeggio separately with the same seed:

```sh
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 classical-pad.wav standard pad
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 classical-strings.wav standard strings
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 classical-lead.wav standard lead
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 classical-arp.wav standard arp
```

## Use it in your game

```toml
[dependencies]
verdandi = { default-features = false }
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
| `preset.rs` | The four musical presets and standard/fast pacing |
| `composer.rs` | Preset-specific layer generators, motif-based melodies and riffs, mutation, repeat/mutate/regenerate policy per layer |
| `synth.rs` | Small polyphonic synth + drums + stereo delay; no allocation on the hot path |
| `engine.rs` | Sequencer, `MusicConfig`, lock-free `MusicShared` control/status block |
| `examples/render_wav.rs` | Headless CLI exporter for fixed-length WAV tracks |
| `plugin.rs` | Bevy `Decodable` source, `MusicController`, plugin |

## Tuning and extending

- Feel of "how much changes per loop": `Layer::volatility` in `composer.rs`.
- When layers enter: `Layer::threshold`; auto curve: `begin_loop` in `engine.rs`.
- New preset: add its profile in `preset.rs` and `Theme::from_seed_for_preset`, then provide style-specific generators in `composer.rs`.
- New layer: add a `Layer` variant (update `COUNT`/`ALL`/`name`/`threshold`/`tag`/`volatility`), write a generator, and handle it in composition and mutation.
- Better sound: improve the procedural voices in `synth.rs`. Bowed-string, guitar, choir, and bell voices are stylized synthesis rather than recordings or physical models. The string and guitar timbres intentionally favor a compact, stable synthesis model over acoustic realism. Classical Ambient uses a separate, softer bowed-string approximation so its sustained harmony can evolve without changing the brighter Strings timbre used by the arpeggio.

## Known limits

- Fixed 4/4, 16 steps per bar, 4-bar loops.
- Instrument voices are procedural approximations; they do not yet reproduce real acoustic violins, guitars, or choirs.
- The metal guitar and bowed-string voices are stylized timbres, not physical models; improve them incrementally based on WAV listening tests.
- Presets are selected through `MusicConfig`; seamless runtime transitions between presets are not implemented.
- Seed changes wait for the loop boundary (about 4.6-16.6 s across built-in preset tempo ranges).
- No crossfade or stinger system yet; intensity gating is note-level, so layers enter on the next note.
