# ADR-001: Production Architecture for the Vertical Slice

- Status: accepted after Terra High cross-review for vertical-slice implementation; not yet performance-certified
- Date: 2026-07-14
- Decision owner: project architecture council synthesis
- Evidence: the three research reports, `research/terra-high-attestation.md`, and the three `research/review-*.md` cross-reviews

## Decision

Build the production editor around a **Rust deterministic editor core**, a deliberately small stable C ABI, and a **Qt Quick desktop shell with thin C++20/Objective-C++ platform bridges**.

- Rust owns project/timeline truth, exact time, commands and undo/redo, render-plan compilation, scheduling, cache/proxy policy, audio mix semantics, persistence, observability, export plans, and the benchmark harness.
- Qt Quick owns windows, menus, panels, the timeline presentation, keyboard/mouse interaction, accessibility, DPI, and the native preview surface.
- The macOS bridge owns AVFoundation/VideoToolbox/CoreVideo/Metal objects and their lifetimes.
- The Windows bridge owns Media Foundation/D3D11 objects and their lifetimes; D3D12 is a later measured option, not a v1 requirement.
- A stable C ABI carries opaque core handles, immutable snapshots/diffs, command batches, POD metadata, callback vtables, and opaque bridge-owned frame-lease IDs. Rust never owns or casts a COM, Objective-C, Metal, D3D11, or Qt pointer. Raw video frames never pass through UI serialization or a WebView.
- MediaBunny remains a cited reference and optional narrow comparison harness for container/WebCodecs behavior. It is not the production timeline, compositor, audio engine, persistence layer, or universal codec stack.
- Go is excluded from the real-time engine. It may later be used for an isolated non-real-time service only if that creates a measured advantage.

This is an architecture baseline, not a “world's fastest” claim. It must be reopened if the zero-copy, correctness, portability, licensing, or packaging gates fail.

## Candidate Comparison

The council compared the following candidates under the same NLE-specific criteria: deterministic ownership and memory safety, decoder-to-GPU path, native API fidelity, desktop UI integration, build complexity, plugin isolation, packaging, and open-source licensing.

| Candidate | Preliminary weighted score | Decision |
| --- | ---: | --- |
| Rust core + C++20/Obj-C++ native bridge + Qt Quick | 92/100 | Selected for the vertical slice |
| C++20 core + Qt Quick | 87/100 | Fallback if the Rust/C ABI spike fails |
| Pure Rust core + Tauri control UI + native preview child surface | 85/100 | Rejected for v1 because preview/window/DPI integration remains an extra platform seam |
| Go core + Wails + native media bridge | 58/100 | Rejected for the real-time engine |

Electron + MediaBunny was evaluated separately as the most reproducible browser-media prototype because it pins Chromium. It was not selected as the production engine: WebCodecs does not require a universal codec set, hardware behavior varies, and browser-native GPU/media interop cannot be assumed copy-free. This decision supersedes the earlier Electron editor-prototype recommendation: any retained Electron/MediaBunny work is a fixture-and-oracle comparison harness only, owns no project, UI, persistence, preview-performance, or export authority, and cannot delay the native slice.

The scores are qualitative research estimates, not measurements. The vertical slice has authority to reopen this decision.

## System Boundaries

```text
Qt Quick UI thread
  commands, timeline view, panels, accessibility, native preview item
                  |
                  | stable C ABI: commands + immutable snapshots/diffs
                  v
Rust editor core
  command log -> immutable revision -> typed render plan
  asset identity/index | cache/proxy policy | export plan | telemetry
       |             |             |              |
       v             v             v              v
 I/O/demux pool   decode pool   background pool   persistence worker
       |             |          thumbnails/proxy        |
       +-------------+-------------------------------+  SQLite + project bundle
                     v
Platform media/GPU bridge
  macOS: VT/CV native YUV planes -> Metal color/effect pass -> owned RGBA texture
  Windows: MF/D3D11 native YUV planes -> D3D11 color/effect pass -> owned RGBA texture
                     |
       bridge-owned frame lease + device generation + completion primitive
                     |
       Qt scene-graph render thread imports 2D RGBA texture and presents

Audio device callback: preallocated Rust mix graph, audio clock is preview master
Plugin host process: later, versioned C/IPC protocol; never an in-process Rust ABI
```

Every asynchronous request carries the project revision and a cancellation epoch. Results from an obsolete seek/edit epoch are discarded. All queues are bounded by bytes and resource tokens, not merely item counts.

