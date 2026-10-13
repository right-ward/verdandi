---
layout: default
title: Docs & Guides
permalink: /docs/
description: "How to add Verdandi to a Bevy game, control procedural music, render WAV files, and use its public API."
---

# Verdandi documentation

This page brings the practical guides and public API overview together. It describes the current source in the [Verdandi repository](https://github.com/right-ward/verdandi), which targets **Rust and Bevy 0.20**. APIs can differ between repository code and older published crate versions, so check the version you install.

## What Verdandi does

Verdandi builds evolving, layered game music from a seed and a musical style. It chooses a key, mode, tempo, and chord progression, then composes patterns for each layer. At four-bar boundaries, a layer may repeat its last pattern, mutate it, or create a new one. The result is designed to be both recognizable and changeable rather than a single fixed loop.

All current instrument voices are synthesized at runtime. Verdandi does not bundle prerecorded soundtrack files or a sample library.

## Installation

### Bevy integration

Verdandi's current source uses **Bevy 0.20**. To use the Bevy plugin from the repository's current main branch, add dependencies like these:

<pre><code>[dependencies]
bevy = "0.20"
verdandi = { git = "https://github.com/right-ward/verdandi", branch = "main" }</code></pre>

The <code>bevy-plugin</code> feature is enabled by default. For a release build, pin the Git dependency to a reviewed commit or tag instead of relying indefinitely on a moving branch.

### Core engine without the Bevy plugin

If you provide your own audio output and only need the renderer, disable default features:

<pre><code>[dependencies]
verdandi = { git = "https://github.com/right-ward/verdandi", branch = "main", default-features = false }</code></pre>

This leaves the engine-agnostic composition and synthesis code available without pulling in Verdandi's optional Bevy audio integration.

## Quick start: play procedural music in Bevy

Register Bevy's normal plugins first, then add Verdandi's music plugin. The plugin creates and starts one music player by default.

<pre><code>use bevy::prelude::*;
use verdandi::prelude::{
    MusicConfig, MusicPace, MusicPreset, ProceduralMusicPlugin,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ProceduralMusicPlugin::new(MusicConfig {
            seed: 1234,
            preset: MusicPreset::MythicAmbient,
            pace: MusicPace::Standard,
            ..default()
        }))
        .run();
}</code></pre>

Use one <code>ProceduralMusicPlugin</code> for one procedural music player. If you need full control over player creation, set the plugin's <code>autoplay</code> field to <code>false</code> and manage the <code>ProceduralMusic</code> asset/player yourself.

## Choose a musical style

The public enum variants retain older internal names. Their displayed names are the newer style names used in the site and command-line examples.

| Public API variant | Style | Character |
|---|---|---|
| <code>MusicPreset::MythicAmbient</code> | **Ethereal** | Spacious, slow exploration music with sustained harmony and bell-like accents. |
| <code>MusicPreset::HeavyMetal</code> | **Warbound** | Driving riffs, distorted-guitar-like tones, bass, and drums. |
| <code>MusicPreset::LegendaryMetal</code> | **Orchestral** | Forceful metal arrangements expanded with strings and choir-like layers. |
| <code>MusicPreset::ClassicalAmbient</code> | **Contemplative** | Restrained, string-led ambience with a warm pad, darker lead, and gentle ostinato. |

<code>MusicPace::Fast</code> is intended to make **Warbound** and **Orchestral** faster and rhythmically denser. Ethereal and Contemplative keep their slower tempo ranges with either pace setting.

## Set the seed and musical theme

A seed makes the generated musical patterns repeatable for a given preset, pace, and set of theme overrides. This is useful for replaying a particular atmosphere, testing encounters, or making a save/game seed select its soundtrack.

<pre><code>MusicConfig {
    seed: 9001,
    preset: MusicPreset::HeavyMetal,
    pace: MusicPace::Fast,
    ..default()
}</code></pre>

To pin parts of the musical theme instead of letting the seed choose them, supply <code>ThemeOverrides</code>:

<pre><code>use verdandi::prelude::{MusicConfig, MusicPreset, MusicPace, ScaleKind, ThemeOverrides};

let config = MusicConfig {
    seed: 9001,
    preset: MusicPreset::ClassicalAmbient,
    pace: MusicPace::Standard,
    theme: ThemeOverrides {
        tonic_pitch_class: Some(0), // C; 0 = C through 11 = B
        scale: Some(ScaleKind::Minor),
        bpm: Some(84.0),
        progression: Some([0, 4, 5, 3]), // four scale-degree chord roots
    },
    ..default()
};</code></pre>

