//! The sequencer: steps through the loop, asks the composer for a new plan each loop, and drives
//! the synth. Engine-agnostic: call `render_interleaved` from any audio callback.
//!
//! Game code talks to the audio thread through `MusicShared` (lock-free atomics only).

use std::f32::consts::TAU;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Arc;

use crate::composer::{Composer, Layer, LayerAction, LoopPlan, STEPS_PER_BAR, STEPS_PER_LOOP};
use crate::preset::{MusicPace, MusicPreset};
use crate::synth::Synth;
use crate::theory::{ScaleKind, Theme, ThemeOverrides, NOTE_NAMES};

#[derive(Clone, Debug)]
pub struct MusicConfig {
    pub seed: u64,
    /// Musical identity used for composition and synthesis.
    pub preset: MusicPreset,
    /// Fast mode increases tempo and rhythmic density for metal presets.
    pub pace: MusicPace,
    pub sample_rate: u32,
    /// Pin key / mode / tempo / progression instead of deriving them from the seed.
    pub theme: ThemeOverrides,
    /// Let intensity rise and fall on its own (layers fade in and out over `auto_intensity_period_loops`).
    pub auto_intensity: bool,
    pub auto_intensity_period_loops: u32,
    /// Manual intensity, used when `auto_intensity` is false.
    pub intensity: f32,
    pub layers_enabled: [bool; Layer::COUNT],
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            preset: MusicPreset::MythicAmbient,
            pace: MusicPace::Standard,
            sample_rate: 44_100,
            theme: ThemeOverrides::default(),
            auto_intensity: true,
            auto_intensity_period_loops: 8,
            intensity: 0.6,
            layers_enabled: [true; Layer::COUNT],
        }
    }
}

/// Snapshot of what the audio thread is doing, for UI / gameplay.
#[derive(Clone, Debug)]
pub struct MusicStatus {
    pub seed: u64,
    pub preset: MusicPreset,
    pub pace: MusicPace,
    pub loop_index: u64,
    pub step: u32,
    pub bpm: f32,
    pub intensity: f32,
    pub tonic: i32,
    pub scale: ScaleKind,
    pub actions: [LayerAction; Layer::COUNT],
    pub enabled: [bool; Layer::COUNT],
    pub audible: [bool; Layer::COUNT],
}

impl MusicStatus {
    pub fn key_name(&self) -> &'static str {
        NOTE_NAMES[self.tonic.rem_euclid(12) as usize]
    }
}

fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed);
}

fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

pub struct MusicShared {
    // game -> audio
    seed_request: AtomicU64,
    seed_pending: AtomicBool,
    preset_request: AtomicU32,
    preset_pending: AtomicBool,
    layer_mask: AtomicU32,
    intensity: AtomicU32,
    auto_intensity: AtomicBool,
    // audio -> game
    cur_seed: AtomicU64,
    loop_index: AtomicU64,
    step: AtomicU32,
    bpm: AtomicU32,
    eff_intensity: AtomicU32,
    tonic: AtomicU32,
    scale: AtomicU32,
    actions: AtomicU32,
    preset: AtomicU32,
    pace: AtomicU32,
}

impl MusicShared {
    pub fn new(cfg: &MusicConfig) -> Self {
        let mut mask = 0u32;
        for (i, on) in cfg.layers_enabled.iter().enumerate() {
            if *on {
                mask |= 1 << i;
            }
        }
        Self {
            seed_request: AtomicU64::new(cfg.seed),
            seed_pending: AtomicBool::new(false),
            preset_request: AtomicU32::new(encode_preset(cfg.preset, cfg.pace, false)),
            preset_pending: AtomicBool::new(false),
            layer_mask: AtomicU32::new(mask),
            intensity: AtomicU32::new(cfg.intensity.clamp(0.0, 1.0).to_bits()),
            auto_intensity: AtomicBool::new(cfg.auto_intensity),
            cur_seed: AtomicU64::new(cfg.seed),
            loop_index: AtomicU64::new(0),
            step: AtomicU32::new(0),
            bpm: AtomicU32::new(0.0f32.to_bits()),
            eff_intensity: AtomicU32::new(0.25f32.to_bits()),
            tonic: AtomicU32::new(36),
            scale: AtomicU32::new(0),
            actions: AtomicU32::new(0),
            preset: AtomicU32::new(cfg.preset.index() as u32),
            pace: AtomicU32::new(cfg.pace.index() as u32),
        }
    }

    /// Ask for a new seed. Takes effect at the next loop boundary so playback never glitches.
    pub fn request_seed(&self, seed: u64) {
        self.seed_request.store(seed, Relaxed);
        self.seed_pending.store(true, Release);
    }

