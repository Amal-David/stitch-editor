# Cross-review: media, web-runtime, codec, and slice falsifiability

**Reviewer:** Terra High media/web-stack council member  
**Scope:** independent review of `native-architecture.md`, `nle-performance.md`, ADR-001, and the vertical-slice contract, rechecked against the MediaBunny/WebCodecs report and current primary documentation.  
**Verdict: REVISE, not reopen.**

The selected native architecture remains the right production direction: Rust owns editorial semantics and scheduling; platform-native media/GPU code owns platform frames; Qt Quick is a native shell rather than a WebView media path. The decision becomes implementation-ready only after the corrections below. They are contract/evidence defects, not evidence that Electron, Tauri, Go, or a WebCodecs-only engine should replace it.

## Material objections, ranked

| Rank | Objection | Why it is material | Required disposition |
|---|---|---|---|
| P0 | The slice promises 3K export but lets the required `3K-final` profile return “unsupported.” | The outcome requires QHD **and 3K** deliverables, but the export profile accepts no 3K deliverable. A lossless oracle intermediate is useful evidence, not a user-deliverable substitute. This can let the selected architecture pass without meeting the stated product scope. | Make 3K H.264/AAC/MP4 export mandatory on every named baseline machine, or explicitly narrow the product/slice outcome and reopen ADR-001. |
| P0 | The performance report’s baseline fixtures contradict the ADR and slice format promise. | `nle-performance.md` requires 10-bit 4:2:2 all-intra QHD/3K files, while ADR-001 and the vertical slice deliberately limit v1 to 8-bit 4:2:0 AVC/common audio. A test suite cannot simultaneously treat a codec class as a later capability probe and a release baseline. | Adopt the vertical-slice 8-bit 4:2:0 AVC/PCM fixture matrix as the release baseline; retain 10-bit 4:2:2 only as a non-gating research probe. |
| P1 | “Native decoder surface to Qt” is asserted without the Qt render-thread/state contract. | Qt permits Metal/D3D11 integration, but native graphics interaction must occur on the scene-graph render thread; Qt also requires external-command bracketing for direct native API use. A generic separate GPU-submit thread risks races, state corruption, or an unmeasured copy. | Define the exact Qt integration mechanism and ownership/thread/fence model, then prove it per OS before shared preview/export work. |
| P1 | The first media-container promise is too broad for the evidence. | Media Foundation recognizes `.mov` through its MPEG-4 source, but its own documentation says MP4 extensibility means it cannot recognize every sample description. “MP4/MOV video plus common audio” is not a testable interoperability contract. | State exact accepted sample entries/codecs/audio and the chosen per-OS reader/writer path; unsupported QuickTime sample entries must be typed failures. |
| P1 | The candidate score is mathematically wrong and should not be used as decision evidence. | The candidate-A row sums to **92/100**, not 93: `(20×5 + 20×5 + 15×5 + 10×5 + 10×2 + 10×5 + 10×4 + 5×5) / 5 = 92`. The ordering stays A (92), C (87), B (85), D (58), but the ADR repeats the wrong value. | Correct 93 to 92 in the research and ADR; label the matrix as a qualitative hypothesis, not a measured comparison. |
| P2 | The Electron/MediaBunny recommendation is resolved but not explicitly superseded. | The MediaBunny report recommends Electron + MediaBunny as a bounded browser prototype; ADR-001 says a full Electron editor will not precede the native slice. That is a valid council decision, but the relationship must be explicit so the work graph does not recreate a competing editor. | Constrain Electron/MediaBunny to a small comparison harness with shared fixtures/oracles and no project/persistence/UI authority. |
| P2 | Licensing/patent language is directionally right but not yet a release gate. | GPL-3.0-or-later is compatible with the chosen open Qt route; MediaBunny/MPL, Electron/MIT, and Tauri MIT/Apache are correctly separated. Codec-patent exposure is nevertheless independent of source licenses, and “will not redistribute patented codecs casually” is not an operational policy. | Add a dated legal/distribution review gate: system-framework-only codecs, no bundled FFmpeg/NodeAV/codecs, shipped formats/jurisdictions, notices/SBOM, and a support matrix. |

## Exact proposed corrections

### 1. Make the 3K slice falsifiable

Replace the `3K-final` bullet at [`vertical-slice.md:45`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/vertical-slice.md:45) with:

> `3K-final`: 3072x1728 at 30 fps CFR, 8-bit 4:2:0 Rec.709 SDR, AVC/H.264 and AAC-LC 48 kHz stereo at 320 kb/s in fast-start MP4. This profile is **required** on every named baseline machine. The implementation records the encoder-selected profile/level and hardware/software path. If the profile cannot be produced or independently decoded on either OS baseline, the slice fails and ADR-001 is reopened. The lossless oracle intermediate is a supplemental diagnostic artifact, never a pass condition.