Every override is optional. Explicit values take priority over preset defaults. Tempo is clamped by the engine to the supported range of 40–220 BPM.

## Control music during gameplay

The plugin inserts a <code>MusicController</code> resource. Use it from your Bevy systems to react to gameplay events.

### Change preset, intensity, or layers

<pre><code>use bevy::prelude::*;
use verdandi::prelude::{Layer, MusicController, MusicPace, MusicPreset};

fn start_boss_fight(music: Res&lt;MusicController&gt;) {
    music.set_preset(MusicPreset::LegendaryMetal, MusicPace::Fast);
    music.set_auto_intensity(false);
    music.set_intensity(1.0);
    music.set_layer_enabled(Layer::Choir, true);
}</code></pre>

- <code>set_preset(preset, pace)</code> queues the change for the next bar, then crossfades over about two seconds.
- <code>set_preset_immediate(preset, pace)</code> begins the crossfade as soon as the audio callback processes the request.
- <code>set_seed(seed)</code> queues a new seed for the next four-bar loop boundary.
- <code>set_auto_intensity(true)</code> lets Verdandi swell the arrangement automatically; <code>false</code> switches to your manual intensity.
- <code>set_intensity(value)</code> sets manual intensity from <code>0.0</code> to <code>1.0</code>. It only determines the audible intensity while automatic intensity is disabled.
- <code>set_layer_enabled(layer, enabled)</code> or <code>toggle_layer(layer)</code> lets your game explicitly enable or disable a layer.

Intensity is not a per-track master-volume control. It determines which arrangement layers clear their audible thresholds; use Bevy's audio sink for overall playback volume and pause/resume.

### React to events with stingers

Verdandi includes short synthesized cues that play over the current music:

<pre><code>use bevy::prelude::*;
use verdandi::prelude::{MusicController, MusicStinger};

fn on_boss_revealed(music: Res&lt;MusicController&gt;) {
    music.play_stinger(MusicStinger::BossReveal);
}</code></pre>

Available cues are <code>CombatStart</code>, <code>Victory</code>, <code>BossReveal</code>, and <code>Transition</code>. They follow the active theme's scale and tempo. A newly triggered cue replaces a cue already in progress.

### Read the current playback state

<code>MusicController::status()</code> returns a <code>MusicStatus</code> snapshot. It includes the active seed, preset and pace, loop and step positions, BPM, intensity, key/scale, and arrays describing each layer's enablement, audibility, and latest pattern action. For example:

<pre><code>fn inspect_music(music: Res&lt;MusicController&gt;) {
    let status = music.status();
    info!(
        "seed={}, style={}, {} BPM, intensity={:.2}",
        status.seed,
        status.preset.name(),
        status.bpm,
        status.intensity,
    );
}</code></pre>

The active preset in the status may not change until a queued transition has completed.

## How-to: render a WAV file without opening a game

The <code>render_wav</code> example renders a fixed number of four-bar loops to a stereo, 16-bit PCM WAV file. It does not require a Bevy window or an audio playback device, making it useful for auditioning a seed or inspecting a layer.

From the repository root:

<pre><code>cargo run --release --no-default-features --example render_wav -- 1234 ethereal 8 ethereal.wav
cargo run --release --no-default-features --example render_wav -- 1234 warbound 8 warbound-fast.wav fast
cargo run --release --no-default-features --example render_wav -- 1234 contemplative 4 contemplative-pad.wav standard pad</code></pre>

Arguments are:

<pre><code>render_wav &lt;seed&gt; &lt;preset&gt; &lt;loops&gt; &lt;output.wav&gt; [standard|fast] [layer]</code></pre>

Use the current style names <code>ethereal</code>, <code>warbound</code>, <code>orchestral</code>, or <code>contemplative</code>. Older aliases remain accepted. Supported layer names are <code>drums</code>, <code>bass</code>, <code>pad</code>, <code>arp</code>, <code>lead</code>, <code>rhythm-guitar</code>, <code>strings</code>, and <code>choir</code>. To render a single layer, supply a pace first—even <code>standard</code>—then the layer name. A layer that the selected preset does not use is rejected.

