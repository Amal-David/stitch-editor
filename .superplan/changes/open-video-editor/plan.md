# Open Video Editor Program Plan

## Objective

Deliver a free and open-source Windows and macOS video editor whose common 2K- and 3K-class workflows are measurably fast, memory-efficient, reliable, and easy to use.

## Operating Strategy

Use checkpointed autopilot. Research, prototypes, implementation, tests, and performance work may proceed autonomously. Stop for review when an architecture decision would create large downstream lock-in, when licensing or codec constraints change the product promise, or when benchmark evidence contradicts the selected design.

All research and implementation sub-agents must use GPT-5.6 Terra with high reasoning. Capture each returned sub-agent thread ID and verify its authoritative local thread row before accepting material output; interrupt and discard missing or mismatched work. Parent-thread settings and failed override attempts are not proof. The first council is attested in `research/terra-high-attestation.md`.

## Phase 1: Architecture Evidence

Run three independent research tracks in parallel:

1. Browser/media stack: MediaBunny, WebCodecs, Web Audio, workers, GPU access, and desktop wrappers.
2. Systems stack: Rust, Go, justified alternatives, native media APIs, GPU/audio layers, FFI, packaging, and portability.
3. Editor model: deterministic timeline, render graph, audio scheduling, caching/proxies, recovery, and benchmark methodology.

Synthesize the results into one architecture decision record. Select a stack only if its weakest subsystem has a credible fallback.

## Phase 2: Falsifiable Vertical Slice

Bootstrap one shared repository/build contract, then build the smallest end-to-end slice that can disprove the architecture quickly: deterministic core and durable store, independent fixture/oracle harness, Qt/C ABI frame-lease boundary, native YUV-to-RGBA preview on both platforms, real-time audio, demux/index/encode/mux/atomic export, integration, and packaging. Run the same benchmark contract on Windows and macOS.

If the slice misses hard latency, memory, correctness, or portability gates, fix the owning subsystem or revisit the architecture before adding product breadth.

## Phase 3: Usable MVP

Implement project/media management, multi-track timeline editing, trim/split/stitch/merge, music and audio placement, gain and fades, basic transitions/effects, responsive preview, autosave/recovery, undo/redo, export presets, and installers. Keep preview and export driven by the same timeline semantics.

## Phase 4: Performance and Reliability

Add proxies, render caching, background scheduling, hardware-acceleration tuning, bounded queues, memory-pressure behavior, long-project tests, corrupted-media handling, crash recovery, and regression benchmarks. Optimize from profiles and traces, not intuition.

## Phase 5: Open-Source Release

Publish an OSI-licensed repository, architecture and contributor documentation, format and hardware-support matrices, reproducible benchmark methodology, signed Windows and macOS packages where feasible, and a clear separation between verified capabilities and experimental ones.

## Verification Gates

- Correctness: deterministic timeline evaluation, frame/audio oracles, A/V sync bounds, project round-trips, and export validation.
- Performance: named 2K- and 3K-class fixtures with seek, scrub, playback, memory, proxy, and export metrics.
- Portability: real Windows and macOS runs with hardware and driver metadata recorded.
- Reliability: interrupted save/export, malformed media, missing assets, low disk space, device changes, and recovery tests.
- Product: the core edit-and-export workflow is operable without proprietary services, paid codecs supplied by the project, or command-line knowledge.

## Re-shape Triggers

- MediaBunny or WebCodecs cannot meet required codec, threading, memory, or desktop-runtime behavior.
- A Rust or Go candidate cannot access hardware media or GPU paths without an unstable or unmaintainable bridge.
- Codec licensing prevents a promised default format from being redistributed safely.
- Preview and export semantics diverge materially.
- The vertical slice fails its cross-platform performance or correctness gates after evidence-driven optimization.
