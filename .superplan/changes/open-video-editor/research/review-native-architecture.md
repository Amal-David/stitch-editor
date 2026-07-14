# Terra High cross-review: native architecture

**Verdict: REVISE.** ADR-001's hybrid choice remains the right vertical-slice
direction: Rust should own deterministic edit semantics and scheduling, with
small platform-native bridges for decode, graphics, and UI. The current
decision is not yet acceptable unchanged, however. Two P0 boundary errors can
make a supposedly zero-copy preview either invalid or subtly CPU-backed, and a
missing media-I/O/export proof leaves a release-critical seam until integration.
This is a revision, not a reopen: it does not change the selected architecture
or reintroduce a Go media engine.

## What remains sound

- MediaBunny is appropriate as a Web/API behavior comparator and bounded
  browser experiment, not as this product's native media runtime. Its server
  package uses NodeAV/FFmpeg, which is outside the stated no-FFmpeg production
  constraint. [MediaBunny server documentation](https://mediabunny.dev/guide/server-side-usage)
- Rust's role should be the revisioned project model, render-plan compiler,
  bounded work scheduler, cache keys, effect parameters, and deterministic
  offline audio/render semantics. It must expose a C ABI, never Rust object
  layout or the Rust ABI, because Rust does not promise a stable native Rust
  ABI. [Rust Reference: external blocks](https://doc.rust-lang.org/reference/items/external-blocks.html)
- Go should not be the realtime engine or native-frame owner. This is an
  architecture/FFI choice, not a claim that Go cannot process media: cgo has
  pointer-lifetime rules and pins Go memory passed to C, which makes its
  runtime ownership model a poor fit for long-lived native frame and callback
  ownership. [cgo pointer rules](https://pkg.go.dev/cmd/cgo)
- The platform split is still correct: VideoToolbox/CoreVideo/Metal on macOS;
  Media Foundation/D3D11 on Windows; Qt Quick only as the presentation shell.
  D3D12 is a later experiment, not a vertical-slice dependency; Microsoft's
  D3D11-on-12 guidance calls out CPU and memory overhead. [D3D11-on-12 guidance](https://learn.microsoft.com/en-us/windows/win32/direct3d12/direct3d-11-on-12)

## Material objections

| Rank | Objection | Why it blocks acceptance | Exact correction |
| --- | --- | --- | --- |
| P0 | The documents imply decoder textures can be handed directly to Qt. | Qt's supplied Metal and D3D11 native texture wrappers are for **2D RGBA** textures. Hardware decode commonly produces NV12/P010, so wrapping decoder output directly is neither the defined Qt path nor an adequate color contract. | Amend ADR-001, the vertical slice, and T-0010/T-0011 acceptance to require a GPU-resident YUV-to-linear-working-RGBA conversion pass (or an explicitly prototyped custom multi-plane Qt material) before Qt receives a texture. The native path is `decoder YUV planes -> native GPU color pass -> 2D RGBA texture -> Qt`; it has one intentional GPU pass and zero steady-state GPU-to-CPU transfers. Validate Rec.709 matrix, limited/full range, transfer function, and source order. |
| P0 | “C ABI passes native surface handles + sync metadata” has no ownership or thread-domain contract. | Qt native-resource access is render-thread scoped and its wrappers do not take ownership. Passing raw COM, Objective-C, Metal, or Qt pointers as generic C POD lets the wrong thread release or reuse a surface before GPU completion. | Replace that phrase with an opaque, bridge-owned `frame_lease` protocol. Rust may request/present/release lease IDs and read immutable metadata; only the C++/Obj-C++ bridge retains/releases platform objects, imports an RGBA texture, and retires it after the relevant GPU completion. Include creator domain, device generation, revision/epoch, and explicit completion notification. No Rust panic or C++ exception may cross the ABI. |
| P1 | Qt native integration is treated as stable and backend-independent. | Qt documents no source or binary compatibility guarantee for native interfaces. Its graphics device must be configured before scenegraph initialization; a fallback software/WARP path would invalidate the performance conclusion. | Freeze an exact Qt build/version and shader toolchain for the slice; record it in evidence. Require backend assertions after initialization (`Metal` on macOS, `D3D11` on Windows), fail zero-copy gates on WARP/software/wrong backend, and rerun T-0009/T-0010/T-0011 whenever the pinned Qt native-interface version changes. Create/share the media device with Qt before scenegraph initialization. |
| P1 | The dependency graph reaches shared preview/export integration without a dedicated demux/index/encode/mux proof. | T-0010/T-0011 prove preview decode surfaces, while T-0013 is the first clearly stated real export integration. PTS/DTS ordering, VFR, seek index policy, encoder capability, container finalization, cancellation, and atomic output are all release-critical. | Add a **Media I/O and export spike** after T-0008 and before T-0013. It must ingest QHD-I, QHD-LGOP, and VFR-AV; prove seek/index behavior; select a disclosed encoder or typed unsupported result; mux interleaved audio/video with correct DTS/PTS/timebases; verify output independently; and prove cancellation leaves no final artifact (atomic rename only). Make T-0013 depend on it. |
| P2 | The audio task says “Rust mix graph” but not how the device callback crosses the native boundary. | A correct offline mixer can still underrun if a callback invokes a broad FFI layer, allocates, locks, logs, blocks, or permits panic unwinding. | T-0012 must choose and document one callback shape: a Rust-owned CPAL callback, or a minimal C trampoline into a preallocated Rust mixer context. Add a negative instrumentation test for allocation, mutex, file/network I/O, logging, and unwinding in the callback; include hot-unplug/teardown and device-clock discontinuity recovery. |
| P2 | “Supported 3K profiles return typed results” conflicts with a vertical-slice promise of 3K export unless the certified machine capability is fixed. | A typed unsupported result is valid for an optional platform codec, but not a substitute for the released 3K deliverable. | State that each disclosed acceptance machine must export at least the baseline 8-bit AVC/AAC 3K profile, whether by a permitted software path or a verified hardware path. Keep HEVC/AV1 optional and capability-gated. If no allowed baseline encoder exists, reopen the media scope rather than calling the slice complete. |

The first two objections are direct constraints from Qt's API: both
`QSGMetalTexture::fromNative` and `QSGD3D11Texture::fromNative` say the
resource is not owned by Qt, must be used on the scenegraph render thread, and
is suitable only for 2D RGBA textures. [Metal wrapper](https://doc.qt.io/qt-6/qnativeinterface-qsgmetaltexture.html) · [D3D11 wrapper](https://doc.qt.io/qt-6/qnativeinterface-qsgd3d11texture.html)

## Proposed boundary contract

The corrected native data path should be explicit in the ADR and both platform
tasks:

```text
macOS:   VT decode -> CVPixelBuffer (NV12/P010)
         -> CVMetalTexture plane views -> Metal color/effect pass
         -> owned RGBA MTLTexture -> Qt scenegraph import -> present

Windows: MF MFT decode -> ID3D11Texture2D (NV12/P010)
         -> D3D11 color/effect pass -> owned RGBA texture
         -> Qt scenegraph import -> present
```

`CVMetalTextureCacheCreateTextureFromImage` is the appropriate Core Video to
Metal bridge; Apple documents a live relationship to the source image, which
is exactly why the lease must retain the backing pixel buffer through GPU
completion. [Apple Core Video/Metal texture cache](https://developer.apple.com/documentation/corevideo/cvmetaltexturecachecreatetexturefromimage(_:_:_:_:_:_:_:_:_:))

On Windows, configure Media Foundation transforms with the same D3D11 device
through an `IMFDXGIDeviceManager`; Microsoft documents that the manager
associates an MFT with a D3D11 video-support device. Do not call
`IMFMediaBuffer::Lock` on the steady preview path: Microsoft notes that it can
require making a contiguous copy. [MFCreateDXGIDeviceManager](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mfcreatedxgidevicemanager) · [uncompressed video buffers](https://learn.microsoft.com/en-us/windows/win32/medfound/uncompressed-video-buffers)

The ABI is deliberately a control plane, not a graphics interchange format:

```c
typedef struct ove_frame_lease ove_frame_lease; /* opaque, bridge-owned */

ove_result ove_frame_acquire(ove_request, ove_frame_lease **out);
ove_frame_metadata ove_frame_metadata_get(const ove_frame_lease *);
ove_result ove_frame_submit_for_revision(ove_frame_lease *, ove_revision);
void ove_frame_release(ove_frame_lease *); /* schedules owner-thread retirement */
```

The concrete names may differ, but these invariants may not: ownership is
unambiguous; object destruction is on its owner thread after a completion
fence/event; stale device generations fail typed; consumers cannot cast the
lease to a native pointer; and all ABI structs are versioned C POD with
explicit size/alignment. Qt's renderer interface further restricts native
resources to the scenegraph render context and says returned resources are not
owned by the caller. [QSGRendererInterface](https://doc.qt.io/qt-6/qsgrendererinterface.html)

## Required prototype evidence before integration

1. **T-0009 strengthened shell/ABI spike.** Before placeholder success is
   accepted, use one externally created 2D RGBA texture from the same native
   graphics device as Qt. Prove scenegraph-init ordering, render-thread
   import, release-after-GPU-completion, resize/teardown, and stale-generation
   rejection. Run an ABI fuzzer plus sanitizers. Qt's `setGraphicsDevice()`
   must be called before scenegraph initialization. [QQuickWindow graphics device](https://doc.qt.io/qt-6/qquickwindow.html)
2. **T-0010/T-0011 decode-to-color spikes.** Decode actual 8-bit AVC fixtures
   to native YUV, execute the specified GPU color pass, and compare sampled
   output to a CPU oracle for black/white/range, Rec.709 primaries, matrix, and
   transfer. Capture a trace proving no steady-state CPU frame readback and a
   device-loss/recreate trace on Windows. Do not count a decoder-only or
   placeholder-only result as this evidence.
3. **New Media I/O/export spike.** Demonstrate long-GOP and VFR seeking,
   packet timestamp monotonicity where container rules require it, exact
   duration/sample counts, independent output parse/decode, encoder fallback
   disclosure, cancellation cleanup, and atomic publication. It must run on
   both acceptance OSes before T-0013 claims a unified preview/export graph.
4. **Audio callback spike.** Produce ten minutes of continuous callback
   telemetry showing zero underruns and the forbidden-operation instrumentation
   above, then simulate device loss/reopen while preserving bounded A/V drift.
5. **Packaging spike.** In T-0014, pin and inventory Qt/native media runtime
   artifacts in each signed package, include notices/SBOM, and run the native
   backend assertion on a clean machine. Qt's open-source obligations must be
   evaluated for the exact Qt edition and distribution method, not inferred
   from the application's GPL choice. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)

## Task-graph assessment

The graph already attacks three high-risk seams early: native macOS decode
(T-0010), native Windows decode/device-loss handling (T-0011), and realtime
audio (T-0012). Keep that ordering. Its weakness is that T-0009 currently
permits a placeholder to pass without proving same-device Qt ownership, and
export/container ownership is deferred to T-0013. Strengthen T-0009 as above,
add the Media I/O/export spike in parallel with T-0010/T-0011 where practical,
and make T-0013 depend on it. This creates early failure points for all four
irreversible seams: UI/device interop, decode/color conversion, audio callback,
and source-to-container media I/O.

Finally, explicitly pin the Qt native-interface version rather than treating
its APIs as a durable stable ABI: Qt warns that native interfaces have no
source or binary compatibility guarantees. [Qt native interfaces](https://doc.qt.io/qt-6/qnativeinterface.html)