The file is written at 44.1 kHz by default. Transfer it to a phone or open it in any WAV-capable player to listen.

## How-to: run the interactive demo

The demo shows live seed, style, loop, intensity, and layer status. It also lets you toggle the arrangement while listening.

<pre><code>cargo test --no-default-features
cargo run --release --no-default-features --features demo --example demo -- 1234
cargo run --release --no-default-features --features demo --example demo -- 1234 warbound fast</code></pre>

In the demo, use **Left/Right** to change the seed, **N** for a random seed, **1–8** to toggle layers, **Up/Down** to adjust manual intensity, **A** to toggle automatic intensity, and **Space** to pause or resume playback. Seed changes take effect at the next loop boundary.

On Ubuntu, Bevy's demo may require native build dependencies. See the [repository README](https://github.com/right-ward/verdandi#demo) before building.

## Use Verdandi without Bevy

The core engine exposes <code>Engine</code>, <code>MusicConfig</code>, and <code>MusicShared</code>. Create a shared state, pass it into an engine, and pull interleaved stereo <code>f32</code> samples into your own audio callback:

<pre><code>use std::sync::Arc;
use verdandi::prelude::{Engine, MusicConfig, MusicShared};

let config = MusicConfig::default();
let shared = Arc::new(MusicShared::new(&config));
let mut engine = Engine::new(shared, &config);

let mut stereo_samples = vec![0.0_f32; 2048]; // 1024 stereo frames
engine.render_interleaved(&mut stereo_samples);
// Send the interleaved left/right samples to your audio output.</code></pre>

Keep rendering at the sample rate configured in <code>MusicConfig</code> (44,100 Hz by default). The output buffer should hold stereo interleaved samples, with left and right samples paired. If your audio backend invokes callbacks on a dedicated thread, render there rather than blocking gameplay systems.

For a working WAV writer, start with [examples/render_wav.rs](https://github.com/right-ward/verdandi/blob/main/examples/render_wav.rs). For a complete Bevy example, see [examples/demo.rs](https://github.com/right-ward/verdandi/blob/main/examples/demo.rs).

## Public API at a glance

| Type | What it is for |
|---|---|
| <code>MusicConfig</code> | Initial seed, preset, pace, sample rate, theme overrides, intensity behavior, and enabled layers. |
| <code>MusicPreset</code> / <code>MusicPace</code> | Select one of the four musical styles and the arrangement pace. |
| <code>MusicController</code> | Bevy resource for runtime seed, preset, layer, intensity, and stinger requests. |
| <code>MusicStatus</code> | Snapshot of the style and arrangement currently playing. |
| <code>Layer</code> | <code>Drums</code>, <code>Bass</code>, <code>Pad</code>, <code>Arp</code>, <code>Lead</code>, <code>RhythmGuitar</code>, <code>Strings</code>, and <code>Choir</code>. Not every preset uses every layer. |
| <code>MusicStinger</code> | Built-in event cues: combat start, victory, boss reveal, and transition. |
| <code>ThemeOverrides</code> / <code>ScaleKind</code> | Pin the tonic, mode, tempo, and four-bar chord progression. |
| <code>Engine</code> / <code>MusicShared</code> | Lower-level, Bevy-independent rendering and control. |

For exact signatures and source-level documentation, start at the public [prelude](https://github.com/right-ward/verdandi/blob/main/src/lib.rs), [plugin API](https://github.com/right-ward/verdandi/blob/main/src/plugin.rs), [engine](https://github.com/right-ward/verdandi/blob/main/src/engine.rs), [composer and layers](https://github.com/right-ward/verdandi/blob/main/src/composer.rs), and [theme overrides](https://github.com/right-ward/verdandi/blob/main/src/theory.rs).

## Current limitations

- Music is composed in fixed 4/4 time using 16 steps per bar and four-bar loops.
- Instrument sounds are stylized synthesis, not recordings or physically accurate models of acoustic instruments.
- Sample playback and bundled instrument assets are not implemented yet; the roadmap describes the evaluation work planned for them.
- Seed changes are applied at the next loop boundary rather than cutting over immediately.
- Intensity controls layer audibility. It does not replace the host game's master-volume control.

See the [audio-quality roadmap](https://github.com/right-ward/verdandi/blob/main/ROADMAP.md) for planned improvements and the [README](https://github.com/right-ward/verdandi#readme) for the latest run commands.
