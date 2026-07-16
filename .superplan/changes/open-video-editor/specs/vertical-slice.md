# Vertical Slice and Acceptance Contract

- Status: revised by the attested Terra High council; implementation-ready after the dependency graph is reconciled
- Date: 2026-07-14
- Architecture: ADR-001
- Purpose: disprove weak architecture choices before expanding product scope

## Outcome

The slice is a signed, installable macOS and Windows desktop editor that can:

1. import the fixed QHD, 3K, and VFR fixtures and report the actual codec path;
2. create a deterministic two-video/two-audio-track project;
3. split, trim, ripple-move, stitch, and reorder clips;
4. apply transform, opacity, a 12-frame dissolve, a video fade, one deterministic color effect, clip gain, gain keyframes, and an equal-power audio crossfade;
5. seek, scrub, and play through the shared render graph while proxy and thumbnail work is saturated;
6. save, reopen, undo/redo, autosave, and resolve injected-crash recovery through durable idempotent command receipts; and
7. export QHD and 3K Rec.709 SDR deliverables whose decoded video, audio, timestamps, duration, and metadata pass independent oracles.

Anything beyond this list is out of scope for the slice. In particular: arbitrary third-party plugins, HDR, RAW, nested sequences, collaboration, AI features, and a universal container/codec matrix.

## Fixed Fixtures

The repository will contain generator source, recipes, manifests, expected semantic hashes, and license data. A 30-second development set exercises every marker quickly; the release set uses the same recipes at 10 minutes. Encoded-file digests are pinned per generated artifact, while decoded frame/audio oracles remain authoritative across platform encoder variations.

| ID | Source | Required role |
| --- | --- | --- |
| `QHD-I` | 2560x1440, 30 fps CFR, AVC/H.264 all-IDR, 8-bit 4:2:0, Rec.709 SDR, AAC-LC 48 kHz stereo in MP4; companion lossless WAV oracle | edit-friendly 2K-class original-media playback and export |
| `QHD-LGOP` | 2560x1440, 60 fps CFR, AVC/H.264 long-GOP, 8-bit 4:2:0, Rec.709 SDR, AAC 48 kHz stereo in MP4 | random-access, cancellation, and hardware-path measurement |
| `3K-I` | 3072x1728, 30 fps CFR, AVC/H.264 all-IDR, 8-bit 4:2:0, Rec.709 SDR, AAC-LC 48 kHz stereo in MP4; companion lossless WAV oracle | 3K ingest, proxy generation, playback, and export correctness |
| `3K-LGOP` | 3072x1728, 60 fps CFR, HEVC Main or AV1 Main as separately reported variants, 8/10-bit 4:2:0, Rec.709 SDR, AAC/Opus 48 kHz stereo | capability probe and measured baseline; not a release performance gate |
| `VFR-AV` | 2560x1440 AVC/H.264 in MP4 with alternating exact PTS/durations, AAC-LC 48 kHz stereo, companion lossless WAV, and numbered flash/chirp/single-sample markers | exact-time, VFR, resampling, and A/V-sync oracle |

Every baseline manifest pins AVC profile/level, chroma, bitrate, GOP structure, closed-GOP/random-access flags, AAC encoder delay/padding, container timescale, sample entry/configuration, exact artifact hash, and decoded-semantic oracle. Every fixture contains deterministic frame numbers, exact rational timestamps, moving detail, gradients, color bars, hard cuts, dissolve boundaries, and audio chirp/impulse markers. Unsupported optional variants must return a typed capability result; they must not be silently transcoded or decoded through an undisclosed backend.

The broader 10-bit 4:2:2 all-intra research fixtures remain post-slice capability probes because ADR-001 deliberately narrows the first supported format promise to system-supported 8-bit 4:2:0 AVC plus common audio.

### Proxy profile

The required proxy is 1280 pixels wide, square-pixel, CFR, AVC/H.264 8-bit 4:2:0 Rec.709 SDR with every frame an IDR/random-access point, AAC-LC 48 kHz stereo, and MP4 containment. Its manifest pins codec configuration, GOP structure, parent asset identity, profile digest, source-to-proxy PTS map, audio delay/padding, and proxy digest. Pre-generated proxies prove playback; proxy generation is a separate capability and throughput gate. Final export must use the original unless a visibly labeled draft mode is selected.

### Export profiles

- `QHD-final`: 2560x1440 at 30 fps CFR, AVC/H.264 High, 8-bit 4:2:0 Rec.709 SDR, AAC-LC 48 kHz stereo at 320 kb/s, MP4 fast-start.
- `3K-final`: 3072x1728 at 30 fps CFR, AVC/H.264, 8-bit 4:2:0 Rec.709 SDR, AAC-LC 48 kHz stereo at 320 kb/s, MP4 fast-start. This profile is required on every named acceptance machine; record the selected profile/level and hardware/software path. An unsupported result, failure to independently decode, or fallback to the lossless oracle intermediate fails the slice and reopens ADR-001.
- `oracle`: canonical decoded linear-light RGBA frames plus 32-bit float 48 kHz audio blocks and an exact timestamp/metadata manifest. This is the correctness reference, not a user deliverable.

