# Product Goal

Build and ship a free and open-source non-linear desktop video editor for Windows and macOS that makes common 2K- and 3K-class editing work fast, reliable, and approachable.

The editor must support media import, a multi-track non-destructive timeline, trim/split/stitch/merge, adding and mixing music or drum tracks, audio and video fades, basic transitions and effects, responsive preview, crash-safe project persistence, and export. Its first user experience should be substantially simpler than a professional post-production suite while remaining architecturally capable of growing beyond the MVP.

## Architecture Goal

Choose the stack only after evidence. Compare Rust, Go, native platform media APIs, GPU APIs, web/native desktop shells, MediaBunny, WebCodecs, and justified from-scratch components. Do not treat a container library, codec layer, render engine, UI toolkit, or desktop shell as interchangeable parts of one problem.

The selected architecture must isolate:

- deterministic project and timeline evaluation
- media demux, decode, encode, and mux boundaries
- real-time video compositing and effects
- audio scheduling, mixing, and effects
- preview scheduling, proxies, caching, and background work
- platform hardware acceleration and software fallbacks
- UI state from render and export state
- crash recovery, autosave, and project format evolution

“From scratch” is an option where it creates a durable advantage, not a requirement to reimplement codecs, container standards, drivers, or mature primitives without evidence.

## Efficiency Goal

Efficiency must be demonstrated with reproducible workloads rather than adjectives. The benchmark contract must cover at least:

- import/index time and peak memory
- cold and warm seek latency
- scrub latency and dropped-frame behavior
- sustained preview frame rate under representative edits and effects
- proxy generation and cache behavior
- audio/video synchronization drift
- export throughput, output correctness, and peak memory
- project open/save/recovery behavior
- installer size and idle resource use where architecture choices materially affect them

Tests must include at least one 2K-class and one 3K-class fixture project on representative Windows and macOS hardware. Any comparative performance claim must name the workload, source codecs, output settings, hardware, and competing version.

## Release Goal

Deliver in evidence-gated phases: architecture research, vertical-slice prototype, usable MVP, performance hardening, cross-platform packaging, and open-source release. Publish the source, reproducible builds where practical, contributor documentation, benchmark methodology, supported-format matrix, and an OSI-approved license. No proprietary service may be required for core editing or export.

## Initial Acceptance Boundary

The first architecture gate is complete only when:

- primary-source research documents current capability and licensing constraints
- at least two viable architecture candidates are compared under one benchmark contract
- the chosen design explains what is reused, wrapped, or written from scratch
- risks and fallbacks are explicit for codecs, hardware acceleration, GPU portability, audio, packaging, and browser-hosted execution
- a benchmarkable vertical slice can ingest media, seek, apply one transition/effect and one audio fade, preview deterministically, and export a verifiably correct result
