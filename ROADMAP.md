# Verdandi Audio Quality Roadmap

This document is the durable plan for improving Verdandi's instrument realism and audio quality. Update the milestone checkboxes and decisions here as work is completed so future development sessions can resume from the repository rather than relying on conversation history.

## Goal and design direction

Prioritize the best musical result, not a sample-free implementation. Verdandi may combine recorded samples, procedural synthesis, and hybrid techniques. Choose the method per instrument based on audible quality, runtime requirements, licensing, portability, and maintenance cost.

Keep the existing deterministic composition engine and layered, game-oriented behavior. The composer decides what notes to play and when; instrument backends decide how those notes sound. Preserve existing sounds unless a replacement or addition is demonstrably better.

## Current baseline

- [x] Seeded, layered procedural composition and four named presets: Ethereal, Warbound, Orchestral, and Contemplative.
- [x] Runtime preset/pace transitions.
- [x] Built-in deterministic musical stingers.
- [x] Improved procedural plucked-string and bowed-string voices.
- [ ] Sample playback or bundled instrument assets: not implemented.
- [ ] Sampled acoustic guitar and violin: not implemented.
- [ ] Sample asset distribution and provenance process: not implemented.

The current procedural voices remain the baseline and fallback while the sample-based path is evaluated. Contemplative's established sound and the distorted Guitar voice should not be changed without an audible reason.

## Milestones

### M1 — Evaluate sample sources and licenses

Status: **Not started**

Find and audition candidate acoustic-guitar and violin/ensemble sample sets. Initial candidates discussed include Karoryfer Emilyguitar and the Versilian Community Sample Library (VCSL); these are leads to verify, not pre-approved dependencies.

Tasks:
- [ ] Verify the exact source release, instrument coverage, format, sample rate, bit depth, and quality.
- [ ] Listen to actual samples and compare low, middle, and high registers, attacks, releases, and velocity response.
- [ ] Confirm the license for the exact files selected, including redistribution inside a separately distributed asset pack and use by downstream games.
- [ ] Record source URL, release/version, license text or identifier, attribution requirements, modifications, and checksums for every selected asset.
- [ ] Estimate compressed and uncompressed footprint and identify any unsuitable, noisy, incomplete, or inconsistent material.

**Exit criteria:** at least one viable candidate for acoustic guitar and one for violin/bowed strings, supported by listening notes and verified redistribution terms. Do not commit assets before these checks.

### M2 — Choose the sample playback architecture

Status: **Not started**

Run a focused engineering spike before selecting a permanent backend. Compare a mature SoundFont/SFZ-capable approach with a small dedicated sampler, using the same candidate material and representative Verdandi note events.

Requirements to evaluate:
- [ ] Pitch and velocity zones; sensible handling outside recorded ranges.
- [ ] Round-robin or other deterministic variation where multiple takes exist.
- [ ] Appropriate interpolation and acceptable pitch-shifting artifacts.
- [ ] Smooth sustain loops for bowed instruments and natural release/decay.
- [ ] Deterministic sample/round-robin selection from stable seed and event identity.
- [ ] Multiple concurrent notes, voice stealing, note-off behavior, and tail handling.
- [ ] No file I/O, sample decoding, locks, or avoidable allocations in the realtime render path.
- [ ] Clear asset loading/preparation lifecycle and useful errors for missing or incompatible assets.
- [ ] Compatibility with the existing engine, offline WAV renderer, Bevy integration, and supported Rust targets.
- [ ] Dependency health, license, maintenance, and format/tooling trade-offs.

**Exit criteria:** a documented backend decision backed by an isolated render and an explicit account of limitations. Prefer reuse when a maintained dependency meets the requirements; do not build a custom sampler solely for its own sake.

### M3 — Define asset packaging and fallback behavior

Status: **Not started**

Keep large instrument libraries out of the core crate if their size or license makes bundling inappropriate. Evaluate a separately versioned asset bundle or another explicit distribution mechanism.

Tasks:
- [ ] Choose an asset layout and versioning scheme that maps cleanly to instrument presets and sample zones.
- [ ] Keep code and optional sample assets independently consumable where practical.
- [ ] Document how applications obtain, locate, configure, and update assets.
- [ ] Define behavior when assets are absent, corrupt, or incompatible; retain procedural fallback where practical.
- [ ] Keep license notices and asset provenance with the distributed assets.
- [ ] Ensure applications can preload/decode samples before realtime playback begins.

**Exit criteria:** documented, reproducible asset packaging with license information and no runtime network or disk work on the audio render thread.

### M4 — Implement and audition acoustic guitar

Status: **Not started**

Implement the first sampled instrument as a small, isolated vertical slice. Do not replace the existing Pluck or distorted Guitar voice at the outset.

