# verdandi

Seed-driven, layered music for games. Verdandi has four style presets and a procedural synthesizer that works without assets. Its first-party SFZ loader can also play user-supplied sampled instruments; no sample libraries are bundled or downloaded automatically. The same preset and seed produce the same musical decisions.

Presets change musical composition as well as timbre:

- **Ethereal** — slow, spacious exploration music with bowed-string-like tones, sustained harmony, and bell-like accents.
- **Warbound** — riff-driven music with synthesized distorted guitars, bass, and metal-oriented drums.
- **Orchestral** — heavy metal combined with orchestral-string and choir-like layers for a grand, sacred atmosphere.
- **Contemplative** — restrained, string-led ambience with a stately, historical-soundtrack character, a warm sustained pad, softer bowed-string-like harmony, a darker melodic lead, and a gentle ostinato.

Select `MusicPace::Fast` for denser, faster metal arrangements. Ethereal and Contemplative keep their slower tempo ranges. Explicit `ThemeOverrides` for key, scale, tempo, or progression always take priority over preset defaults.

The same seed, preset, pace, and overrides reproduce the same musical patterns. Layer patterns can repeat, mutate, or regenerate at four-bar loop boundaries, while intensity controls when layers become audible. Contemplative develops its lead and string ostinato across each four-bar phrase, with a clearer cadence at the end.

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
cargo run --release --no-default-features --features demo --example demo -- 1234 ethereal
cargo run --release --no-default-features --features demo --example demo -- 1234 contemplative
cargo run --release --no-default-features --features demo --example demo -- 1234 warbound fast
cargo run --release --no-default-features --features demo --example demo -- 1234 orchestral
```

## Offline WAV renderer

The `render_wav` example generates a standalone stereo, 16-bit PCM WAV file. It uses the engine directly, with no Bevy window, audio device, or extra dependencies, so it can also run in Termux. The requested loop count is rendered to the file; each loop is four bars. To diagnose a preset's sound, append a layer name (`drums`, `bass`, `pad`, `arp`, `lead`, `rhythm-guitar`, `strings`, or `choir`) after the optional pace. Isolated renders disable automatic intensity and hold intensity at 1.0, so the chosen layer stays audible throughout.

```sh
cargo run --release --no-default-features --example render_wav -- 1234 mythic-ambient 8 mythic.wav
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 8 classical.wav
cargo run --release --no-default-features --example render_wav -- 1234 heavy-metal 8 heavy.wav fast
cargo run --release --no-default-features --example render_wav -- 1234 legendary-metal 12 legendary.wav
```

Arguments are `seed preset loops output.wav [standard|fast] [layer]`. The layer argument requires the pace argument to come first. A layer not used by the selected preset is rejected. The new names are `ethereal`, `warbound`, `orchestral`, and `contemplative`. Previous names (`mythic-ambient`, `heavy-metal`, `legendary-metal`, `classical-ambient`) and short aliases remain accepted. WAV output is 44.1 kHz; transfer the generated file to Android or open it in any WAV-capable player.

For example, render the Classical Ambient pad, strings, lead, and arpeggio separately with the same seed:

```sh
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 contemplative-pad.wav standard pad
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 contemplative-strings.wav standard strings
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 contemplative-lead.wav standard lead
cargo run --release --no-default-features --example render_wav -- 1234 classical-ambient 2 contemplative-arp.wav standard arp
```

## First-party SFZ sample audition

Verdandi includes an in-progress native SFZ loader and sample voice renderer. It currently supports the basic mapping and one-shot playback features needed by the Spanish Classical Guitar mapping, plus WAV PCM (8/16/24/32-bit) and IEEE float (32/64-bit) decoding. Loaded instruments can be assigned to a procedural timbre in `MusicConfig`; matching notes use the sample voice and uncovered notes fall back to procedural synthesis. This is not yet a complete SFZ implementation.

Keep sample libraries on your own device. The mapping's `sample=` paths are resolved against the sample-root argument, so the SFZ text can live separately from the original library directory. No samples are included in the crate.

Render a chromatic audition file (the example defaults to MIDI notes 48–72):

```sh
cargo run --release --no-default-features --example sfz_render_wav -- samples/SpanishClassicalGuitar-20190618.sfz /path/to/SpanishClassicalGuitar-SFZ guitar.wav
```

Replace `/path/to/SpanishClassicalGuitar-SFZ` with the directory containing the library's `samples/` subdirectory. You can optionally pass the first and last MIDI notes after the output path. The renderer reports unsupported opcodes and errors for missing samples rather than silently substituting an unrelated sound.

Current support includes key and velocity ranges, root key, volume, release time, and basic loop points. Envelopes beyond release, velocity curves, random/round-robin selection, filters, modulation, stereo preservation, and many SFZ extensions are not implemented yet. The procedural engine remains fully usable without SFZ files.

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

Per-run seeds: `music.set_seed(run_seed)`. Change styles at runtime with `music.set_preset(MusicPreset::HeavyMetal, MusicPace::Fast)`, which waits for the next bar and crossfades over two seconds. `music.set_preset_immediate(...)` starts that crossfade as soon as the audio callback processes the request. `MusicController::status()` reports the active preset after the transition. Trigger synthesized event cues with `music.play_stinger(MusicStinger::CombatStart)`, `Victory`, `BossReveal`, or `Transition`; cues follow the current scale and tempo and play over the continuing track. A new cue replaces an in-progress cue. Volume/pause: query `AudioSink` with `MusicTrack`. Pin parts of the frame with `MusicConfig::theme` (`ThemeOverrides { bpm, scale, tonic_pitch_class, progression }`). Spawn only one player per `ProceduralMusicPlugin`.

Not using Bevy? Depend on it with `default-features = false` and call `Engine::render_interleaved` from your own audio callback.

## Layout

| File | Role |
|---|---|
| `rng.rs` | SplitMix64 + `derive_seed(master, tag, index)` so each layer/loop gets an independent, reproducible stream |
| `theory.rs` | Scales and preset-aware `Theme` (key/mode/tempo/progression from seed) |
| `preset.rs` | The four musical presets and standard/fast pacing |
| `composer.rs` | Preset-specific layer generators, motif-based melodies and riffs, mutation, repeat/mutate/regenerate policy per layer |
| `synth.rs` | Procedural polyphonic synth + drums + stereo delay; no allocation on the hot path |\n| `sfz.rs` | First-party SFZ region loader, WAV decoder, and sampled voice renderer (partial SFZ support) |
| `engine.rs` | Sequencer, `MusicConfig`, lock-free `MusicShared` control/status block |
| `examples/render_wav.rs` | Headless CLI exporter for fixed-length WAV tracks |
| `plugin.rs` | Bevy `Decodable` source, `MusicController`, plugin |

## Tuning and extending

- Feel of "how much changes per loop": `Layer::volatility` in `composer.rs`.
- When layers enter: `Layer::threshold`; auto curve: `begin_loop` in `engine.rs`.
- New preset: add its profile in `preset.rs` and `Theme::from_seed_for_preset`, then provide style-specific generators in `composer.rs`.
- New layer: add a `Layer` variant (update `COUNT`/`ALL`/`name`/`threshold`/`tag`/`volatility`), write a generator, and handle it in composition and mutation.
- Better sound: improve the procedural voices in `synth.rs`. Bowed-string, guitar, choir, and bell voices are stylized synthesis rather than recordings or physical models. The Pluck voice uses a seeded Karplus–Strong string loop with a variable pick position, velocity-sensitive brightness/attack, and pitch-sensitive decay, while the Ethereal/Orchestral Strings voice has a softer bow onset, gradually introduced vibrato, evolving harmonics, and subtle deterministic bow noise. The dedicated Contemplative strings and metal Guitar voices remain separate so these changes do not alter their character.

## Known limits

- Fixed 4/4, 16 steps per bar, 4-bar loops.
- Instrument voices are procedural approximations; they do not yet reproduce real acoustic violins, guitars, or choirs.
- The metal guitar and bowed-string voices are stylized timbres, not physical models. The Pluck voice uses a simple physical-inspired string model with velocity- and pitch-dependent articulation, but still needs WAV listening tests and further tuning.
- Runtime preset changes crossfade over two seconds; the default waits for the next bar, while the immediate variant starts as soon as the audio callback processes the request. Built-in event stingers are synthesized at runtime and replace one another if retriggered.
- Seed changes wait for the loop boundary (about 4.6-16.6 s across built-in preset tempo ranges).
- Intensity gating is note-level, so layers enter on the next note.
