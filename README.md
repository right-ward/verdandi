# verdandi

Seed-driven, layered music for games with deterministic composition. Verdandi has four style presets; by default, voices are synthesized at runtime and no prerecorded audio assets are required. It can also load user-provided SFZ instruments for more realistic recorded timbres. Sample files remain external to the crate. The same seed and configuration produce the same musical events, with deterministic SFZ region selection for the same note sequence.

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

## SFZ sample instruments

Verdandi includes its own SFZ parser and WAV sample player. No extra backend crate or sample download is required for procedural-only use. To use recorded instruments, install a sample library yourself and load its SFZ patch before creating the engine. The patch must point to the corresponding sample files; paths are resolved relative to the patch and its optional SFZ default_path.

The first implementation supports mono/stereo RIFF/WAVE samples encoded as PCM 8/16/24/32-bit or IEEE float 32/64-bit. FLAC and other sample codecs are not supported yet. The parser reports opcodes or SFZ sections it does not implement through SfzInstrument::unsupported_opcodes(). Unsupported opcodes are currently ignored, so check this list before relying on patches with advanced behaviour. The committed files under samples/ are small mapping fixtures, not complete instrument libraries: their referenced audio samples are intentionally not included.

Assign a preloaded instrument to any composition layer. If no sample region matches a note, Verdandi uses the existing procedural voice for that event.

~~~rust
use std::sync::Arc;
use verdandi::prelude::*;

fn configure_sampled_music() -> Result<MusicConfig, SfzError> {
    // This mapping text is committed under samples/, while the large audio library stays local.
    // The explicit root resolves sample=samples/... paths into your downloaded library.
    let guitar = Arc::new(SfzInstrument::load_with_sample_root(
        "samples/SpanishClassicalGuitar-20190618.sfz",
        "/path/to/SpanishClassicalGuitar-SFZ",
    )?);

    if !guitar.unsupported_opcodes().is_empty() {
        eprintln!("SFZ features not implemented: {:?}", guitar.unsupported_opcodes());
    }

    let mut config = MusicConfig::default();
    config.seed = 1234;
    config.preset = MusicPreset::ClassicalAmbient;
    config.instruments.set_sfz(Layer::Arp, guitar);
    Ok(config)
}
~~~

InstrumentConfig is part of MusicConfig, so the same setup works with the engine-agnostic renderer and the Bevy plugin. Sample loading and decoding happen before engine construction; the render callback performs no file access or decoding.

To audition an SFZ library directly and render a WAV without Bevy or an audio device:

~~~sh
cargo run --release --no-default-features --example sfz_audition -- /path/to/library/instrument.sfz guitar-audition.wav
~~~

The audition renderer accepts optional seed and MIDI note bounds after the output filename. When using a committed mapping whose audio samples live in a separate local library directory, pass `--sample-root`:

~~~sh
cargo run --release --no-default-features --example sfz_audition -- samples/SpanishClassicalGuitar-20190618.sfz guitar-audition.wav --sample-root /path/to/SpanishClassicalGuitar-SFZ
~~~

The sample root is the directory containing the library's `samples/` directory. The mapping's SFZ `default_path`, if present, is applied beneath that root.

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
| `synth.rs` | Small polyphonic synth + drums + stereo delay; no allocation on the hot path |
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
