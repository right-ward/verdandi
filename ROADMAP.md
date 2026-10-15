# Verdandi Audio Quality Roadmap

This document is the durable plan for improving Verdandi's instrument realism and audio quality. Update the milestone checkboxes and decisions here as work is completed so future development sessions can resume from the repository rather than relying on memory.

## Goal and design direction

Prioritize the best musical result, not a sample-free implementation. Verdandi combines recorded samples and procedural synthesis where each is useful. The SFZ parser and playback engine are first-party components in this repository; sample libraries are supplied and managed by the user and are never bundled or downloaded by Verdandi.

Keep the existing deterministic composition engine and layered, game-oriented behavior. The composer decides what notes to play and when; instrument backends decide how those notes sound. Preserve existing sounds unless a replacement or addition is demonstrably better.

## Current baseline

- [x] Seeded, layered procedural composition and four named presets: Ethereal, Warbound, Orchestral, and Contemplative.
- [x] Runtime preset/pace transitions.
- [x] Built-in deterministic musical stingers.
- [x] Improved procedural plucked-string and bowed-string voices.
- [x] First-party partial SFZ loading, WAV decoding, sample voices, and optional timbre routing.
- [ ] Sampled acoustic guitar and violin: playback integration exists, but instrument auditions and musical-layer acceptance are pending.
- [x] User-supplied sample paths; no sample assets are bundled or distributed by Verdandi.

The current procedural voices remain the baseline and fallback while the sample-based path is evaluated. Contemplative's established sound and the distorted Guitar voice should not be changed without an audible reason.

## Milestones

### M1 — Inspect sample mappings and audition candidates

Status: **In progress**

The user has auditioned most of the locally collected libraries. MerryOrks and FreePats Electric Guitar FSBS still need a reliable audition path because the earlier Python helper reports a generic error for their mappings. The SFZ mapping text for the available candidates is committed under `samples/`; the corresponding audio files remain local and are not part of this repository.

Verdandi does not distribute sample libraries. Users are responsible for obtaining, licensing, storing, and updating their own libraries. Therefore, third-party sample redistribution permissions are outside the Verdandi crate's scope; any future Ashen Chronicle use must be reviewed separately.

Tasks:
- [ ] Inspect each committed SFZ mapping and identify the features used by each library.
- [ ] Diagnose the two audition-helper failures without assuming the mappings themselves are invalid.
- [ ] Record the useful note range, articulations, loops, and sample formats for candidate instruments.
- [x] Commit small SFZ mapping text files as parser fixtures and development references.

**Exit criteria:** mappings and required SFZ features are understood well enough to drive implementation and compatibility tests.

### M2 — Implement a first-party SFZ engine

Status: **In progress**

**Decision:** implement the SFZ parser, region selection, sample decoding, and playback voice engine inside Verdandi. Do not add an external sampler dependency or a separate `verdandi-sfz` crate. Keep procedural synthesis as the default and fallback.

The first vertical slice now exists in `src/sfz.rs`: it loads SFZ global/group/region settings, resolves sample paths, decodes supported WAV formats, selects regions by key and velocity, pitch-shifts by resampling, applies gain/release, and renders a sample voice. The `sfz_render_wav` example creates offline note-range audition files. This is wired into the composition engine through optional timbre-to-instrument assignments, with procedural fallback for unmatched notes. It is not full SFZ compatibility.

Current implementation scope:
- [x] Parse the basic opcodes required by the Spanish Classical Guitar mapping.
- [x] Load WAV PCM (8/16/24/32-bit) and IEEE float (32/64-bit) samples before rendering.
- [x] Select regions by key/velocity and render pitched sample voices.
- [x] Add a headless WAV audition example and focused unit tests.
- [ ] Improve diagnostics, parser coverage, and fixtures against the other committed mappings.
- [ ] Implement reliable sustain-loop and note-release semantics.
- [ ] Add additional common sample formats, including FLAC, if required by the target libraries.
- [ ] Establish voice limits, deterministic round-robin/random selection, and realtime-safe voice management.
- [ ] Integrate sample voices with the existing engine while preserving procedural fallback.
- [ ] Validate compatibility with the offline renderer, Bevy integration, and supported Rust targets.

**Exit criteria:** the first-party sampler can render selected real instruments accurately enough for audition, and its integration design preserves deterministic composition and realtime safety.

### M3 — Define user-supplied sample paths and fallback behavior

Status: **Decided; integration pending**

Sample libraries remain external to the crate. Users download a library, keep its SFZ file and referenced samples together, and explicitly provide the mapping and sample root to Verdandi. No network access or automatic sample downloads are part of runtime playback.

Tasks:
- [x] Keep sample audio out of the repository and published crate.
- [x] Allow the sample root to be specified separately from the SFZ mapping location.
- [x] Add an engine-level instrument configuration that selects an SFZ instrument per procedural timbre.
- [ ] Define fallback behavior for missing or invalid sample instruments.
- [ ] Document memory use, loading lifecycle, supported opcodes, and library setup.

**Exit criteria:** applications can opt into local samples without making sample assets a requirement for using Verdandi.

### M4 — Implement and audition acoustic guitar

Status: **Not started**

Implement the first sampled instrument as a small, isolated vertical slice. Do not replace the existing Pluck or distorted Guitar voice at the outset.

Tasks:
- [ ] Validate the Spanish Classical Guitar mapping and its sample-root layout with the new renderer.\n- [ ] Map available recordings to pitch/velocity zones and choose sensible fallback behavior for uncovered ranges.
- [ ] Implement note-on, note-off/tail behavior, gain, interpolation, and deterministic take selection as required by the chosen backend.
- [x] Add a configurable MIDI-range WAV audition example.\n- [ ] Produce and listen to isolated WAV renders across low, middle, and high registers and several velocities.
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

- **Composition and sound are separate:** Verdandi's seeded composition remains independent of the backend used to render each instrument.
- **Both backends live in Verdandi:** procedural voices remain the default and fallback; the first-party SFZ parser and sampler are part of the same crate.
- **No third-party sampler dependency:** use Rust audio-decoding libraries only when useful, but own SFZ parsing, region selection, voice management, and playback semantics.
- **User-supplied sample libraries:** Verdandi does not distribute or download sample assets. The committed SFZ text files are small development fixtures; recordings remain outside the repository.
- **First instrument order:** validate Spanish Classical Guitar, then support the electric guitar and bowed-string mappings according to their opcode requirements.
- **Initial sampler scope:** support the common subset needed by real candidate libraries, report unsupported opcodes, and expand compatibility through fixtures rather than claiming complete SFZ support prematurely.
- **Non-goal for the first pass:** building a graphical sampler application or replacing every procedural voice at once.