Canonical time is normalized rational time only. A platform adapter converts each canonical absolute PTS or duration once into `CMTime`, Media Foundation 100-nanosecond units, audio samples, or a container timescale using checked 128-bit arithmetic and a named rounding policy. It never accumulates rounded relative durations. Every adapter trace records canonical time, adapter tick, rounding policy, and exact rational error; output PTS must remain strictly monotonic with error bounded by half an adapter tick unless the platform requires a documented floor rule.

Preview and export share a `semantic_plan_digest` covering revision, ranges, time maps, effect order and sampled parameters, compositing, audio routing/automation, color intent, and missing-media policy. Each run also emits an `execution_plan_digest` that adds proxy/original choice, resolution, precision, backend, shader/encoder build, and cache policy. Semantic digests must match; execution digests are expected to differ. Audio uses an analogous shared `semantic_mix_digest` before device-only conversion and buffering.

The Qt preview is a render-thread contract, not a generic GPU handoff. The native bridge owns every `frame_lease`, platform surface, device-generation tag, fence/event, YUV-to-linear-working-RGBA pass, and retirement operation. Qt native scene-graph access occurs only on its render thread through the pinned documented integration path, with external commands bracketed where required. The bridge retains decoder backing storage and the RGBA texture through actual GPU completion. A device mismatch, stale generation, wrong backend, undocumented cross-thread access, or steady-state GPU-to-CPU frame transfer fails the architecture gate.

## Write From Scratch

Write only the parts that create a durable editor advantage:

1. Canonical schema-versioned project model with stable IDs, exact rational time, immutable revisions, typed commands, deterministic undo/redo, and explicit VFR/CFR/rounding policies.
2. Timeline compiler and typed demand-driven render graph shared by preview and export semantics.
3. Scheduler, cancellation epochs, backpressure, resource budgets, cache/proxy invalidation, and performance telemetry.
4. Deterministic block-based audio mixer, automation/fades, sync policy, and export audio path; use a wrapper only for device I/O.
5. Built-in effect nodes and the narrow Metal/HLSL effect kernels needed by the first product scope.
6. Project recovery, asset identity/relinking, render manifests, benchmark fixtures, correctness oracles, and regression harness.
7. Stable C ABI and later out-of-process plugin protocol.

## Reuse or Wrap

Do not reimplement codecs, GPU drivers, operating-system media stacks, windowing, database atomicity, or signing infrastructure.

- macOS: AVFoundation, VideoToolbox, CoreVideo, Metal, CoreAudio as thin native adapters.
- Windows: Media Foundation, D3D11, WASAPI as thin native adapters.
- UI: Qt 6 Quick and its native scene-graph integration.
- Audio device I/O: CPAL initially, with direct CoreAudio/WASAPI fallback if timestamp, device-change, or real-time guarantees are insufficient.
- Persistence: `ProjectStore v1`, a local single-writer bundled SQLite >= 3.51.3 database plus an external disposable cache directory. The store verifies its SQLite version/compile options, `journal_mode`, and `synchronous=FULL` on open. A command response is returned only after command, immutable revision, head, and the caller-stable `RequestId` receipt commit in one transaction. If the process dies after commit but before response, the result is intentionally ambiguous until the caller retries or queries that same `RequestId`; the store returns the committed receipt without applying the command twice. The live database, WAL, and SHM remain one unit until verified checkpoint/close; one worker owns writes and cancel-safe compaction.
- Hashing and diagnostics: audited open-source primitives with locked versions and SBOM entries.
- MediaBunny: reference implementation and optional benchmark comparator only. Do not use `@mediabunny/server` under a no-FFmpeg baseline because it wraps NodeAV/FFmpeg C APIs.

The slice-required ingest promise is exact: MP4 containing AVC/H.264 8-bit 4:2:0 Rec.709 video with AAC-LC 48 kHz stereo, plus separate WAV oracle sources and the fixed VFR fixture. The required output is AVC/H.264 + AAC-LC fast-start MP4 at QHD and 3072x1728. Each adapter reports container, sample entry, codec configuration, reader/writer, decoder/encoder, muxer, and hardware/software path. Every other MOV/MP4 sample entry is a typed unsupported-media result until corpus/fuzz evidence admits it. MOV+PCM, MP3, HEVC, AV1, HDR, 10-bit/4:2:2, RAW, ProRes, and third-party effects remain named non-gating probes.

## Open-Source and Licensing Direction

Default the initial repository and application to **GPL-3.0-or-later**, subject to legal review before publication. This keeps the fully open Qt route straightforward. If the project later chooses a more permissive application license, it must document and test Qt LGPL dynamic-linking/relinking compliance instead of assuming it.