The Microsoft system stack documents MPEG-4 source/sink, H.264 encode and AAC encode; it also documents that a hardware path is not guaranteed by a codec name. The test must record the actual path, rather than assert hardware acceleration. [Media Foundation supported formats](https://learn.microsoft.com/en-us/windows/win32/medfound/supported-media-formats-in-media-foundation) · [Microsoft AAC encoder](https://learn.microsoft.com/en-us/windows/win32/medfound/aac-encoder) · [hardware MFTs](https://learn.microsoft.com/en-us/windows/win32/medfound/hardware-mfts)

### 2. One baseline corpus, with explicit non-gating probes

At [`nle-performance.md:23-27`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/research/nle-performance.md:23), replace `H.264/AV1-or-platform export` with `capability-probed H.264/AAC MP4 export for the fixed QHD and 3K baseline; optional AV1/HEVC exports are recorded capability probes only`.

At [`nle-performance.md:310-313`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/research/nle-performance.md:310), replace `QHD-I` and `3K-I` with the matching vertical-slice AVC/H.264 all-IDR, 8-bit 4:2:0, Rec.709 SDR, PCM-in-MOV rows. Move 10-bit 4:2:2 all-intra content to a clearly headed **post-slice, non-gating research probe** section. The current vertical slice has already made this distinction at [`vertical-slice.md:28-36`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/vertical-slice.md:28).

Also replace the proxy wording at [`vertical-slice.md:40`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/vertical-slice.md:40) with an operational GOP contract, for example:

> `1280-wide, CFR, square-pixel, AVC/H.264 8-bit 4:2:0 Rec.709 SDR, all-IDR (every frame is a random-access point), AAC-LC 48 kHz stereo in MP4/MOV; the manifest records the exact codec configuration, GOP structure, PTS map, parent identity, profile digest and proxy digest.`

“Independently decodable” is not a sufficient requirement for fast random seek; it does not state whether every frame can be decoded without prior GOP state.

### 3. Turn “zero-copy Qt preview” into an implementation contract

Add this paragraph to ADR-001’s system boundaries after the platform bridge diagram ([`architecture-decision.md:37-66`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/architecture-decision.md:37)):

> The Qt preview integration is a specific render-thread contract, not a generic GPU handoff. On macOS the bridge retains `CVPixelBuffer`/`CVMetalTexture` through the Qt scene-graph command-buffer completion; on Windows it retains the `ID3D11Texture2D` through the Qt/D3D11 completion boundary. Native scene-graph interaction occurs only on Qt’s render thread, using the selected documented integration path (`QSGRenderNode` or `beforeRendering`/`afterRendering` with `beginExternalCommands`/`endExternalCommands`). The first spike records device identity, native surface format, synchronization primitive, and any copy/readback; a device mismatch, steady-state CPU copy, or undocumented cross-thread use fails the gate.

This is required by Qt’s own scene-graph guidance, not just an optimization preference. [Qt Quick scene graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html) · [Qt D3D11 under QML](https://doc.qt.io/qt-6/qtquick-scenegraph-d3d11underqml-example.html)

### 4. Make container capability testable

Replace the generic first-format statement at [`architecture-decision.md:92`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/architecture-decision.md:92) with:

> Slice-required ingest is MOV/MP4 containing AVC/H.264 8-bit 4:2:0 Rec.709 video with PCM or AAC-LC 48 kHz stereo, plus the fixed VFR-AV fixture. Slice-required output is H.264/AAC-LC fast-start MP4. Each platform adapter must report container, sample entry, codec configuration, decoder/encoder and muxer identity before opening/exporting. Every other MOV/MP4 sample entry—including otherwise valid QuickTime content—is an explicit typed unsupported-media result until corpus and fuzz evidence admits it.

This correction is conservative but grounded: Microsoft lists `.mov` under its MPEG-4 source/sink, while separately warning that the extensible MP4 format prevents recognition of every sample description. AVFoundation’s writer does support MPEG-4 and QuickTime containers, but that does not prove cross-platform support for arbitrary MOV contents. [Microsoft MPEG-4 source](https://learn.microsoft.com/en-us/windows/win32/medfound/mpeg-4-file-source) · [AVAssetWriter](https://developer.apple.com/documentation/avfoundation/avassetwriter)

### 5. Repair the plan boundary and score

- Change candidate A from `93/100` to `92/100` in [`native-architecture.md:15,34`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/research/native-architecture.md:15) and [`architecture-decision.md:28`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/architecture-decision.md:28).
- Replace the one-video/one-audio “First implementation boundary” in [`native-architecture.md:133-135`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/research/native-architecture.md:133) with the canonical two-video/two-audio recipe defined in [`vertical-slice.md:50-54`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/vertical-slice.md:50). A dissolve and equal-power audio crossfade cannot falsify composite/mix ownership with only one video and one audio input.
- Add to ADR-001’s Electron paragraph ([`architecture-decision.md:33`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/architecture-decision.md:33)): `It supersedes the earlier Electron editor prototype recommendation: any Electron/MediaBunny work is fixture/oracle comparison only, owns no project state, and cannot delay the native slice.`

### 6. Tighten the legal gate

Replace the second sentence at [`architecture-decision.md:98`](/Users/amal/experiments/stitch-editor/.superplan/changes/open-video-editor/specs/architecture-decision.md:98) with:

> Before a public binary or format-support claim, complete a dated legal/distribution review covering GPL/Qt obligations, H.264/AAC patent considerations in intended jurisdictions, system-framework reliance, third-party notices/SBOM, and the explicit prohibition on bundling FFmpeg, NodeAV, or codec binaries. Publish the resulting supported-format/legal matrix with the binary.

Do not use `@mediabunny/server` to fill a native feature gap: it wraps NodeAV and FFmpeg C APIs. Browser-side MediaBunny is a TypeScript container/WebCodecs layer, not a native engine. [Mediabunny server extension](https://mediabunny.dev/guide/extensions/server) · [Mediabunny overview](https://mediabunny.dev/guide/introduction)

## Missing prototype evidence before accepting the architecture unchanged

1. **Qt/native-surface spike:** prove that VideoToolbox/CoreVideo/Metal and Media Foundation/D3D11 each use the same native device/context as Qt’s scene graph, with frame lifetime tied to the correct completion primitive. This must include device-loss/recreation and high-DPI/multi-monitor behavior.
2. **Required 3K export:** on one representative baseline per OS, produce the exact `3K-final` file, independently decode it, and publish profile/level, actual encoder path, fast-start evidence, PTS/duration/audio-marker checks, and failure behavior.
3. **Container corpus:** corpus/fuzz the declared MOV/MP4 sample entries, malformed atom/box sizes, missing indexes, long GOPs, VFR edit lists, and no-audio/video variants in a constrained helper. “MOV” cannot be the corpus category.
4. **Audio/preview ownership:** demonstrate no locks/allocations/waits in the audio callback while the Qt render thread and proxy/index work are saturated; trace the audio clock, video presentation, GPU fences, and cancellation epochs together.
5. **Browser comparator only:** if retained, run MediaBunny + WebCodecs on the exact AVC/AAC and VP9/Opus test assets to document what is browser-specific. It must not supply export or preview performance evidence for the selected native engine. WebCodecs lacks container I/O and codec support is runtime-dependent; MediaBunny supplies container I/O but not NLE scheduling/compositing. [WebCodecs overview](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API) · [MediaBunny supported formats/codecs](https://mediabunny.dev/guide/supported-formats-and-codecs)

## Claims adequately supported

- **Native over WebView hot path:** supported. Electron pins Chromium, while Tauri uses WebView2 on Windows and system WKWebView on macOS; browser codec/GPU support therefore remains a capability probe, not a reliable native-frame transport. [Electron architecture](https://www.electronjs.org/docs/latest) · [Tauri webview versions](https://v2.tauri.app/reference/webview-versions/)
- **MediaBunny boundary:** supported. It provides container mux/demux and WebCodecs-oriented processing; it does not own the editor’s project model, timeline, compositor, audio mix graph, or persistence. [MediaBunny introduction](https://mediabunny.dev/guide/introduction)
- **Rust core + C ABI and native platform bridge:** supported as an engineering trade-off, not a benchmark result. The Rust ABI is not a stable plugin/FFI ABI; opaque C ABI handles are the correct boundary. [Rust ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html)
- **D3D11 as Windows v1:** supported as a lower-risk interop choice. Media Foundation’s DXGI device manager is D3D11-oriented, while D3D12 requires explicit resource/synchronization management. [MF DXGI device manager](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mfcreatedxgidevicemanager) · [D3D12 resource barriers](https://learn.microsoft.com/en-us/windows/win32/direct3d12/using-resource-barriers-to-synchronize-resource-states-in-direct3d-12)
- **Qt route under GPL-3.0-or-later:** directionally supported, subject to the requested legal check. The current docs list Qt licensing routes; this review does not give legal advice. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Deterministic revisioned project/render-plan model and shared preview/export semantics:** adequately specified by `nle-performance.md` and correctly made acceptance criteria by the slice. No material browser/native contradiction found there.

## Final assessment

After the P0/P1 corrections, the vertical slice is a credible architecture falsifier: it will prove or disprove the actual risky boundaries—native decoder surface to Qt, deterministic shared graph, real-time audio, bounded cancellation, and a real 3K deliverable—without pretending that browser capability or a score matrix proves performance. Until then, the current slice can prove much of the editor model but cannot prove that the selected architecture meets the promised 3K export scope.
