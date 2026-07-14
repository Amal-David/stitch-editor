# MediaBunny / browser media-stack assessment

**Status (revised 2026-07-14): browser-comparator research, not a production approval.** The [Terra High attestation](terra-high-attestation.md) now verifies this council thread; the earlier provisional model-tier caveat is removed. This revision incorporates the required disposition from the [media/web cross-review](review-media-web.md): Electron/MediaBunny is comparison-only and cannot become a parallel editor.

## Decision in one paragraph

The Electron-editor prototype recommendation is **superseded**. Retain a **narrow, pinned-Chromium Electron + MediaBunny/WebCodecs comparison harness** only: it measures browser container/codec behavior against fixed shared fixtures and oracles. It owns **no project state, editor UI, persistence, timeline/render-plan authority, preview authority, or user export authority**. MediaBunny remains a useful web demux/mux and WebCodecs orchestration layer, but is not an NLE timeline, compositor, audio engine, project database, or universal codec stack. The production direction is a **Rust native media/render core with a native Qt Quick shell**, with native media/GPU paths and a project/timeline schema independent of the harness. Tauri remains potentially useful only as a future shell if all hot media paths are native; its Windows WebView2 and macOS WKWebView make a MediaBunny/WebCodecs renderer non-uniform.

**Production constraint:** do not ship `@mediabunny/server`, NodeAV, an FFmpeg CLI, FFmpeg libraries, or bundled codec binaries in the product. `@mediabunny/server` is backed by NodeAV and FFmpeg's C APIs, so it fails this constraint even though it presents a cleaner TypeScript API. Production code relies only on declared platform-framework media adapters and separately approved libraries. [Mediabunny server extension](https://mediabunny.dev/guide/extensions/server)

## What each layer owns

| Layer | Comparator harness (Electron + MediaBunny) | Durable production boundary |
|---|---|---|
| Container demux/mux | Reads fixed fixture assets and records MediaBunny demux/mux behavior. It may write only ephemeral comparator artifacts chosen by the native test runner. | A replaceable `MediaIO` interface with platform-specific reader/writer contracts. |
| Decode / encode | Runs WebCodecs probes against exact test configurations; records available/unavailable configurations and selected path. | Platform codec adapters behind `Decoder`/`Encoder` traits; capability probe is persisted per machine. |
| Timeline scheduling | **Out of scope.** It receives a fixed test manifest only. | The native core owns immutable project state, edit graph, time mapping, keyframe index, look-ahead, proxy policy, and cancellation. |
| Effects / compositing | **Out of scope.** At most, isolated frame-import/copy diagnostics. | GPU render graph with explicit color, texture lifetime, fence and native-surface ownership. |
| Audio mixing | **Out of scope.** It can decode fixed audio fixtures and report sample metadata. | Native mixer/DSP and offline render own mix semantics. |
| Preview | **Out of scope.** No user-facing preview or editor UI. | Native preview engine with the documented Qt/native GPU-sharing boundary; never serialize raw frames through WebView IPC. |
| Export | **Out of scope.** It has no user export authority; only native-runner-controlled diagnostic artifacts. | Native renderer/encoders, export journal, resumable checkpoints, and atomic final rename. |
| Project persistence | **Out of scope.** No `.stitch`, SQLite, browser storage, or source-file authority. | Versioned project schema and migrations owned by the core. |

