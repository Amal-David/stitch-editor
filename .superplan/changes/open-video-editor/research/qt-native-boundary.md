# Qt 6.11.1 native preview boundary

## Decision

T-0009 may prove a synthetic, bridge-owned, same-device RGBA texture without
claiming real decoder integration. The Qt scene graph receives an opaque
surface token and synchronization metadata; QML receives control/status
metadata only. Native pointers and media bytes remain inside the platform
bridge.

The graphics API and native device must be selected before scene-graph
initialization. `QQuickGraphicsDevice` is non-owning, so the bridge retains the
Metal device/queue or D3D11 device/immediate context for the window lifetime.
At scene-graph initialization and reinitialization, the selected backend and
the exact adopted device resource must match or the gate fails loudly.

## macOS completion proof

On the render thread, wrap a bridge-owned `id<MTLTexture>` with
`QNativeInterface::QSGMetalTexture::fromNative`. Neither the Qt wrapper nor a
frame signal proves GPU completion, and the wrapper does not own the native
texture.

After Qt records the scene-graph draw, obtain the current Metal command buffer
through `QSGRendererInterface::CommandListResource` and attach a native
completion handler. The handler only queues the completed lease/use token. A
later render-thread drain detaches the node generation, completes and retires
the ABI lease, deletes the Qt wrapper, and releases the native texture.
`afterRendering` and `afterFrameEnd` are CPU/submission milestones, not GPU
completion.

Primary references:

- <https://doc.qt.io/qt-6.11/qquickgraphicsdevice.html>
- <https://doc.qt.io/qt-6.11/qsgrendererinterface.html>
- <https://doc.qt.io/qt-6.11/qnativeinterface-qsgmetaltexture.html>
- <https://doc.qt.io/qt-6/qtquick-scenegraph-metaltextureimport-example.html>
- <https://doc.qt.io/qt-6.11/qquickwindow.html>

## Windows completion proof

A raw `ID3D11DeviceContext::End(D3D11_QUERY_EVENT)` call in
`afterRendering` is not ordered after Qt's texture sample: pinned Qt 6.11.1
buffers D3D11 QRhi commands until later replay. The source-pinned sequence is:

1. From the render-thread `afterRendering` callback, call
   `beginExternalCommands()`. In Qt 6.11.1 this replays pending QRhi commands
   on the adopted immediate context.
2. Re-query `DeviceContextResource`, assert pointer identity and an immediate
   context, then call `End` on a per-use event query.
3. Call `endExternalCommands()`.
4. On later render-thread work, poll `GetData` with
   `D3D11_ASYNC_GETDATA_DONOTFLUSH`. Only `S_OK` with `TRUE` proves completion.
5. Retire only after query completion and node-generation detachment.

`DONOTFLUSH` proves completion but not forward progress. Normal presentation
submits the frame; bounded final teardown may issue one asynchronous `Flush`
and still waits for the event query. Deferred contexts are rejected. Device
loss aborts completion claims and recreates the entire device/scene-graph
generation.

This ordering relies on pinned Qt 6.11.1 implementation, not a stable public
Qt promise, so every Qt patch requires source re-audit. Windows remains
unaccepted until a real hardware run captures the positive sequence, a
negative control without `beginExternalCommands`, resize/minimize stress, and
forced device loss.

Primary references:

- <https://code.qt.io/cgit/qt/qtdeclarative.git/tree/src/quick/items/qquickwindow.cpp?h=v6.11.1#n694>
- <https://code.qt.io/cgit/qt/qtbase.git/tree/src/gui/rhi/qrhid3d11.cpp?h=v6.11.1#n1418>
- <https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_query>
- <https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-getdata>
- <https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_async_getdata_flag>
- <https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-flush>

## Acceptance boundary

macOS requires a real Metal completion trace through resize and teardown.
Windows requires the hardware event-query experiment above. This task does not
claim decoder output, YUV conversion, color correctness, hardware encode, or
2K/3K throughput; those belong to the later platform and benchmark tasks.

## macOS implementation evidence (2026-07-15)

The current Qt 6.11.1 shell now proves the synthetic macOS half of this gate:

- The bridge creates and retains the Metal device and command queue before QML
  loading or scene-graph initialization, adopts both through
  `QQuickGraphicsDevice`, and checks the exact `DeviceResource` and
  `CommandQueueResource` identities on the render thread.
- A bridge-owned 2x2 RGBA8 Metal texture is wrapped only on the scene-graph
  render thread. The wrapper identity is checked, the visual node is detached
  without transferring wrapper ownership, and native resources remain retained
  until a real Metal command-buffer completion handler publishes the lease.
- The completion callback performs only an allocation-free atomic lease-ID
  handoff. A later render-thread pass completes and retires the C-ABI lease.
- The self-test opens a snapshot, submits a typed edit, validates its
  synchronous revision-tagged diff, cancels the submitted preview epoch,
  resizes after submission, checks DPR and accessibility metadata, and requires
  direct scene-graph teardown before reporting success.
- Automated self-tests retain the real window/swapchain path but are fully
  transparent, non-activating, and transparent to input. Visibility requires
  the explicit `--self-test-visible` diagnostic flag.

Verified locally with the canonical offline bootstrap, the linked C++ CTest,
AppleClang AddressSanitizer/UndefinedBehaviorSanitizer, and hidden Qt Metal
self-tests. The trace contains distinct GUI/render threads in the threaded
render loop, adopted device/queue identity, DPR 2, lease acquire/submit,
command-buffer completion attach/dequeue, epoch cancellation, resize,
detach, complete/retire, and direct teardown.

Commands used for the current evidence:

```sh
rtk env Qt6_DIR=/opt/homebrew/opt/qtbase/lib/cmake/Qt6 CMAKE_PREFIX_PATH=/opt/homebrew/opt/qtbase:/opt/homebrew/opt/qtdeclarative RUSTUP_TOOLCHAIN=1.97.0 ./scripts/bootstrap.sh all
rtk env ASAN_OPTIONS=abort_on_error=1:symbolize=1 UBSAN_OPTIONS=halt_on_error=1 ctest --test-dir build/asan-qt611 --output-on-failure
rtk env ASAN_OPTIONS=abort_on_error=1:symbolize=1 UBSAN_OPTIONS=halt_on_error=1 ./build/asan-qt611/desktop/qt/stitch_editor_shell --self-test
```

This does not accept the cross-platform task. The Windows source remains a
guarded D3D11 bootstrap and has not yet implemented or run the synthetic
texture import, source-pinned event-query completion sequence, wrong-device and
WARP rejection, resize/minimize stress, or device-loss negative controls on a
real MSVC/Qt/D3D11 host.
