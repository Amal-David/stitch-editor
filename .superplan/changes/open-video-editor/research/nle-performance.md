# NLE model and performance contract

> **Status:** reviewed council recommendation, attested as Terra High
> ([attestation](terra-high-attestation.md)). It is deliberately independent of
> Rust, Go, MediaBunny, FFmpeg, and UI framework selection. The cross-review
> corrections are binding ([review](review-nle-performance.md)). No claim of
> "fastest" is credible until the benchmark contract below has published,
> reproducible results.

## Decision in one page

Build the editor around an immutable, revisioned **editorial model**, compiled on
demand into one shared **audio/video render graph**. The project model is the
source of truth; decoder, proxy, preview, and export implementations are
replaceable adapters. This is the useful separation demonstrated by mature
systems: OpenTimelineIO distinguishes editorial structure from media references,
while GES separates user-facing layers from output tracks
([OTIO model](https://opentimelineio.readthedocs.io/en/v0.12/tutorials/architecture.html),
[GES architecture](https://gstreamer.freedesktop.org/documentation/gst-editing-services/)).

Do not make a mutable playback pipeline, a codec library, or UI state the project
format. Those choices make deterministic undo, recovery, relinking, cache
invalidation, and backend replacement far harder than they need to be.

The initial product should optimize for one excellent vertical slice: multi-track
cut/trim/split, clip and track fades, gain, a dissolve, transform/opacity, and
one color effect; 2K/3K local files; preview, proxies, save/reopen/recover, and
mandatory AVC/H.264 + AAC MP4 export on acceptance machines. It should *not*
initially promise arbitrary
third-party effects, nested sequences, HDR finishing, collaboration, AI, or a
universal codec matrix.

MLT is a useful reminder that a consumer can pull lazily through a graph, with
image and audio requested separately; its multitrack `tractor` pulls tracks
evenly ([MLT framework](https://www.mltframework.org/docs/framework/)). Adopt
the pull/demand insight, not its frame-number-centric storage model.

## 1. Deterministic non-destructive project model

### Canonical objects

Persist a canonical, schema-versioned project document with stable random 128-bit
IDs, canonical key ordering, normalized rationals, and no derived/cache/UI-only
data. A revision consists of:

| Object | Required immutable data |
| --- | --- |
| `Project` | format/schema version, project ID, settings, ordered sequences, asset registry, root revision |
| `Sequence` | display name, exact output geometry, pixel aspect, frame-rate policy, working color config digest, audio rate/layout, ordered tracks/buses |
| `Track` / `Bus` | stable order key, kind, enabled/locked state, compositing/blend policy or routing, items |
| `ClipInstance` | asset and stream IDs, timeline range, source range, explicit time map, transform, opacity, effect stack, audio gain/pan/fades |
| `Transition` | explicit two input IDs, overlap/range, alignment, curve, transition/effect ID and parameter automation |
| `EffectInstance` | provider ID/version/content digest, parameter schema version, typed static parameters and keyed automation |
| `Asset` | stable asset ID, immutable media identity record, stream catalogue, locators and proxy variants |
| `Marker` / `Caption` / `Generator` | typed data and time range; generators have a versioned deterministic parameter definition |

Use explicit ordering keys, not incidental array order, and half-open ranges
`[start, end)`. A transition is a first-class operation with declared inputs;
never infer it merely from overlap. OTIO provides useful editorial precedent for
tracks, stacks, clips, gaps, and transitions, including transition in/out offsets
([OTIO timeline structure](https://opentimelineio.readthedocs.io/en/v0.12/tutorials/otio-timeline-structure.html)).
Our internal model may export an OTIO subset, but is not constrained by OTIO's
interchange limitations.

### Revisions, commands, undo/redo

Every durable edit is a typed command with:

`command_id, parent_revision_hash, operation, canonical arguments, preconditions,
inverse-or-before-image, author/device metadata, schema version`.

Applying a command produces a new immutable revision and canonical content hash.
Validation happens before the revision is visible: IDs resolve, ranges are valid,
ordering is total, graph types match, and no checked rational arithmetic
overflows. A command whose preconditions fail is rejected with a structured
conflict, never "best-effort" applied.

Undo moves a local head to the command's parent (or applies a recorded inverse
when sharing is later introduced); redo moves to the remembered child. Dragging,
scrubbing, and slider motion are transient previews; coalesce them into one
durable command on commit, with an optional 250 ms durable checkpoint for a long
gesture. The result is deterministic undo/redo, exact history inspection, and a
future-compatible revision DAG without building collaboration now.

### Exact time, frame rate, and VFR

Store every canonical project/render-plan time as a normalized signed rational
`num/den` (positive denominator, checked 128-bit intermediate arithmetic); do
not store seconds as floating point, or make a single `i64` tick rate the
authoritative timeline. `i64` ticks are permitted only as a private cache/index
optimization. This aligns with Core Media's
rational `value/timescale` model ([CMTime](https://developer.apple.com/documentation/coremedia/cmtime?changes=_9))
and OTIO's rational-time/range vocabulary, including exclusive end ranges
([OTIO `TimeRange`](https://opentimelineio.readthedocs.io/en/v0.17.0/api/python/opentimelineio.opentime.html)).

Rules:

- Source samples retain demuxed presentation timestamps and duration. Variable
  frame rate remains VFR; it is never silently converted to a nominal frame grid.
- A sequence declares `CFR(rate)` or `VFR(pass-through only where an export
  profile explicitly permits it)`. CFR render sample `n` is requested at the
  exact sequence time `n / rate`; source selection and any resampling rule are
  explicit in the `TimeMap`.
- Audio time is sample-indexed at the sequence rate (initially 48,000 Hz).
  Timecode/drop-frame notation is a display/interchange attribute, never the
  authoritative clock.
- All rounding sites name a policy (`floor`, `ceil`, `nearest-even`, or
  `hold-previous`). Persist it where it affects output. Reject invalid,
  indefinite, or overflowed times rather than mapping them to zero.

Platform APIs are views of canonical time, not replacements for it. An adapter
converts each **absolute** canonical PTS and duration once to its required
integer unit with checked arithmetic, and logs `canonical_time`, `adapter_tick`,
`rounding_policy`, and rational conversion error. It must not repeatedly add
rounded durations. This is material on Windows, where Media Foundation samples
use integer 100-nanosecond times
([`IMFSample::SetSampleTime`](https://learn.microsoft.com/en-us/windows/win32/api/mfobjects/nf-mfobjects-imfsample-setsampletime)); Microsoft likewise requires
timestamps/durations to be preserved as far as possible and permits rounded
calculated durations ([MFT timestamps](https://learn.microsoft.com/en-us/windows/win32/medfound/time-stamps-and-durations)).
Per-track adapter PTS are strictly monotonic and bounded to the named conversion
rule; container PTS derive from canonical output time/track timescale, not from
rounded decoder callbacks. The conformance suite exercises 10,000 frames each at
24000/1001, 30000/1001, and 60000/1001 plus VFR PTS, checking the exact
rational sequence, monotonicity, and logged conversion bound.

This prevents 23.976/29.97/59.94 drift, makes frame-accurate trims meaningful,
and lets the same project render identically through different backends.

### Media identity and relinking

An `Asset` is not a file path. It owns a content identity and a set of mutable
locators:

1. **Strong identity:** full BLAKE3 digest (computed in the background), file
   length, and a stream fingerprint (container/stream ID, codec configuration,
   dimensions, channel layout, duration, timebase).
2. **Fast candidate identity:** volume/file ID where supported, size, mtime, and
   head/tail chunk digest. It only narrows search; it never proves equivalence.
3. **Locators:** original absolute URI, project-relative URI, user-approved
   relink URI, and platform bookmark/token where required. Locator provenance
   and last verification time are retained.

Relink asks the user when the strong digest differs or is unavailable. It can
offer candidates with a confidence explanation, but must not silently substitute
a weak match. If a valid replacement has a different available range or stream
shape, retain the timeline edit and flag every affected instance. This follows
the useful OTIO distinction between an external and missing reference
([OTIO file bundles](https://opentimelineio.readthedocs.io/en/v0.14/tutorials/otio-filebundles.html))
and Kdenlive's explicit missing-clip/placeholders workflow
([Kdenlive file menu](https://docs.kdenlive.org/en/user_interface/menu/file_menu.html)).

## 2. One render graph for preview and export

Compile `(revision hash, sequence, time/range)` first into a typed, acyclic
**semantic plan**, then combine it with a `QualityProfile` and backend capability
selection to produce an **execution plan**:

```text
asset bytes -> demux/index -> decode -> source time map -> color normalize
           -> clip effects/transform -> track composite -> sequence effects
           -> preview display OR video encode/mux

asset bytes -> demux/decode -> resample/channel map -> clip gain/fades/effects
           -> bus mix/latency compensation -> master -> device OR audio encode/mux
```

The plan is demand-driven from the requested output frame or audio block, with
bounded parallel work inside it. MLT's lazy consumer pull is a sound precedent
([MLT framework](https://www.mltframework.org/docs/framework/)); GES usefully
models separately ordered user layers and output tracks
([`GESTimeline`](https://gstreamer.freedesktop.org/documentation/gst-editing-services/gestimeline.html)).

Every node declares input/output media types, color metadata, alpha convention,
temporal dependency window, latency, determinism level, memory/VRAM estimate,
and cancellation safety. The semantic plan has a `semantic_plan_digest` over
revision, ranges/time maps, effect order and sampled parameters, compositing,
audio routing/automation, color intent, and missing-media policy. It must match
for preview and export of the same revision/range. The execution plan has an
`execution_plan_digest` over that semantic digest plus proxy/original selection,
resolution, precision/pixel format, backend/shader/encoder build and cache
policy. It may differ, and must be present in every trace and render manifest.

An execution-cache key includes:

`node/provider digest + canonical parameters at time + upstream keys + source
identity + color config digest + execution_plan_digest + pixel/audio format`.

Pure node results are cacheable. Stateful nodes must declare reset, warm-up, and
look-behind/look-ahead requirements; the scheduler supplies those samples or the
node returns a typed "not ready" result. No cache key may depend on wall clock,
address, UI state, or an unspecified hardware path.

Use a linear-light scene-referred working representation for compositing where
practical, retain source color metadata, and make display and export transforms
explicit graph nodes. A color configuration's content digest belongs in every
cache/render manifest. OpenColorIO's separate CPU/GPU processors and cache ID
are a useful integration pattern ([OCIO processors](https://opencolorio.readthedocs.io/en/latest/api/processors.html)).

`QualityProfile` changes cost, never editorial semantics:

- `final`: original media, full resolution, final precision, no preview-only
  simplifications;
- `preview`: may use an exact-linked proxy, lower resolution, and an explicit
  reduced-quality implementation only when the UI labels it;
- `draft-export`: an opt-in profile recorded in export metadata.

Preview and final export share semantic-plan construction, time maps, effect
ordering, color intent, audio mix semantics, and parameter sampling. They may
differ only through declared execution policy and sink. Final export must use
original assets. Original-media preview is compared numerically to final export
at selected frames; proxy preview is compared after its declared rescale/precision
tolerance. No preview-only time-map, effect, color, or audio branch is allowed.
GPU and hardware codec output need not be bit-identical to a CPU reference; their
equivalence tolerance and path must be recorded, never hidden. On macOS,
VideoToolbox explicitly exposes hardware availability and actual use
([VideoToolbox](https://developer.apple.com/documentation/videotoolbox?changes=la),
[decoder properties](https://developer.apple.com/documentation/videotoolbox/decompression-properties?changes=__7&language=objc));
Windows Media Foundation similarly separates decoder, encoder, and video
processor transforms ([hardware MFTs](https://learn.microsoft.com/en-us/windows/win32/medfound/hardware-mfts)).

## 3. Audio, scheduling, cache, and failure behavior

### Audio correctness

For preview, the audio device clock is the master; video is scheduled against it
and may drop nonessential video frames to preserve audio continuity. GStreamer
documents the same basic clock/timestamps/segment synchronization model
([clocks and synchronization](https://gstreamer.freedesktop.org/documentation/application-development/advanced/clocks.html)).
For offline export, render fixed blocks from exact sample positions, with no wall
clock dependence.

Initial audio contract: 48 kHz float processing; fixed bounded blocks selected
by the device/export profile; per-sample or sample-ramped automation; declared
linear/equal-power fade curve; explicit channel layout/downmix matrix; delay
compensation from every effect; NaN/Inf sanitization; and dither only on final
integer PCM conversion. A plugin/effect reports its latency in samples. LV2
provides precedent for explicit plugin latency and free-wheeling offline mode
([LV2 core](https://lv2plug.in/ns/lv2core)). The real-time callback may not lock,
allocate, do disk I/O, wait on proxy work, or log; LV2's hard-real-time guidance
makes the same constraint explicit ([LV2 programming guide](https://lv2plug.in/book/)).
The canonical mixer exposes a `semantic_mix_digest` shared by preview and export;
the preview device adapter may only resample, buffer, and compensate measured
device latency after that mix. The callback owns fixed preallocated buffers and
lock-free bounded rings, and receives no ownership-changing UI/project commands.
The acceptance harness runs 16 active tracks with fades, gain automation and one
declared-latency effect at recorded 48 kHz stereo 128- and 256-frame device
buffers (shared/exclusive mode stated), including device-change/unplug. Debug
allocator, lock, I/O and logger intercept counters around the callback must all
remain zero; underruns and calibrated device offset remain separate metrics.

### Proxies and caches

Maintain separate, size-bounded stores for:

- source indexes/seek maps and waveform summaries;
- decoded frames/audio blocks (RAM/VRAM); and
- disk preview renders and proxies.

All entries are disposable and addressed by the complete cache key above. Cache
admission cannot evict the current audio safety window, current-frame path, or
durable project data. Limit RAM and disk explicitly and expose usage/reasons in
the UI; Blender demonstrates why raw, preprocessed, composite, and final caches
must be distinguished ([Blender VSE cache](https://docs.blender.org/manual/en/3.6/editors/video_sequencer/sequencer/sidebar/cache.html)).

A proxy is a derived asset with parent strong identity, creation profile digest,
frame/PTS map, and its own strong digest. The baseline proxy contract is
operational rather than generic: 1280 pixels wide, square-pixel CFR, 8-bit 4:2:0
AVC/H.264, closed GOP=1/all-IDR, 48 kHz stereo AAC, a pinned container timescale,
and a manifest with source-to-proxy PTS map, encoder delay/padding and actual
hardware/software path. Creation begins only after the platform encoder proves
that exact profile; otherwise it returns a typed capability failure. Pre-generated
proxies test playback independently of proxy-generation capability/throughput.
Final export resolves the
original asset by default; using a proxy is an explicit, watermarked/manifested
draft choice. This matches the mature expectation that proxy playback is cheap
but final render uses originals ([Kdenlive proxy clips](https://docs.kdenlive.org/en/user_interface/menu/media_menu.html)).

### Background work and backpressure

Use bounded queues plus CPU, decoder-session, RAM, VRAM, disk-I/O, and GPU-submit
tokens. Priority is: audio safety -> current seek/playhead -> decode prefetch ->
UI thumbnails/waveforms -> proxy/index -> preview render -> maintenance. A new
seek increments a generation; obsolete requests are cancelled at a safe boundary
and their late outputs are discarded. Background tasks may use only the capacity
left after interactive reservations. Queue depth and bytes are hard-capped; when
full, producers slow or coalesce rather than allocating without bound.

### Autosave/recovery and bad inputs

`ProjectStore v1` is a local, single-writer project directory containing a bundled
SQLite database in `journal_mode=WAL` and an externally managed disposable cache
directory. Network/shared filesystem projects are unsupported until separately
validated. Each committed edit writes command, immutable revision, and current-head
update in one transaction; the one persistence worker alone owns writes and
compaction. Compact canonical snapshots periodically, cancel-safely and
transactionally--never by replacing an open database file.

Bundle SQLite **>= 3.51.3** (or documented fixed backport), log its version and
compile options, and verify configured `journal_mode` and `synchronous` on open.
This must be checked, not assumed: SQLite notes that unknown pragmas can be
ignored ([SQLite pragmas](https://www.sqlite.org/pragma.html)). For the advertised
power-loss durability, acknowledge a command only after its transaction commits
with `synchronous=FULL`; `NORMAL` is allowed only with an explicit not-yet-durable
UI state. SQLite's atomic commit model provides the transaction boundary
([SQLite atomic commit](https://www.sqlite.org/atomiccommit.html)); its own
internals identify FULL as every-commit sync
([SQLite internals](https://sqlite.org/talks/howitworks-20240624.pdf)).

WAL, `-shm`, and database are one live unit; they are never moved or copied while
open. SQLite documents those persistent WAL side files and the 2026 WAL-reset fix
([WAL documentation](https://www3.sqlite.org/wal.html)). Archive only after a
verified checkpoint/close into a portable bundle. The durable frontier is visible:
after a normal acknowledgement it must recover exactly; a continuing drag may lose
only uncommitted transient position. On open, validate schema, hashes, SQLite
integrity and command chain; replay only complete transactions; preserve the last
good revision and present recovery choices rather than repairing silently. The
fault matrix kills or powers off before commit, after WAL commit, during snapshot,
checkpoint and archive; expected recovery is the last acknowledged revision, never
a silently altered head.

Probe imports in a restricted helper process with time/size/packet/dimension
limits. A missing asset, unsupported stream, corrupt frame, effect failure, or
plugin crash becomes a typed diagnostic bound to its asset/node/time. Preview
shows a deterministic missing-media slate and silence (or last-good frame only
when explicitly configured). Final export blocks by default and emits a complete
error report; `allow_missing` is an explicit export policy that burns the slate
and warning into its render manifest. Never hang, substitute a different asset,
or silently omit a failed effect.

### Effect/plugin boundary

Ship built-in effects first. Later third-party effects run in a versioned,
out-of-process plugin host, communicating only through a stable C ABI or
language-neutral IPC plus shared-memory/GPU-handle adapters. Do not expose Rust
or Go object layouts as the ABI.

An effect manifest must declare provider/version/build digest, parameter schema,
media types/alpha/color assumptions, temporal window, latency, thread safety,
determinism/random seed, state serialization, resource ceiling, and CPU/GPU
capabilities. The host applies conversions, owns scheduling and cache keys, and
enforces deadlines/cancellation. OpenFX is a credible compatibility target, not
the core model: it already requires effects to describe pixel/alpha/frame-rate
preferences ([OpenFX clip preferences](https://openfx.readthedocs.io/en/main/Reference/ofxClipPreferences.html))
and temporal frame requirements ([OpenFX frames needed](https://openfx.readthedocs.io/en/main/Reference/ofxImageEffectActions.html)).

## 4. Observability is part of the renderer

Every frame/audio request gets `trace_id, project_revision, semantic_plan_digest,
execution_plan_digest, semantic_mix_digest, quality_profile, request_generation,
time/range, backend path`. Emit structured spans for probe, I/O, demux, index,
decode, upload, node evaluation, composite, audio mix, present, encode, mux,
cache hit/miss, cancellation, and error. Export Chrome/Perfetto-compatible traces
plus a JSON render manifest.

Record p50/p95/p99 latency, queue depth/bytes, in-flight tokens, decoded/presented/
dropped frames by cause, audio underruns, A/V drift, cache hit rate and evictions,
RSS/VRAM, CPU/GPU utilization where available, hardware-vs-software codec path,
and recovery result. A benchmark/artifact is invalid when it lacks this metadata.

## 5. Reproducible 2K/3K benchmark contract

### Fixed fixtures and project recipes

Publish generator source, exact asset digests, project JSON, expected hashes, and
license. Do not publish a single marketing montage. The release suite is:

| ID | Exact source | Why |
| --- | --- | --- |
| `QHD-I` | MP4, 2560x1440, 30 fps CFR, AVC/H.264 all-IDR closed GOP=1, 8-bit 4:2:0 Rec.709 SDR + AAC 48 kHz stereo | edit-friendly 2K-class baseline |
| `QHD-LGOP` | MP4, 2560x1440, 60 fps CFR, AVC/H.264 8-bit 4:2:0 long-GOP + AAC 48 kHz stereo | common difficult random-access case |
| `3K-I` | MP4, 3072x1728, 30 fps CFR, AVC/H.264 all-IDR closed GOP=1, 8-bit 4:2:0 Rec.709 SDR + AAC 48 kHz stereo | mandatory 3K baseline |
| `3K-LGOP` | MP4/WebM capability probe, 3072x1728, 60 fps, HEVC Main or AV1 Main, 8/10-bit 4:2:0 + AAC/Opus | measured only; report codec separately |
| `VFR-AV` | MP4, 2560x1440 AVC/H.264 8-bit 4:2:0, exact variable PTS/duration manifest, AAC 48 kHz stereo with flash/chirp/impulse markers | timestamp and A/V oracle |

Each clip is 10 minutes of deterministic synthetic content (zone plate, moving
detail, gradients, alpha/title, cuts, discontinuities, color bars, timecode,
audio chirp and single-sample impulses). Every fixture manifest pins AVC profile,
level, bitrate, container timescale, encoder delay/padding, exact artifact digest,
and the separate raw WAV/linear-frame oracle input. The raw oracle, not AAC
decoder output, is the audio truth. A separate 2048x1080/24 DCI fixture is only
for interoperability; "2K" in headline results means QHD above. 10-bit 4:2:2
all-intra, MOV+PCM, ProRes, HEVC and AV1 are non-gating capability probes. License
the generator and recipe so sources can be regenerated; report OS codec component
and legal distribution dependency separately.

Create four fixed projects from those assets: `cuts` (200 edits, 8 tracks),
`effects` (four video layers, transform/opacity/dissolve/color and fades),
`audio` (16 clips, crossfades, gain automation, bus effect latency), and `stress`
(30 minutes, 16 active video layers plus proxy/thumbnail jobs). A project recipe
pins output 2560x1440 or 3072x1728, 30/60 fps, Rec.709 SDR, 48 kHz stereo.

### Measurements and oracles

Report individual samples, median, p95, p99, min/max, and coefficient of
variation--not a single best result. Randomly interleave repetitions, separate
cold and warm cache runs, and time GPU work with GPU timestamps where available;
this follows the sensible reporting/repetition guidance in
[Google Benchmark](https://google.github.io/benchmark/user_guide.html). Measure:

| Area | Metric | Correctness oracle |
| --- | --- | --- |
| Open/index | cold and warm open-to-editable; index bytes/time | all streams and PTS map match fixture manifest |
| Seek/scrub | request-to-correct-frame p50/p95/p99; 100 random seeks; cancellation lag | displayed frame watermark/time equals requested frame policy; no stale generation presented |
| Playback | presented FPS, dropped frames by cause, frame lateness, refresh-relative UI p99, audio underruns | semantic plan/mix digests match; A/V marker offset after device calibration |
| Export | wall time, realtime factor, encode/mux time, CPU/GPU/RSS/VRAM, output size | mandatory QHD and 3K AVC/AAC output; independent decoded comparison; container duration/PTS monotonic |
| Effects | semantic/execution graph compile/eval, cache hit, preview-render chunk time | semantic digest and golden frames/waveforms; execution digest disclosed |
| Proxy/cache | generation throughput, disk/RAM/VRAM, hit rate, invalidation | proxy parent/profile/map digest matches; capability path recorded; final export uses original |
| Memory/backpressure | owned-resource budget, process RSS/GPU observation, allocation count, max queue bytes, cancellation debt | every queue/token stays within configured bound; no orphaned work |
| A/V sync | worst/mean marker offset and long-run drift | offline sample marker exact; preview drift <=5 ms over 10 min after calibrated device offset |
| Recovery | kill at deterministic write/compact/export points; recovery time/lost committed commands | zero corrupt project; exact last durable revision; never a silently altered timeline |

CPU reference oracles use raw linear frames/audio generated from the canonical
graph and exact hashes where integer/pure operations permit. For floating GPU
paths, use fixed color-space conversions plus maximum absolute error, RMSE/PSNR,
and per-channel error thresholds established from the CPU reference *before*
comparison. `oracle-lock.json` is mandatory before any benchmark claim: it pins
hashes/versions for the fixture generator, canonical CPU evaluator, independent
demux/decode/container verifier, metadata checker and comparison tolerances. The
verifier is test-only if desired, but must not share the app's demux/mux
implementation. Export must be independently decoded and checked for frame count,
timestamps, duration, sync markers, color metadata and sample count; failed
frames/audio windows/traces are retained. A codec may be visually plausible yet
fail this contract.

### Run disclosure and comparative-claim rules

Each result publishes: app commit/build flags; fixture/project/expected-oracle
digests; OS build; CPU model/core policy; RAM; GPU/driver; storage model/free
space; display refresh; power mode; codecs and actual hardware/software path;
backend versions; cache state; concurrent load; all settings; raw samples;
traces; and failures. This mirrors the basic fairness principle that benchmark
claims need complete hardware/software disclosure
([SPEC CPU run rules](https://www.spec.org/cpu2026/docs/runrules.html)).

Cold = app/process stopped, caches purged by the harness, first project open.
Warm = project open once and fixed pre-roll completed; both must be reported.
Run at least 10 repetitions per machine/configuration; discard no result except
a documented harness/environment fault, retain failures, and report thermal/power
state. Pin no undocumented "benchmark mode," proxies, or codecs.

Permitted language before a public cross-product suite exists: "on [exact
hardware/config], this build achieved [metric] under this fixture." Forbidden:
"world's fastest," cross-application percentages without identical fixtures and
settings, and comparing proxy playback to another editor's original-media
playback. A regression is >5% median or a worse correctness result on the same
machine with >=10 valid paired runs; investigate it, do not silently reset the
baseline.

## 6. Minimum falsifiable vertical slice

Implement only enough to disprove a weak architecture:

1. **Early media I/O/export spike:** independently prove the exact `QHD-I`,
   `3K-I` and proxy AVC/AAC profiles can be demuxed, decoded, encoded and muxed
   on each acceptance machine. Record actual hardware/software paths, validate
   the output through `oracle-lock.json`, and fail closed on unsupported profiles.
2. Import `QHD-I`, `QHD-LGOP`, `3K-I`, and `VFR-AV`; calculate identity/index;
   make a two-video/two-audio-track sequence.
3. Apply split, trim, ripple move, a 12-frame dissolve, opacity/transform, a
   video fade, audio equal-power crossfade, gain keyframes, and one deterministic
   color effect. Persist/reopen and undo/redo all operations.
4. Compile semantic and execution plans to preview and final export; create proxy,
   seek/scrub/play 10 minutes, saturate thumbnail/proxy jobs, and kill/recover at
   scripted persistence boundaries.
5. Run all cold/warm measurements and emit trace, render manifest, and oracle
   report on one Windows and one macOS machine.

The slice passes functional acceptance only if canonical revision hashes match
across save/reopen/replay; 1,000 deterministic edit sequences end in the same
revision and export; undo/redo returns exact prior hashes; 10,000-frame rational
rate/VFR adapter tests pass; no stale seek result is presented; semantic plan and
semantic mix digests match between preview/export; the independent `oracle-lock`
verifier accepts mandatory QHD and 3K AVC/AAC exports on every acceptance machine;
missing/corrupt media does not crash or silently relink; and every injected crash
recovers the last durably acknowledged command.

Before hardware baselines exist, performance acceptance must test architecture,
not pretend to certify workstation speed. A checked-in baseline-machine manifest
states physical memory, unified/local GPU budget, display refresh, power mode and
cache limits. Require bounded queues/tokens with no leak after 100 seek
cancellations; editor-owned CPU-frame/GPU-texture/decode/queue bytes within those
hard budgets; <=2% second-run growth in tracked editor-owned resources; process
RSS/GPU process memory reported separately; and input-event-dequeue-to-command-
accepted p99 <= one reported refresh interval (16.7 ms only on a 60 Hz display)
while proxy/thumbnail work is saturated. Require zero audio underruns in 10-minute
`QHD-I` and `3K-I` proxy warm playback at both 128- and 256-frame tests; <=0.1%
late/dropped video frames; and preview A/V drift <=5 ms after calibration. On the
same acceptance machines, require QHD-I original-media and 3K-I proxy playback at
their declared rate, plus mandatory 3K AVC/AAC export. Treat 3K long-GOP original-
media, export realtime factor, and absolute seek latency as **measured baselines,
not release gates**, until at least two representative Windows and Mac machines
have published results. This exposes unbounded work, duplicate semantics, bad
cache invalidation and clock drift without inventing hardware-independent
milliseconds.