MediaBunny itself is intentionally scoped to container I/O plus abstractions around WebCodecs. Its documentation describes per-container multiplexers/demultiplexers, pipelined decoder/encoder wrappers, conversions, trimming, transforms and custom per-sample processing; it does not provide an edit timeline or multi-clip compositing model. The harness therefore tests the boundary, not an editor. [MediaBunny technical overview](https://mediabunny.dev/guide/introduction) · [conversion API](https://mediabunny.dev/guide/converting-media-files) · [media sources](https://mediabunny.dev/guide/media-sources)

## MediaBunny: fit, limits, and licensing

### Good fit

- It handles the missing layer that WebCodecs deliberately omits: demuxing encoded chunks from files and muxing encoded chunks into playable files. [WebCodecs overview](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API)
- Its container support is broad for a creator MVP, and it understands AVC/H.264, HEVC/H.265, VP8/VP9, AV1 and ProRes identifiers; audio includes AAC, Opus, MP3, Vorbis, FLAC, AC-3/E-AC-3 and PCM families. Built-in PCM is always available; other browser coders are conditional. Ask `track.canDecode()` and `canEncode*()` with the real resolution, bitrate, channels and sample rate before promising import or export. [supported formats and codecs](https://mediabunny.dev/guide/supported-formats-and-codecs)
- Inputs are lazy and range-read. `BlobSource(File)` is suitable for a user-selected disk file and has an 8 MiB default cache; custom sources can apply file-system or network prefetch strategies. This avoids loading a multi-gigabyte source into JavaScript memory. [input sources](https://mediabunny.dev/guide/reading-media-files)
- Outputs can stream to a `WritableStream`; backpressure reaches the output and encoders. This is required for large exports. `BufferTarget` is explicitly only appropriate for small files (the guide suggests below 100 MB); do not use it for video export. Feed audio and video in interleaved time chunks because certain multi-track formats otherwise buffer packets in memory. [writing and output targets](https://mediabunny.dev/guide/writing-media-files) · [stream target](https://mediabunny.dev/api/StreamTarget)
- It exposes elementary encoded-packet, decoded-video-sample and decoded-audio-sample paths, which is exactly the control surface an editor needs. Its canvas sink can reuse a canvas pool, but the docs explicitly flag additional framebuffer VRAM use. [media sinks](https://mediabunny.dev/guide/media-sinks)

### Boundaries and failure modes

- **No automatic NLE semantics.** Stitching compatible clips can be packet-copy/transmux work only when codec, track configuration, timebase and output-container rules permit it. A cut away from a keyframe needs decode/re-encode or a GOP-aware edit policy. Overlaps, fades, transforms, text and effects require a frame render plan; audio overlap requires a sample mixer.
- **WebCodecs is a capability, not a codec promise.** A recognized source codec can still be undecodable; a format-compatible output codec can still be unencodable. Probe exact configurations, log the selected fallback, and surface a precise unsupported-media result rather than a generic export failure. [MediaBunny codec probes](https://mediabunny.dev/guide/supported-formats-and-codecs) · [WebCodecs codec selection](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API/Codec_selection)
- **Resource pressure is normal.** `VideoFrame`/`AudioData` hold CPU/GPU memory and scarce codec resources. The WebCodecs specification says to close them promptly and permits the UA to reclaim codecs with `QuotaExceededError`; raw frames are transferable, so worker hand-off need not copy the underlying resource when transferred correctly. Bound decode, effect and encode queues; close on every success, cancellation and error path. [WebCodecs memory model and reclamation](https://www.w3.org/TR/webcodecs/)
- **Worker queues are mandatory.** Decode/export must not run on the UI thread. WebCodecs works in dedicated workers; `OffscreenCanvas` can be transferred to a worker. Transfer `VideoFrame` rather than `copyTo()` raw pixels; `copyTo()` is a real CPU-memory copy. [WebCodecs API](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API) · [OffscreenCanvas](https://developer.mozilla.org/en-US/docs/Web/API/OffscreenCanvas)
- **Do not assume one copy-free GPU pipeline.** WebGPU can import a `VideoFrame` as a `GPUExternalTexture`, and implementations may avoid a copy, but this is implementation-dependent. `copyExternalImageToTexture()` is explicitly a copy. Keep the effect graph GPU-resident, avoid readback, and benchmark the actual Chromium/GPU/driver combination. [WebGPU specification](https://gpuweb.github.io/gpuweb/) · [Chrome WebCodecs/WebGPU integration](https://developer.chrome.com/blog/new-in-webgpu-116)
- **Color/HDR is a separate project.** WebGPU itself stores raw numeric texture values and performs conversion at external inputs/outputs; Canvas transformation also risks unwanted conversion/precision loss. Phase 1 should explicitly be SDR Rec.709/sRGB, 8-bit 4:2:0 output, with color metadata round-trip tests. Do not advertise HDR, 10-bit, wide-gamut, alpha video or ProRes fidelity before dedicated validation. [WebGPU color spaces](https://gpuweb.github.io/gpuweb/)
- **Browser custom codecs are not free.** MediaBunny permits custom coders and has WASM extensions (for example MP3), but they introduce a CPU/WASM performance and compatibility branch. The MP3 extension uses a worker/WASM because major browsers do not natively encode MP3. A “free/open-source app” also does not make H.264/HEVC patent obligations disappear; current MDN guidance flags both as patented and recommends legal review. [MP3 encoder extension](https://mediabunny.dev/guide/extensions/mp3-encoder) · [codec licensing notes](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API/Codec_selection)

Mediabunny is MPL-2.0: use and distribution are permitted, including commercial use, but distributed modifications to Mediabunny source must remain under MPL-2.0. Electron is MIT; Tauri is MIT or Apache-2.0. Choose the editor's own license independently after a complete third-party notice/SBOM review. [MediaBunny license](https://github.com/Vanilagy/mediabunny) · [Electron license](https://github.com/electron/electron/blob/main/LICENSE) · [Tauri license](https://v2.tauri.app/concept/architecture/)

### Dated codec/distribution gate

**By 2026-08-14, before any public binary or format-support claim**, complete and record a legal/distribution review covering: intended distribution jurisdictions; the declared H.264/AAC (and any later HEVC/AV1) support matrix; patent and platform-framework implications; GPL/Qt and third-party notice obligations; an SBOM; and confirmation that no FFmpeg, NodeAV, or bundled codec binary is shipped. The reviewer must approve the published support matrix or block the binary. This is a release gate, not a claim that an open-source license resolves codec patent questions.

## Codec, container and export policy

For the native first working desktop release, constrain both ingest expectations and export promises. The browser harness may report only whether its particular Electron/Chromium/GPU/OS runtime supports the exact configuration; it cannot set the product support matrix.

| Use | Required first choice | Fallback / rule |
|---|---|---|
| Interchange export | MP4: H.264 Main/High + AAC | In the native product, select the declared platform adapter and record its actual path. In the browser comparator, probe exact strings using `isConfigSupported`/MediaBunny. H.264 + AAC is compatible in practice, but browser AAC encoder support is runtime-dependent. |
| Fully open controlled-playback export | WebM: VP9 + Opus | Prefer for the app's own preview/cache/export tests; WebM is the natural pairing. |
| High-efficiency opt-in | WebM: AV1 + Opus | Only expose after the exact machine proves hardware/software performance is acceptable. AV1 encoding is more computationally expensive and Safari support is limited. |
| Input | MP4/MOV H.264/HEVC, WebM VP9/AV1, WAV/MP3/AAC as capability-tested MVP | Unsupported or malformed tracks become explicit placeholders; never substitute a different decode without disclosure. |
| Avoid in phase 1 | universal ProRes, DNxHR, RAW, HDR, 10-bit/4:2:2, variable frame-rate perfection, Dolby/AC-3 export | These expand codec, color, licensing and hardware validation beyond a basic editor. |

H.264 is widespread but patented; HEVC has larger browser-encoding gaps outside Apple and is also patented. VP9/Opus in WebM is the sensible open controlled-playback target. Browser codec availability is a function of the runtime, operating system, GPU/driver, and exact configuration; query it at the selected settings, never infer it from a codec family. A browser probe result is neither evidence that a native platform adapter supports the profile nor a benchmark of native decode, GPU compositing, or export performance. [MDN codec/container guidance](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API/Codec_selection)

## Audio: preview and render are different jobs

Web Audio is appropriate for interactive monitoring: it has a separate rendering thread, scheduled times are relative to `AudioContext.currentTime`, and `AudioWorklet` provides custom low-latency work on that audio rendering thread. Use it to play clip slices, gain/pan/fades and a click-free scrub/preview buffer. [Web Audio 1.1](https://www.w3.org/TR/webaudio-1.1/) · [AudioWorklet](https://developer.mozilla.org/en-US/docs/Web/API/AudioWorklet)

It must not be the sole export engine. `OfflineAudioContext.startRendering()` resolves a whole rendered `AudioBuffer`, which makes it unsuitable as the default for an arbitrarily long project. Instead, run the same edit graph in fixed sample blocks (for example 1024–8192 frames): decode only the needed clips, apply gain envelopes/fades/effects, mix with headroom/limiting policy, pass `AudioData` to the encoder, and immediately release each block. Preserve sample positions as integer frames at a project rate (48 kHz in v1), converting to microseconds only at WebCodecs boundaries. [OfflineAudioContext specification](https://www.w3.org/TR/webaudio-1.1/)

AudioWorklet's real-time thread is not a general worker: it must never wait on disk I/O, allocate unpredictably, or receive large per-quantum postMessages. Use a bounded ring buffer only if the runtime is cross-origin isolated; `SharedArrayBuffer` requires a secure, cross-origin-isolated document. Otherwise use short buffered chunks and accept a slightly larger preview latency. [SharedArrayBuffer requirements](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer)

## Desktop runtime comparison

| Option | Strengths | Material weaknesses | Verdict |
|---|---|---|---|
| **Electron + MediaBunny / WebCodecs / WebGPU** | Bundles Chromium and Node, so Windows/macOS use a pinned browser runtime; useful for repeatable container/codec comparison against the fixed corpus. Electron's process model and sandboxing are documented and mature. | Larger package/RAM baseline; Chromium media capability still varies by GPU/OS; secure IPC and rapid Electron updates are mandatory. Do not enable Node in the renderer. | **Comparator harness only.** It has no editor, project, UI, persistence, preview, or export authority and cannot prove native performance. |
| **Tauri + browser MediaBunny** | Small Rust host; web UI; compiled native backend; bundles for macOS/Windows and capability-scoped file APIs. | Tauri uses WebView2 (updating Chromium) on Windows but system WKWebView on macOS. Thus codec, WebGPU and WebCodecs behavior tracks Windows runtime and macOS version differently. Its JS-to-Rust message bridge is not a raw-frame transport. | **Do not use for the MediaBunny comparator.** Potential future shell only after media/render moves fully native. |
| **Native Rust core + native Qt Quick shell** | One deterministic timeline, cache, audio mixer, decode/encode and render graph; direct Metal/VideoToolbox and D3D11/Media Foundation paths can be optimized; no WebView raw-frame IPC in the hot path. | Considerably more engineering and platform QA. “From scratch” means edit engine/render scheduler, not writing codecs/container parsers or drivers from scratch. The Qt/native surface bridge needs explicit thread/fence proof. | **Recommended production direction.** |
| **Browser-only/PWA** | Lowest installation friction; MediaBunny can use user-selected files, streams and OPFS; a useful demo/editor-lite. | File System Access picker is limited-availability and user-gesture gated; OPFS is quota-bound and storage can be evicted; no controlled runtime/driver version; browser tab lifecycle and memory policy make long 3K exports fragile. | **Not viable as the primary Windows/macOS desktop NLE.** Keep only as a demo or constrained companion. |

Electron renderers are sandboxed by default since Electron 20; filesystem/subprocess work belongs in the main process behind narrowly validated IPC, with context isolation and no remote code/Node integration. Native dialogs can yield user-selected paths, and a main/utility process should expose only `open`, range read, save-stream, cache and project APIs. [Electron process model](https://www.electronjs.org/docs/latest/tutorial/process-model) · [Electron sandboxing](https://www.electronjs.org/docs/latest/tutorial/sandbox) · [Electron security guidance](https://www.electronjs.org/docs/latest/tutorial/security)

Tauri's documented engine split is decisive here: Windows uses WebView2/Edge while macOS uses the OS's WKWebView, which is updated with the OS. Tauri's file plugin has scoped permissions and binary read/write APIs, and sidecars require explicit capability grants; these are useful for a native core but do not erase the engine split. [Tauri webview versions](https://v2.tauri.app/reference/webview-versions/) · [Tauri filesystem permissions](https://v2.tauri.app/plugin/file-system/) · [Tauri sidecars](https://v2.tauri.app/develop/sidecar/)

For a PWA, the File System Access API is limited-availability and requires a transient user activation. OPFS supports fast in-place worker access, but is origin-private, quota-governed, and subject to eviction unless persistence is granted; source media and export artifacts must therefore stay in the user filesystem, not be assumed durable in OPFS. [file picker](https://developer.mozilla.org/en-US/docs/Web/API/Window/showOpenFilePicker) · [OPFS](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system) · [storage quota and eviction](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria)

Both Electron and Tauri can package macOS and Windows applications, but code signing is a real release requirement. Electron recommends signing and macOS notarization; Tauri documents DMG/App Store and Windows installer paths, with signing/notarization for macOS. [Electron packaging/signing](https://www.electronjs.org/docs/latest/tutorial/tutorial-packaging) · [Tauri distribution](https://v2.tauri.app/distribute/)

## Fixed MediaBunny/WebCodecs comparison harness

Build at most a **two-week diagnostic harness**, not an editor clone. It uses Electron's pinned current Chromium, a local packaged test page, renderer sandbox, `contextIsolation`, a minimal preload API, and one MediaBunny worker pool. It has no project file, editor UI, persistence, timeline, compositor, preview surface, user file authority, or user export command. It receives a manifest and fixtures from the native test runner and emits only native-runner-owned telemetry plus disposable diagnostic artifacts. No `@mediabunny/server`, NodeAV, FFmpeg CLI/library, cloud render, or unbounded in-memory buffers.

### Fixed shared fixture and oracle scope

The native slice and harness use the same immutable, content-addressed corpus and expected-result manifest:

1. **QHD source and final fixtures:** 2560×1440, 30 fps CFR, AVC/H.264 8-bit 4:2:0 Rec.709 SDR; source audio is PCM or AAC-LC 48 kHz stereo; final is fast-start MP4 with AAC-LC.
2. **3K source and final fixtures:** 3072×1728, 30 fps CFR, AVC/H.264 8-bit 4:2:0 Rec.709 SDR; source audio is PCM or AAC-LC 48 kHz stereo; final is fast-start MP4 with AAC-LC.
3. **Control fixtures:** the declared VP9/Opus WebM control, a VFR-AV fixture expected to be explicitly rejected by v1 behavior, and malformed/unsupported samples expected to yield typed failures.
4. **Shared oracles:** fixture digest; container/track/sample-entry manifest; expected duration, PTS map, keyframe map, frame count, audio sample count, and audio-marker positions; plus independently decoded output metadata for final fixtures. The oracle checks structure and timing, not byte identity across encoders.

The harness records exact OS, Electron/Chromium version, GPU and driver, runtime capability-probe request/result, decoder/encoder choice if exposed, queue/reclamation errors, elapsed time, peak process memory, and each observed artifact's oracle result. It may show a test-run status page only; that page is not an editor UI. The native runner alone creates or persists projects, owns preview/export decisions, and decides test pass/fail.

### What the comparison can and cannot establish

It can establish that a pinned browser runtime, on a named machine, can or cannot demux/mux the fixed containers and create/decode the exact WebCodecs configurations with bounded resources. It can identify browser-specific failures, API regressions, and unintended CPU copies in that browser path.

It **cannot** prove the native engine's decode, encode, GPU-compositing, Qt surface-sharing, audio, preview, or export performance. A Chromium/WebCodecs result is runtime-dependent and is not transferable to VideoToolbox/Media Foundation or the native render graph. Native gates require native measurements on the same fixture/oracle corpus.

### Harness rejection criteria

Reject the harness as a useful comparator if it cannot consume the fixed manifest without gaining project/UI/persistence/export authority; if it needs NodeAV/FFmpeg or broad renderer filesystem/Node access; if it silently changes an unsupported fixture instead of reporting a typed result; or if telemetry cannot identify the exact runtime/configuration and oracle outcome. A harness pass leaves the native architecture unchanged; a harness failure is diagnostic evidence, not justification to revive a browser-first editor.

## Production architecture to carry forward

```text
Project JSON + command log
          |
          v
  Timeline compiler (pure, deterministic) ---> render plan + dependency windows
          |                                              |
          v                                              v
  Media index / keyframe database                  preview/export scheduler
          |                                              |
          v                                              v
 Native decode ---> GPU compositing/effects ---> native encode ---> mux/write
          ^                    |                                  |
          |                    v                                  v
  source file service      native preview surface             atomic export job
          ^
 web UI: commands, timeline, inspector, telemetry (never raw-frame IPC)
```

Use Rust for this core rather than Go: the engine needs explicit ownership/lifetimes for scarce CPU/GPU frames, FFI to platform media APIs, data-parallel render scheduling and a portable renderer. This is an architectural recommendation, not a request to write codecs; use platform codecs and standardized container libraries where licenses permit, while owning the edit semantics, cache policy, render plan and deterministic test harness. Keep MediaBunny only as the isolated browser comparison harness against the shared fixtures/oracles.