    /// The seed that is playing, or the one queued to play next.
    pub fn target_seed(&self) -> u64 {
        if self.seed_pending.load(Acquire) {
            self.seed_request.load(Relaxed)
        } else {
            self.cur_seed.load(Relaxed)
        }
    }

    fn take_seed_request(&self) -> Option<u64> {
        if self.seed_pending.swap(false, Acquire) {
            Some(self.seed_request.load(Relaxed))
        } else {
            None
        }
    }

    /// Queue a preset change for the next bar, then crossfade over two seconds.
    pub fn request_preset(&self, preset: MusicPreset, pace: MusicPace) {
        self.preset_request
            .store(encode_preset(preset, pace, false), Relaxed);
        self.preset_pending.store(true, Release);
    }

    /// Start a preset crossfade as soon as the audio callback processes this request.
    pub fn request_preset_immediate(&self, preset: MusicPreset, pace: MusicPace) {
        self.preset_request
            .store(encode_preset(preset, pace, true), Relaxed);
        self.preset_pending.store(true, Release);
    }

    fn take_preset_request(&self) -> Option<(MusicPreset, MusicPace, bool)> {
        if self.preset_pending.swap(false, Acquire) {
            Some(decode_preset(self.preset_request.load(Relaxed)))
        } else {
            None
        }
    }

    fn publish_preset(&self, preset: MusicPreset, pace: MusicPace) {
        self.preset.store(preset.index() as u32, Relaxed);
        self.pace.store(pace.index() as u32, Relaxed);
    }

    pub fn set_layer_enabled(&self, layer: Layer, on: bool) {
        let bit = 1u32 << layer.index();
        if on {
            self.layer_mask.fetch_or(bit, Relaxed);
        } else {
            self.layer_mask.fetch_and(!bit, Relaxed);
        }
    }

    pub fn layer_enabled(&self, layer: Layer) -> bool {
        self.layer_mask.load(Relaxed) & (1u32 << layer.index()) != 0
    }

    pub fn set_intensity(&self, v: f32) {
        store_f32(&self.intensity, v.clamp(0.0, 1.0));
    }

    pub fn intensity(&self) -> f32 {
        load_f32(&self.intensity)
    }

    pub fn set_auto_intensity(&self, on: bool) {
        self.auto_intensity.store(on, Relaxed);
    }

    pub fn auto_intensity(&self) -> bool {
        self.auto_intensity.load(Relaxed)
    }

    /// The intensity currently gating layers (auto curve or manual value).
    pub fn effective_intensity(&self) -> f32 {
        if self.auto_intensity() {
            load_f32(&self.eff_intensity)
        } else {
            self.intensity()
        }
    }

    pub fn status(&self) -> MusicStatus {
        let intensity = self.effective_intensity();
        let preset = MusicPreset::from_index(self.preset.load(Relaxed) as usize);
        let pace = MusicPace::from_index(self.pace.load(Relaxed) as usize);
        let bits = self.actions.load(Relaxed);
        let mut actions = [LayerAction::Regenerate; Layer::COUNT];
        let mut enabled = [false; Layer::COUNT];
        let mut audible = [false; Layer::COUNT];
        for layer in Layer::ALL {
            let i = layer.index();
            actions[i] = LayerAction::from_bits((bits >> (i * 2)) & 0b11);
            enabled[i] = self.layer_enabled(layer) && layer.used_by(preset);
            audible[i] = enabled[i] && intensity >= layer.threshold();
        }
        MusicStatus {
            seed: self.cur_seed.load(Relaxed),
            preset,
            pace,
            loop_index: self.loop_index.load(Relaxed),
            step: self.step.load(Relaxed),
            bpm: load_f32(&self.bpm),
            intensity,
            tonic: self.tonic.load(Relaxed) as i32,
            scale: ScaleKind::from_index(self.scale.load(Relaxed) as usize),
            actions,
            enabled,
            audible,
        }
    }
}

fn encode_preset(preset: MusicPreset, pace: MusicPace, immediate: bool) -> u32 {
    preset.index() as u32 | ((pace.index() as u32) << 2) | ((immediate as u32) << 3)
}

fn decode_preset(bits: u32) -> (MusicPreset, MusicPace, bool) {
    (
        MusicPreset::from_index((bits & 0b11) as usize),
        MusicPace::from_index(((bits >> 2) & 1) as usize),
        bits & (1 << 3) != 0,
    )
}

fn step_len_samples(bpm: f32, sample_rate: u32) -> f64 {
    60.0 / f64::from(bpm) / 4.0 * f64::from(sample_rate)
}