Encoder bitrate is recorded by the harness and fixed before baseline publication. Hardware and software encode results are never pooled.

## Canonical Project Recipe

The fixed sequence has two video tracks and two audio tracks. It includes 200 deterministic edits across the long fixture, a 12-frame dissolve, transform and opacity automation, a video fade, one versioned color-matrix effect, an equal-power audio crossfade, and gain keyframes. All time is normalized rational time; source VFR timestamps are retained.

The same canonical revision compiles to both preview and export graphs. `QualityProfile` may change resolution, proxy source, pixel format/precision, backend, cache policy, and implementation quality, but never edit order, time mapping, effect order, automation sampling, compositing, color intent, missing-media policy, audio routing, or audio semantics.

Every plan emits two identities. `semantic_plan_digest` includes the revision, ranges/time maps, effect order and sampled parameters, compositing, audio routing/automation, color intent, and missing-media policy and must match between preview and final export. `execution_plan_digest` adds proxy/original selection, resolution, pixel format/precision, backend, shader/encoder build, and cache policy and may differ. Audio similarly emits a shared pre-device `semantic_mix_digest`.

Canonical time is always a normalized rational. Each platform adapter converts canonical absolute PTS/duration once into its integer unit using checked 128-bit arithmetic and a named rounding policy; it never accumulates rounded relative durations. Traces include canonical time, adapter tick, rounding policy, and exact rational error. PTS must remain monotonic and conversion error may not exceed half an adapter tick unless a documented platform floor rule applies. Audio mix positions remain integer sample indices at 48 kHz.

`ProjectStore v1` is a local single-writer SQLite >= 3.51.3 store. It verifies version, compile options, journal mode, and `synchronous=FULL` on open. An edit is acknowledged only after command, immutable revision, and head commit atomically. One worker owns writes and cancel-safe compaction; the live database, WAL, and SHM remain one unit until verified checkpoint/close. Live network/shared-filesystem projects are unsupported until separately validated.

## Functional Acceptance

The slice fails if any requirement below is skipped:

- canonical project and revision hashes match across command replay, save/reopen, and both operating systems;
- 1,000 seeded edit sequences produce the expected final revision and oracle export; undo/redo returns the exact prior hashes;
- all half-open ranges, 23.976/29.97/59.94 rationals, VFR PTS, and audio sample positions pass overflow and rounding-policy tests, including 10,000-frame adapter round trips with monotonic PTS and bounded recorded error;
- 100 rapidly superseded seeks never present a stale generation, leak work, or exceed a declared queue/token bound;
- preview and export produce the same `semantic_plan_digest` and `semantic_mix_digest` for the same revision/range; each emits its distinct complete `execution_plan_digest`;
- decoded export has the expected frame/sample count, monotonic PTS, duration, sync markers, color metadata, and independent visual/audio comparison result;
- missing, moved, corrupt, and unsupported media produce deterministic diagnostics without crash, silent relink, effect omission, or asset substitution;
- crash injection before commit, after WAL commit but before response, during snapshot, during checkpoint, and during archive proves the idempotent receipt protocol: pre-commit failure leaves the prior head and no receipt; post-commit ambiguity retains the new head and receipt, and retrying the same `RequestId` returns it without reapplying; no returned receipt is lost and no edit is partially applied or silently altered;
- decoder NV12/P010 planes reach a native Metal/D3D11 color/effect pass, validated 2D RGBA texture, and Qt render-thread import through a bridge-owned frame lease with zero steady-state GPU-to-CPU frame transfers; every transfer is counted and traced;
- the real-time audio callback performs zero locks, allocations, disk I/O, waits, or logging calls; and
- signed packages install on clean machines, open the same fixture project, export it, and preserve its canonical hash.

Integer/pure operations require exact hashes. Floating GPU paths use predeclared per-channel maximum error and RMSE/PSNR tolerances derived from the CPU oracle before candidate results are inspected. A checked-in `oracle-lock` manifest pins the independently implemented fixture generator, canonical CPU evaluator, demux/decode/container verifier, metadata checker, tolerance set, and their versions/hashes. The verifier must not share the app's demux/mux implementation. Intentional time-map, color, audio-fade, and stale-epoch faults must each fail the appropriate oracle. A visually plausible frame is not sufficient.

## Performance Acceptance

These gates test architecture on the named baseline machines; they are not universal workstation claims.

