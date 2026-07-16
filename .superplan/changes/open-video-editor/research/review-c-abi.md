# T-0009 C ABI evidence

## Implemented boundary

- Rust owns the deterministic editor core behind nonzero opaque 64-bit IDs.
- The public C header contains only fixed-width POD values, 16-byte IDs,
  32-byte digests, rational time, opaque bridge tokens, and callable functions.
- ABI major/minor negotiation is explicit and rejects unsupported versions.
- Snapshot and diff handles have retain/release ownership; callbacks are
  synchronous, same-thread, after-unlock, and borrow the diff handle.
- Read-only callback reentrancy is allowed. Core mutation and destruction from
  a callback are rejected with `STITCH_REENTRANT_MUTATION`.
- A frame lease is registered and transitioned on one render/bridge owner
  thread and is tagged with revision, epoch, device generation, surface token,
  synchronization token/value, pixel format, and dimensions.
- Submit rejects stale work. Already-submitted canceled work can still complete
  and retire so the native resource can drain safely. Retirement invalidates
  the lease ID.
- Every exported Rust boundary catches unwinding and returns a typed result.

## Verified on macOS

Independent root verification used Rust 1.97.0, AppleClang 21, CMake 3.31.6,
and Qt 6.11.1:

- `cargo fmt --all -- --check`
- strict Clippy for all `stitch-c-abi` targets
- eight Rust ABI tests in debug and release
- 1,000 core create/destroy cycles
- 10,000 deterministic adversarial allocated command batches
- callback thread/reentrancy and immutable metadata access
- callback rejection for project, epoch, destruction, and frame-lease mutation
- reserved and command-kind-unused fields rejected instead of silently ignored
- lease malformed/stale/wrong-device/wrong-thread/lifecycle cases
- explicit owner-thread discard of registered/acquired work that never reached
  the GPU; submitted work cannot use this shortcut
- linked C++ consumer test against the real Rust dynamic library
- exact 64-bit size, alignment, and critical field-offset assertions in Rust
  and C++
- AppleClang AddressSanitizer and UndefinedBehaviorSanitizer on the linked C++
  consumer; 1/1 CTest passed

Root CMake now enables CTest before adding subdirectories. Before that fix the
consumer executable built, but canonical root `ctest` discovered zero tests.

## Open evidence

- Rust itself was not sanitizer-instrumented; stable pinned Rust does not make
  the nightly sanitizer path part of this offline bootstrap.
- The adversarial corpus is deterministic in-process fuzz-style coverage, not
  a retained libFuzzer corpus.
- Windows size/layout, DLL loading, callback, wrong-thread, sanitizer, and
  lifecycle runs remain unverified on a real MSVC host.
- The macOS Qt/Metal shell now proves actual GPU completion and owner-thread
  retirement. T-0009 remains cross-platform-incomplete until Windows compiles
  and runs the ABI suite and the D3D11 event-query falsification test.
