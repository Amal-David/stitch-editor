# Native architecture council: Windows/macOS NLE

**Status (2026-07-14): council-revised architecture proposal.** This revision incorporates the [Terra High attestation](terra-high-attestation.md), [native-surface cross-review](review-native-architecture.md), [MediaBunny/Web review](review-media-web.md), and [NLE-performance review](review-nle-performance.md). It is an architecture hypothesis to falsify with the listed spikes, not a completed performance claim.

## Decision in one sentence

Build a **Rust deterministic editor core behind a small, versioned C ABI, with a C++20/Objective-C++ platform shell using Qt Quick**. Rust owns editorial truth, planning, scheduling, persistence, cache policy, and the realtime mix; the platform bridge owns all Qt, COM, CoreFoundation, and GPU-object lifetimes. Do not use Go for the media engine, and do not transport decode, preview, audio, or export data through a WebView.

The scores below are qualitative decision aids for this Windows/macOS NLE, not benchmarks or precise performance margins. Candidate A is recommended because it keeps native video objects in their native APIs while giving the large mutable editor model deterministic, memory-safe ownership.

| Candidate | Shape | Main trade-off | Qualitative result |
| --- | --- | --- | --- |
| **A. Recommended: Rust core + C++20/Obj-C++ shell + Qt Quick** | Rust timeline/scheduler/cache/mix/render-plan; versioned C ABI; Qt UI; native Apple/Windows bridges. | Two toolchains and an FFI/device-lifetime boundary. | **92/100** |
| B. Rust core + native preview surface + Tauri | Rust engine with WebView UI and separately managed native preview. | Fewer engine languages, but window/focus/DPI/native-surface integration remains a hard seam. | 85/100 |
| C. C++20 + Qt Quick | One C++ engine/UI/platform implementation. | Direct platform access, but manual-memory/concurrency cost across the whole editor. | 87/100 |
| D. Go + Wails + C/C++ bridge | Go control plane with a WebView shell and native media bridge. | cgo/GC/thread affinity make native frame and callback ownership an avoidable hot-path risk. | 58/100 |

## Why Rust, and why not Go, for the core

Rust is selected for bounded ownership of decoded frames, GPU completion, audio buffers, and cancellation epochs—not for an abstract language-speed claim. The public boundary is C because Rust does not guarantee a stable native Rust ABI. [Rust ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html)