| Area | Required threshold |
| --- | --- |
| UI responsiveness | input-event dequeue to command accepted p99 <= one reported display refresh interval while proxy and thumbnail jobs are saturated; 16.7 ms applies only at 60 Hz |
| QHD original playback | `QHD-I` warm playback at 30 fps for 10 minutes, <= 0.1% late/dropped video frames |
| 3K-source proxy playback | `3K-I` through the fixed proxy at 30 fps for 10 minutes, <= 0.1% late/dropped video frames |
| Audio | zero underruns during both 10-minute playback runs plus the 16-active-track mixer stress run at recorded 128- and 256-frame device buffers |
| A/V sync | preview marker drift <= 5 ms after measured device-offset calibration; offline markers exact to the sample |
| Cancellation | all work from 100 superseded seeks drains or cancels within declared bounds; no stale presentation |
| Memory | checked-in machine-specific editor-owned CPU-frame, GPU-texture, decoder, queue, and cache byte budgets are never exceeded; tracked editor-owned retention grows <= 2% after a second identical run; process RSS and GPU-process/unified memory remain required observations |
| Backpressure | every queue has a configured byte and item limit; observed maxima stay within it and producers coalesce or wait when full |
| Zero-copy | zero steady-state GPU-to-CPU readbacks in the native preview path after warm-up |
| Reliability | zero project corruption, zero lost returned receipts, and idempotent resolution of every commit-before-response ambiguity across all scripted crash points |

Cold/warm open time, QHD/3K long-GOP seek p50/p95/p99, cancellation latency, 3K original-media throughput, proxy generation speed, export real-time factor, CPU/GPU utilization, power, RSS/VRAM, cache hit rate, and package size are mandatory measurements but not release thresholds until results exist on at least two representative machines per operating system.

A same-machine paired regression is a correctness regression or greater than 5% worse median performance over at least 10 valid, randomly interleaved repetitions. Results are retained even when they fail.

## Platform Validation and Disclosure

At least two machines per OS are required before performance certification. The first slice may establish provisional baselines on one machine per OS but cannot support a comparative marketing claim.

Each run records:

- application commit, dirty state, build flags, dependency lock and SBOM digest;
- fixture, project, proxy profile, render graph, and oracle digests;
- OS build and security/graphics updates;
- CPU model, physical/logical cores, scheduler/power mode, and thermal state;
- RAM capacity/speed where available;
- GPU model, VRAM/unified-memory budget, driver, Metal feature set or D3D feature level;
- storage model/interface, filesystem, free space, and project/cache volumes;
- display resolution/refresh/DPI and Qt rendering backend;
- codec/profile/level, actual hardware/software decoder and encoder path, and native surface format;
- audio device, sample rate, buffer size, measured device offset, and driver mode;
- cache state, background load, network state, raw repetitions, failures, logs, traces, and render manifests.

macOS validation proves VideoToolbox/CoreVideo YUV plane views, Metal color conversion, bridge-owned frame lifetime through command-buffer completion, and Qt Metal render-thread import. Windows validation proves Media Foundation/D3D11 YUV surfaces, same-device color conversion to owned RGBA, bridge-owned lifetime/fence retirement, Qt D3D11 render-thread import, device-loss recovery, COM apartment correctness, and hardware-MFT selection. Both assert the pinned Qt/native-interface version and actual Metal/D3D11 backend, reject software/WARP for the zero-copy gate, and run the same editorial/oracle corpus.

Audio validation records 48 kHz stereo at 128- and 256-frame buffers, shared/exclusive mode and fallbacks, measured device offset, 16 active tracks, fades, gain automation, one declared-latency effect, device unplug/reopen, and device-clock discontinuity behavior. Allocator, lock, I/O, logger, wait, and unwind intercept counters around the callback must all remain zero.

## Implementation Evidence Graph

The next graph follows the research risks instead of decomposing an imagined full editor:

1. attest GPT-5.6 Terra/high metadata and accept the council revisions;
2. bootstrap one repository, pinned toolchain, license, build graph, and CI contract before parallel writers begin;
3. build three independent foundations in parallel: deterministic editorial core, benchmark/oracle corpus, and Qt/C ABI/frame-lease shell contract;
4. use those foundations for separate macOS video, Windows video, real-time audio, and demux/index/encode/mux falsification spikes;
5. proceed to the shared preview/export graph only after both native zero-copy paths, the audio callback, and media-I/O/export pass; and
6. integrate, package, and publish raw baseline evidence before expanding scope.

The graph intentionally stops at the vertical slice. Product breadth is reshaped only after its evidence exists.

## Stop and Reopen Rules

Reopen ADR-001 rather than forcing implementation if:

- the Rust/C ABI cannot express ownership safely without frequent raw-frame crossings;
- either native decoder-to-preview path needs a steady-state CPU readback;
- Qt licensing or native-scene-graph integration conflicts with the intended open-source distribution;
- the audio callback cannot meet the real-time contract through the selected boundary;
- preview and export require divergent editorial semantics; or
- packaging cannot reproduce the same project hash and oracle result on clean machines.

Fallback order is narrow C ABI redesign, direct C++/Qt core for proven boundary failure, then a Tauri native-preview shell only for a proven Qt distribution failure. Go and a WebCodecs-only production engine remain excluded unless new measurements overturn the evidence.

## Claim Policy

Until an identical public cross-editor suite exists, permitted language is only: “On this disclosed machine and configuration, this build achieved this metric under this fixture.” “World's fastest” or “most efficient” is forbidden without correct, reproducible, like-for-like evidence.