pub struct Engine {
    shared: Arc<MusicShared>,
    overrides: ThemeOverrides,
    period_loops: u32,
    sample_rate: u32,
    preset: MusicPreset,
    pace: MusicPace,
    synth: Synth,
    composer: Composer,
    plan: LoopPlan,
    step: u32,
    step_len: f64,
    countdown: f64,
    publish_status: bool,
    pending_preset: Option<(MusicPreset, MusicPace, bool)>,
    transition: Option<Box<Engine>>,
    transition_elapsed: u64,
    transition_frames: u64,
}

impl Engine {
    pub fn new(shared: Arc<MusicShared>, cfg: &MusicConfig) -> Self {
        Self::new_inner(shared, cfg, true, true)
    }

    fn new_inner(
        shared: Arc<MusicShared>,
        cfg: &MusicConfig,
        publish_status: bool,
        consume_seed: bool,
    ) -> Self {
        let seed = if consume_seed {
            shared.take_seed_request().unwrap_or(cfg.seed)
        } else {
            cfg.seed
        };
        let theme = Theme::from_seed_for_preset(seed, &cfg.theme, cfg.preset, cfg.pace);
        let step_len = step_len_samples(theme.bpm, cfg.sample_rate);
        let synth = Synth::new(cfg.sample_rate, theme.bpm);
        let engine = Self {
            shared,
            overrides: cfg.theme.clone(),
            period_loops: cfg.auto_intensity_period_loops.max(2),
            sample_rate: cfg.sample_rate,
            preset: cfg.preset,
            pace: cfg.pace,
            synth,
            composer: Composer::with_preset(seed, theme, cfg.preset, cfg.pace),
            plan: LoopPlan::default(),
            step: 0,
            step_len,
            countdown: 0.0,
            publish_status,
            pending_preset: None,
            transition: None,
            transition_elapsed: 0,
            transition_frames: (f64::from(cfg.sample_rate) * 2.0).max(1.0) as u64,
        };
        if publish_status {
            engine.publish_theme();
        }
        engine
    }

    fn publish_theme(&self) {
        let theme = self.composer.theme();
        if self.publish_status {
            self.shared.cur_seed.store(self.composer.seed(), Relaxed);
            store_f32(&self.shared.bpm, theme.bpm);
            self.shared.tonic.store(theme.tonic as u32, Relaxed);
            self.shared.scale.store(theme.scale.index() as u32, Relaxed);
        }
    }

    fn begin_loop(&mut self) {
        if let Some(seed) = self.shared.take_seed_request() {
            let theme = Theme::from_seed_for_preset(seed, &self.overrides, self.preset, self.pace);
            self.synth.set_bpm(theme.bpm);
            self.step_len = step_len_samples(theme.bpm, self.sample_rate);
            self.composer = Composer::with_preset(seed, theme, self.preset, self.pace);
            self.publish_theme();
        }

        let plan = self.composer.next_loop();

        // Auto intensity: a cosine swell so layers build up and fall away over `period_loops` loops.
        if self.publish_status {
            let period = u64::from(self.period_loops);
            let phase = (plan.loop_index % period) as f32 / period as f32;
            let swell = 0.5 - 0.5 * (TAU * phase).cos();
            store_f32(&self.shared.eff_intensity, 0.25 + 0.75 * swell);
        }

        let mut bits = 0u32;
        for layer in Layer::ALL {
            bits |= plan.actions[layer.index()].to_bits() << (layer.index() * 2);
        }
        if self.publish_status {
            self.shared.actions.store(bits, Relaxed);
            self.shared.loop_index.store(plan.loop_index, Relaxed);
        }
        self.plan = plan;
    }

    fn trigger_step(&mut self) {
        let step_secs = self.step_len as f32 / self.sample_rate as f32;
        let intensity = self.shared.effective_intensity();
        for layer in Layer::ALL {
            let audible = self.shared.layer_enabled(layer)
                && layer.used_by(self.preset)
                && intensity >= layer.threshold();
            if !audible {
                continue;
            }
            for ev in &self.plan.layers[layer.index()] {
                if ev.step == self.step {
                    let gate = ev.len as f32 * step_secs * 0.95;
                    self.synth.note_on(ev.timbre, ev.note, ev.vel, gate);
                }
            }
        }
    }

    fn on_step(&mut self) {
        if self.step == 0 {
            self.begin_loop();
        }
        if self.publish_status {
            self.shared.step.store(self.step, Relaxed);
        }
        self.trigger_step();
        self.step += 1;
        if self.step >= STEPS_PER_LOOP {
            self.step = 0;
        }
    }