MediaBunny is MPL-2.0, Electron is MIT, Tauri is MIT/Apache-2.0, and codec patents are separate from source-code licenses. Before any public binary or format-support claim, complete a dated legal/distribution review covering GPL/Qt obligations, H.264/AAC patent considerations in intended jurisdictions, system-framework reliance, third-party notices/SBOM, and the prohibition on bundling FFmpeg, NodeAV, or codec binaries. Publish the resulting supported-format/legal matrix with the binary.

## Risks and Fallbacks

| Risk | Required fallback or gate |
| --- | --- |
| Rust/C ABI complexity | Keep the ABI small, opaque, versioned, fuzzed, and ownership-explicit. Fall back to C++/Qt only if the spike proves the boundary unmaintainable. |
| Qt licensing | Use GPL-3.0-or-later by default; complete legal review before public distribution. Tauri remains the fallback UI shell if Qt obligations conflict with the project direction. |
| Hardware codec differences | Probe codec/profile/resolution at runtime, record the actual hardware/software path, provide proxies or explicit unsupported-media errors, and publish a real support matrix. |
| Native YUV/Qt incompatibility or accidental copies | Convert decoder NV12/P010 planes to an owned 2D RGBA texture in one intentional native GPU pass, validate color/range/transfer, count every transfer, and retain all backing surfaces through GPU completion. Any steady-state GPU-to-CPU frame transfer fails the zero-copy path. |
| D3D12 complexity or interop gaps | Use D3D11 for Windows v1. Evaluate D3D12 only after the D3D11 baseline and real decoder-surface tests exist. |
| Duplicate Metal/HLSL shaders | Keep the initial effect set small and paired. Evaluate Slang or wgpu only after native surfaces pass without new copies or regressions. |
| Codec/container attack surface | Restrict the first format matrix, sandbox/probe malformed imports in a helper process, use corpus/fuzz tests, and reuse audited parsers or OS readers rather than attempting an FFmpeg-scale rewrite. |
| Browser/prototype divergence | Keep MediaBunny outside project truth. Share only the canonical fixture/schema and oracle corpus; never make browser runtime state authoritative. |
| Packaging complexity | Test signed/notarized macOS and signed Windows packages on clean machines during the vertical slice, not at release time. |
| Sub-agent model requirement | All implementation/research sub-agents use GPT-5.6 Terra with high reasoning. Capture each returned thread ID and verify `model`, `reasoning_effort`, and `thread_source` from authoritative local thread metadata before accepting output; interrupt and discard unverifiable work. |

## Decision Gates

Keep this architecture only if the vertical slice proves all of the following on representative Windows and macOS machines:

- native decoder YUV planes reach a validated native GPU color pass, owned RGBA texture, and pinned Qt Metal/D3D11 scene graph with zero steady-state GPU-to-CPU frame transfers
- deterministic project hashes, undo/redo, save/reopen, rational-to-platform time adapters, and preview/export semantic graph digests pass the fixture oracles
- bounded queues and resource budgets survive repeated seek cancellation without leaks or stale frames
- the audio callback allocates and locks zero times, produces no underruns in the baseline run, and holds calibrated A/V drift within the stated gate
- 2K-class original-media and 3K-class proxy playback meet their declared rates on the baseline machines
- both acceptance machines produce independently verified 3072x1728 AVC/AAC fast-start MP4; an unsupported result fails and reopens the media scope
- demux/index/seek and encode/mux/cancel/atomic-publish behavior pass a dedicated media-I/O spike before shared-graph integration
- signed packages install and reopen the same project correctly on both operating systems

Absolute 3K long-GOP seek latency, original-media multilayer throughput, and export speed become measured baselines before they become release gates. Public comparative claims require identical fixtures, settings, hardware disclosure, raw samples, traces, and correctness results.

## Primary Evidence

- [MediaBunny technical overview](https://mediabunny.dev/guide/introduction)
- [WebCodecs specification](https://www.w3.org/TR/webcodecs/)
- [Apple VideoToolbox](https://developer.apple.com/documentation/videotoolbox)
- [Microsoft hardware Media Foundation transforms](https://learn.microsoft.com/en-us/windows/win32/medfound/hardware-mfts)
- [Qt Quick scene graph and native rendering](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html)
- [Rust FFI/ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html)
- [OpenTimelineIO architecture](https://opentimelineio.readthedocs.io/en/v0.12/tutorials/architecture.html)