Go can be effective for batch metadata work, but cgo imposes pointer-retention and pinning rules for memory retained by C; its GC and OS-thread affinity would make realtime native-object ownership a special case throughout the engine. That is an unnecessary cost for this design, not a statement that Go cannot process media. [cgo pointer rules](https://pkg.go.dev/cmd/cgo) · [Go GC guide](https://go.dev/doc/gc-guide) · [LockOSThread](https://pkg.go.dev/runtime#LockOSThread)

MediaBunny remains a behavior/container-design reference and bounded browser comparator, not the production runtime. Its server path uses NodeAV/FFmpeg and is therefore excluded by the no-FFmpeg production constraint. [MediaBunny server](https://mediabunny.dev/guide/server-side-usage)

## System boundary and ownership contract

~~~text
Qt Quick UI (commands, panels, timeline; never raw video frames)
        | versioned C ABI: commands, snapshots, opaque IDs, POD metadata
Rust editor core
  command log/reducer -> immutable TimelineVersion -> semantic render plan
  asset index, demux/index policy, cache scheduler, audio mixer, export plan
        | opaque bridge-owned frame leases; no native pointers cross this boundary
Platform bridge
  macOS: VT decode -> CVPixelBuffer YUV planes -> Metal color/effect pass
  Windows: MF MFT -> D3D11 YUV planes -> D3D11 color/effect pass
        | owned 2D RGBA native texture -> Qt import -> present
~~~

The actual preview path is deliberately **not** decoder texture directly into Qt. Decoder output is normally NV12/P010. On each OS the bridge samples the native YUV planes in a GPU-resident color/effect pass, producing an owned 2D RGBA texture that Qt imports for presentation. This has one explicit GPU pass and **zero steady-state GPU-to-CPU frame transfers**. It must carry source order, color primaries, matrix, range, and transfer metadata; Rec.709 limited/full-range tests are mandatory before claiming color correctness.

Qt's supplied Metal and D3D11 wrappers are non-owning, render-thread-only, and limited to 2D RGBA textures, so the bridge—not Qt and not Rust—retains decoder resources through completion. [Qt Metal native texture](https://doc.qt.io/qt-6/qnativeinterface-qsgmetaltexture.html) · [Qt D3D11 native texture](https://doc.qt.io/qt-6/qnativeinterface-qsgd3d11texture.html)

### C ABI and frame leases

The C ABI is a control plane, never a graphics interchange format. It passes versioned, fixed-width POD values and opaque handles only. A frame lease contains a bridge-owned reference, creator domain, device generation, timeline version/epoch, immutable frame metadata, and retirement state. Rust can request, submit, and release a lease; it cannot cast it to a Core Video, Metal, D3D11, fence, or Qt object.

Frame-lease release schedules destruction on the owning platform/render thread only after the corresponding Metal command-buffer completion or D3D11 fence. Device loss increments the generation and stale leases fail typed; no C++ exception or Rust panic may cross the ABI. Qt native resources are exposed only from its scene-graph render context and are not owned by the caller. [QSGRendererInterface](https://doc.qt.io/qt-6/qsgrendererinterface.html)

### Qt external-command contract

Pin an exact Qt build/version and shader toolchain for the vertical slice; Qt does not promise source or binary compatibility for native interfaces. [Qt native interfaces](https://doc.qt.io/qt-6/qnativeinterface.html) Create or adopt the native graphics device before scene-graph initialization. Assert Metal on macOS and D3D11 on Windows after initialization; WARP/software or any other backend fails the zero-copy gate.

All native Qt interaction occurs on the scene-graph render thread, inside the appropriate render callback and beginExternalCommands/endExternalCommands scope. The bridge must not cache a Qt-provided native pointer outside that scope, and the same device is used by Media Foundation or Metal and Qt. Re-run the native-surface spikes when the pinned Qt native-interface version changes. [QQuickWindow graphics device](https://doc.qt.io/qt-6/qquickwindow.html)

## Deterministic editor semantics

Use an append-only command log and pure reducer to produce immutable TimelineVersions. Every asynchronous request/result carries asset identity, timeline version, epoch, and request key; stale work is discarded. Undo/redo changes commands or snapshots, never decoder state.

Canonical editorial time is **normalized rational time only**. Integer ticks are allowed solely as private cache/index accelerators, not as project or render-plan truth. CoreMediaTimeAdapter and Mf100nsTimeAdapter each convert a canonical **absolute** PTS/duration once, with checked 128-bit arithmetic and a named policy (nearest-ties-to-even, or explicitly logged floor where the platform API requires it). They never accumulate rounded relative durations. Every conversion records canonical rational time, adapter tick, policy, and error numerator/denominator; output PTS is strictly monotonic with documented bounded error. Audio mix positions remain integer 48 kHz sample indices. Container timestamps originate from canonical output time and track timescales, not reconstructed decoder callback times. Media Foundation sample times are integer 100-ns units. [MF timestamps and durations](https://learn.microsoft.com/en-us/windows/win32/medfound/time-stamps-and-durations)

Compile each version into separate immutable digests:

- **semantic plan digest:** timeline maps, effect ordering/parameters, compositing, audio routing/automation, color intent, and missing-media policy. It must match preview and export for the same revision/range.
- **execution plan digest:** semantic digest plus original/proxy choice, resolution, pixel format, backend/shader/encoder build, and cache policy. It may differ, and is emitted in every trace/manifest.

This prevents a proxy or backend selection from concealing a real editorial difference while preserving one shared semantic graph for preview and export.

## Platform media, graphics, and audio

On macOS, VideoToolbox produces Metal-compatible pixel buffers. Retain the buffer and its Core Video/Metal views until GPU completion; Apple documents a live relationship between the Core Video texture and underlying Metal texture. [CVMetalTextureCacheCreateTextureFromImage](https://developer.apple.com/documentation/corevideo/cvmetaltexturecachecreatetexturefromimage(_:_:_:_:_:_:_:_:_:))

On Windows v1, use a video-support D3D11 device and pass its DXGI manager to D3D-aware Media Foundation transforms. Do not call IMFMediaBuffer Lock on the steady preview path because it can require a contiguous memory copy. [MF DXGI device manager](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mfcreatedxgidevicemanager) · [media buffers](https://learn.microsoft.com/en-us/windows/win32/medfound/uncompressed-video-buffers)

D3D12 is a later benchmarked experiment, not a v1 requirement: it adds explicit resource-state/synchronization responsibility, and D3D11-on-12 has documented CPU and memory overhead. [D3D11-on-12](https://learn.microsoft.com/en-us/windows/win32/direct3d12/direct3d-11-on-12)

The preview audio master is a **Rust-owned CPAL output callback** invoking a preallocated MixerRt context with bounded lock-free rings. It performs no allocation, lock, wait, disk/network I/O, logging, UI call, ownership change, or panic unwind. If CPAL cannot provide the required device-change/timestamp behavior, a minimal native trampoline may replace it only after the same contract and evidence pass. Device conversion/buffering occurs after the shared semantic mix; offline export compares against the same canonical PCM oracle. [CPAL](https://github.com/RustAudio/cpal) · [Apple AURenderCallback](https://developer.apple.com/documentation/audiotoolbox/aurendercallback)

## Narrow media scope and early falsification plan

Do not write codecs, drivers, or a universal FFmpeg replacement. Wrap system decode/encode capability, write the editor-facing packet index/seek policy, and admit a deliberately small MP4/MOV/container scope only after corpus/fuzz and independent-verifier evidence. The first baseline fixture should pin its container, codec/profile/level, chroma, GOP, audio delay/padding, track timescale, and hash; nominal frame rate never overrides the authoritative VFR PTS manifest.

Add a **Media I/O and export spike** after fixture/oracle work and before the shared preview/export integration task. It must prove demux, long-GOP and VFR index/seek behavior, hardware/software encoder selection, DTS/PTS/timebase ordering, interleaved muxing, independent parse/decode, cancellation cleanup, and atomic final rename. Shared integration depends on this spike as well as the macOS, Windows, and audio spikes.

The vertical-slice recipe is two video tracks and two audio tracks—not one of each—so it can falsify dissolve/composite and crossfade/mix ownership. It is limited to trim/stitch/merge, 12-frame dissolve, video fade, transform, opacity, deterministic color effect, gain/keyframes/equal-power crossfade, proxy/original selection, reopen/recovery, and MP4 export. The baseline acceptance machine on **each** OS must produce and independently verify a 3K 8-bit AVC/AAC export. HEVC/AV1 remain optional capability-gated profiles; a typed unsupported result is not a substitute for that baseline deliverable.

Before integration, collect these hard evidence bundles:

1. Same-device externally created RGBA texture imported by Qt with scene-graph-init ordering, render-thread ownership, completion retirement, resize, DPI, and stale-device-generation rejection.
2. Actual AVC decode YUV-plane to GPU-RGBA conversion on both OSes, including color/range oracle, no-readback trace, and Windows device loss/recreate.
3. 10,000-frame 23.976/29.97/59.94 plus VFR rational-time adapter tests.
4. Ten-minute 16-track 48 kHz playback at recorded 128/256-frame buffers, zero underruns and zero allocator/lock/I/O/logger counters in the callback, including device loss/reopen.
5. Independent fixture/oracle lock manifest with pinned generators, CPU reference, demux/decode/container verifier, tolerances, and intentional-bug tests.
6. Signed clean-machine packages with backend assertion, raw traces, hardware manifest, fixture hashes, SBOM, and failures retained.

## Persistence, plugins, and distribution

ProjectStore v1 is a local single-writer SQLite database. A UI command is acknowledged only after the command, immutable revision, and head pointer commit under the documented durability setting. For the stated power-loss guarantee use synchronous=FULL; persist the database, WAL, and SHM as a unit, and run crash tests at commit/checkpoint/archive boundaries. Network/shared filesystem operation is unsupported until separately validated.

Third-party codecs, importers, AI tools, and arbitrary native effects run in a separate plugin-host process behind a stable C/message protocol. Built-in effects remain declarative render nodes. Normal hardware preview remains in-process so cross-process GPU transfer cannot undermine the zero-copy contract.

Before a public binary or format-support claim, complete a dated legal and distribution gate covering the selected Qt GPL/LGPL obligations, H.264/AAC patent considerations in intended jurisdictions, reliance on system frameworks, third-party notices/SBOM, and the explicit ban on bundling FFmpeg, NodeAV, or codec binaries. Publish a supported-format/legal matrix with the binary. Qt's open-source licensing route must be chosen for the exact distribution model, not inferred from the application's intended GPL license. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)

Sign/notarize nested macOS components with hardened runtime; sign Windows executables/installers; and validate both on clean machines. Qt deployment tools help stage dependencies but do not replace signing, notarization, or third-party review. [Qt macOS deployment](https://doc.qt.io/qt-6/macos-deployment.html) · [Qt Windows deployment](https://doc.qt.io/qt-6/windows-deployment.html)

## Decision gates

Retain candidate A unless evidence falsifies it. Reopen the architecture if a same-device native decode-to-RGBA-to-Qt path cannot meet the zero-CPU-transfer, ownership, and device-loss rules; if the realtime callback violates its instrumented constraints; or if the mandatory baseline 3K AVC/AAC export cannot be produced on either acceptance platform without violating the distribution gate. Until identical public cross-editor benchmarks exist, report only metrics for disclosed fixtures, builds, and machines—never “world's most efficient” or similar comparative claims.