Tasks:
- [ ] Map available recordings to pitch/velocity zones and choose sensible fallback behavior for uncovered ranges.
- [ ] Implement note-on, note-off/tail behavior, gain, interpolation, and deterministic take selection as required by the chosen backend.
- [ ] Produce isolated WAV renders across low, middle, and high registers and several velocities.
- [ ] Compare sample playback against the existing procedural Pluck at matched pitch and loudness.
- [ ] Listen for unnatural repetition, pitch-shifting artifacts, clicks, abrupt tails, noise, and inconsistent loudness.
- [ ] Tune the mapping and playback behavior before integrating it into a musical layer.

**Exit criteria:** the sampled voice is audibly preferable for its intended acoustic-guitar role, passes deterministic/bounded-output and lifecycle tests, and can be auditioned independently.

### M5 — Implement and audition violin / bowed strings

Status: **Not started**

Build on the selected backend and asset pipeline. Begin with useful, well-supported articulations rather than promising a complete orchestral library immediately.

Tasks:
- [ ] Evaluate recorded violin/ensemble material for sustain quality, bow changes, release, pitch range, and velocity layers.
- [ ] Implement short and sustained articulations where the recordings support them.
- [ ] Implement smooth sustain looping and release tails where appropriate.
- [ ] Avoid artificial repetition through deterministic round-robin selection when the source offers multiple takes.
- [ ] Produce isolated WAV renders and compare against the current Strings and ClassicalStrings voices.
- [ ] Document gaps in the source material, including unsupported articulations or ranges.

**Exit criteria:** the sampled bowed-string voice sounds better for its intended role and has documented articulation/range limits. Do not imply true legato or other articulations unless the assets and implementation support them.

### M6 — Integrate accepted voices into arrangements

Status: **Not started**

Only integrate a sampled voice after its isolated audition succeeds.

Tasks:
- [ ] Choose the preset layers and musical roles that benefit from sampled acoustic guitar and bowed strings.
- [ ] Compare old and new renders at matched seed, tempo, intensity, and output level.
- [ ] Preserve Warbound's distorted guitar character and Contemplative's established character unless listening demonstrates a clear improvement.
- [ ] Verify that layering does not cause clipping, masking, excessive CPU use, or undesirable stereo placement.
- [ ] Keep existing procedural voices available as fallbacks or alternatives where useful.

**Exit criteria:** each integration has an audible rationale and does not regress other presets or event behavior.

### M7 — Validate, document, and release

Status: **Not started**

- [ ] Add tests for deterministic zone/take selection, sample boundaries, note-off/release, voice exhaustion, and bounded finite output.
- [ ] Confirm realtime rendering does not perform file access, decoding, locking, or avoidable allocation.
- [ ] Check preset transitions, stingers, pause/resume, and concurrent notes with sampled voices.
- [ ] Verify offline WAV rendering works with both procedural-only and sample-enabled configurations.
- [ ] Document asset setup, supported formats, distribution, licenses, fallback behavior, and resource usage.
- [ ] Update examples so individual instruments and musical layers can be rendered for listening tests.
- [ ] Update the crate version according to SemVer if public API, dependency, or distribution changes warrant it.
- [ ] Open focused PRs and record validation results; do not mark tests as passed unless they were actually run.

**Exit criteria:** the feature is documented, tested to the extent supported by the environment, audibly reviewed, and distributable under the verified licenses.

## Quality requirements for every milestone

- **Determinism:** the same seed and same event sequence produce the same musical decisions and deterministic sample choices. Document any platform-level differences in floating-point rendering if relevant.
- **Realtime safety:** prepare assets and allocate playback resources before rendering; avoid disk access, decoding, locks, and avoidable allocations in the audio callback.
- **Robust output:** prevent invalid sample data from propagating NaNs or infinities, and maintain the engine's output bounds.
- **Compatibility:** preserve the current composer, preset API, transitions, stingers, and procedural voices unless a change is deliberate and documented.
- **Audition before acceptance:** unit tests cannot establish that an instrument sounds realistic. Keep isolated WAV renders and listening notes as part of the review.
- **Licensing:** track provenance and redistribution permissions for every included sample set.

## Decision log

- **Direction changed:** sample-free is no longer a requirement. Use samples, synthesis, or a hybrid based on the best result for each instrument.
- **Architecture principle:** retain Verdandi's existing composition/sequencing layer and add instrument playback behind it where practical.
- **Initial instrument order:** acoustic guitar first, violin/bowed strings second.
- **Initial candidate sources:** Karoryfer Emilyguitar and VCSL are evaluation candidates only; exact releases and licenses must be verified before inclusion.
- **Distribution principle:** avoid forcing a large sample library into the core crate; prefer an explicit optional/separately versioned asset distribution if warranted.
- **Non-goal for the first pass:** building a complete orchestral sampler or replacing every procedural voice at once.

## How to resume this roadmap

At the start of a related development session:
1. Read this file and check which milestone is the first incomplete one.
2. Inspect the current repository implementation and open PRs before acting.
3. Work on the smallest milestone-sized change, preserving the exit criteria above.
4. Update this roadmap when a decision changes or a milestone is completed, including evidence and known limitations.
5. Do not mark a milestone complete based solely on code being written; use its exit criteria.