    fn begin_transition(&mut self, preset: MusicPreset, pace: MusicPace) {
        let cfg = MusicConfig {
            seed: self.composer.seed(),
            preset,
            pace,
            sample_rate: self.sample_rate,
            theme: self.overrides.clone(),
            auto_intensity: self.shared.auto_intensity(),
            auto_intensity_period_loops: self.period_loops,
            intensity: self.shared.intensity(),
            layers_enabled: std::array::from_fn(|i| {
                self.shared.layer_mask.load(Relaxed) & (1 << i) != 0
            }),
        };
        self.transition = Some(Box::new(Self::new_inner(
            self.shared.clone(),
            &cfg,
            false,
            false,
        )));
        self.transition_elapsed = 0;
        self.transition_frames = (f64::from(self.sample_rate) * 2.0).max(1.0) as u64;
    }

    fn swap_render_state(&mut self, other: &mut Engine) {
        std::mem::swap(&mut self.overrides, &mut other.overrides);
        std::mem::swap(&mut self.period_loops, &mut other.period_loops);
        std::mem::swap(&mut self.sample_rate, &mut other.sample_rate);
        std::mem::swap(&mut self.preset, &mut other.preset);
        std::mem::swap(&mut self.pace, &mut other.pace);
        std::mem::swap(&mut self.synth, &mut other.synth);
        std::mem::swap(&mut self.composer, &mut other.composer);
        std::mem::swap(&mut self.plan, &mut other.plan);
        std::mem::swap(&mut self.step, &mut other.step);
        std::mem::swap(&mut self.step_len, &mut other.step_len);
        std::mem::swap(&mut self.countdown, &mut other.countdown);
    }

    pub fn render_frame(&mut self) -> (f32, f32) {
        if self.publish_status {
            if let Some(request) = self.shared.take_preset_request() {
                self.pending_preset = Some(request);
            }
            if self.transition.is_none() {
                #[allow(clippy::manual_is_multiple_of)]
                let ready = self
                    .pending_preset
                    .map(|(_, _, immediate)| {
                        immediate || (self.countdown <= 0.0 && self.step % STEPS_PER_BAR == 0)
                    })
                    .unwrap_or(false);
                if ready {
                    if let Some((preset, pace, _)) = self.pending_preset.take() {
                        if preset != self.preset || pace != self.pace {
                            self.begin_transition(preset, pace);
                        }
                    }
                }
            }
        }
        if self.countdown <= 0.0 {
            self.on_step();
            self.countdown += self.step_len;
        }
        self.countdown -= 1.0;
        let outgoing = self.synth.render_frame();
        if let Some(incoming) = self.transition.as_mut() {
            let target = incoming.render_frame();
            let t =
                (self.transition_elapsed as f32 / self.transition_frames as f32).clamp(0.0, 1.0);
            let mixed = (
                outgoing.0 * (1.0 - t) + target.0 * t,
                outgoing.1 * (1.0 - t) + target.1 * t,
            );
            self.transition_elapsed += 1;
            if self.transition_elapsed >= self.transition_frames {
                let mut incoming = *self.transition.take().expect("transition exists");
                self.swap_render_state(&mut incoming);
                self.shared.publish_preset(self.preset, self.pace);
                self.publish_theme();
            }
            mixed
        } else {
            outgoing
        }
    }

    /// Fill `out` with interleaved stereo (L, R, L, R, ...) samples.
    pub fn render_interleaved(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            let (l, r) = self.render_frame();
            frame[0] = l;
            frame[1] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(seed: u64, frames: usize) -> Vec<f32> {
        let cfg = MusicConfig {
            seed,
            ..Default::default()
        };
        let shared = Arc::new(MusicShared::new(&cfg));
        let mut engine = Engine::new(shared, &cfg);
        let mut out = vec![0.0; frames * 2];
        engine.render_interleaved(&mut out);
        out
    }

    #[test]
    fn deterministic_for_a_seed() {
        assert_eq!(render(3, 44_100), render(3, 44_100));
    }

    #[test]
    fn different_seeds_sound_different() {
        assert_ne!(render(3, 44_100 * 2), render(4, 44_100 * 2));
    }

    #[test]
    fn produces_bounded_audible_output() {
        let out = render(3, 44_100 * 3);
        assert!(out.iter().any(|s| s.abs() > 0.01));
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn reseed_request_is_consumed_at_loop_boundary() {
        let cfg = MusicConfig::default();
        let shared = Arc::new(MusicShared::new(&cfg));
        shared.request_seed(555);
        assert_eq!(shared.target_seed(), 555);
        let _engine = Engine::new(shared.clone(), &cfg); // picks the request up immediately
        assert_eq!(shared.status().seed, 555);
    }
}
