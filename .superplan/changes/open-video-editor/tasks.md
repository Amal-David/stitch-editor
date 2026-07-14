# Task Graph

## Graph Metadata
- Change ID: `open-video-editor`
- Title: Open Video Editor
- Work type: investigative, decision-heavy, integration-heavy
- Autonomy: checkpointed autopilot
- Graph strategy: risk-first evidence graph; stop and reopen after failed architecture gates

## Graph Layout
- `T-0001` Evaluate MediaBunny, WebCodecs, and browser-hosted media pipelines
  - workstream: WS-RESEARCH
  - depends_on_all: []
- `T-0002` Compare Rust, Go, and native cross-platform media-engine architectures
  - workstream: WS-RESEARCH
  - depends_on_all: []
- `T-0003` Define the efficient NLE domain model and performance benchmark suite
  - workstream: WS-RESEARCH
  - depends_on_all: []
- `T-0004` Synthesize the architecture decision record and bounded from-scratch strategy
  - workstream: WS-ARCHITECTURE
  - depends_on_all: [T-0001, T-0002, T-0003]
- `T-0005` Specify the benchmarkable vertical slice and architecture acceptance gates
  - workstream: WS-PROTOTYPE
  - depends_on_all: [T-0004]
- `T-0006` Verify Terra High delegation and accept the council revisions
  - workstream: WS-EXECUTION-GATE
  - depends_on_all: [T-0005]
- `T-0015` Bootstrap the open-source repository and pinned cross-platform build contract
  - workstream: WS-BOOTSTRAP
  - depends_on_all: [T-0006]
- `T-0007` Implement the deterministic editorial core and crash-safe project store
  - workstream: WS-CORE
  - depends_on_all: [T-0015]
- `T-0008` Build the fixture, oracle, tracing, and benchmark harness
  - workstream: WS-CORE
  - depends_on_all: [T-0015]
- `T-0009` Prove the stable C ABI and Qt Quick shell boundary
  - workstream: WS-CORE
  - depends_on_all: [T-0015]
- `T-0010` Prove the macOS native decode-to-Metal preview path
  - workstream: WS-PLATFORM
  - depends_on_all: [T-0008, T-0009]
- `T-0011` Prove the Windows native decode-to-D3D11 preview path
  - workstream: WS-PLATFORM
  - depends_on_all: [T-0008, T-0009]
- `T-0012` Prove the bounded real-time audio mixer and device path
  - workstream: WS-PLATFORM
  - depends_on_all: [T-0007, T-0008]
- `T-0016` Prove demux, seek indexing, encode, mux, cancellation, and atomic export
  - workstream: WS-PLATFORM
  - depends_on_all: [T-0007, T-0008]
- `T-0013` Integrate the shared preview/export graph and slice effects
  - workstream: WS-INTEGRATION
  - depends_on_all: [T-0007, T-0008, T-0010, T-0011, T-0012, T-0016]
- `T-0014` Package, benchmark, and accept the cross-platform vertical slice
  - workstream: WS-INTEGRATION
  - depends_on_all: [T-0013, T-0017]
- `T-0017` Execute and accept hosted macOS and Windows bootstrap CI
  - workstream: WS-BOOTSTRAP
  - depends_on_all: [T-0015]

## Workstreams
- `WS-RESEARCH` Evidence gathering and option comparison
- `WS-ARCHITECTURE` Architecture selection and decision record
- `WS-PROTOTYPE` Measured vertical-slice definition
- `WS-EXECUTION-GATE` Verified GPT-5.6 Terra High delegation and evidence acceptance
- `WS-BOOTSTRAP` Repository, license, toolchain, build, and CI foundation
- `WS-CORE` Deterministic core, test evidence, and UI boundary foundations
- `WS-PLATFORM` Native macOS, Windows, and real-time audio falsification spikes
- `WS-INTEGRATION` Shared render semantics, packaging, and measured slice acceptance

## Cross-Workstream Dependencies
- Architecture selection waits for all three independent research tracks.
- Repository bootstrap waits for the Terra High execution gate and is the single owner of shared workspace/build layout.
- Core, harness, and Qt/C ABI foundations wait for bootstrap, then run independently.
- Native video spikes wait for the fixture/oracle harness and UI boundary; audio and media-I/O spikes wait for the deterministic core and harness. All four falsification spikes may run independently.
- Shared preview/export integration waits for both zero-copy video paths, the deterministic core, the real-time audio path, and demux/index/encode/mux proof.
- Packaging and public baseline evidence wait for the integrated vertical slice.
- Final packaging also waits for hosted macOS and Windows bootstrap artifacts; the repository foundation does not claim unexecuted Windows evidence.

## Notes
- The post-research executable frontier is intentionally only `T-0006`. Every council member is attested in `research/terra-high-attestation.md`; future workers must pass the same metadata check.
- `T-0015` is the sole next task because this directory has no Git repository or shared build skeleton and Qt is not installed.
- `T-0007`, `T-0008`, and `T-0009` become parallel only after bootstrap; `T-0010`, `T-0011`, `T-0012`, and `T-0016` attack the four irreversible native/runtime seams before integration.
- Re-shape instead of pushing through if the C ABI, Qt distribution, native zero-copy, audio real-time, correctness, or packaging gates fail.
- The graph ends at the falsifiable vertical slice; product breadth is not decomposed until measured evidence exists.
- `T-0017` is an external-runner evidence gate. It may execute as soon as a remote runner is authorized, but only blocks final packaging rather than platform-independent core work.
- “World's most efficient” is a benchmark target and may only be claimed against named workloads and hardware.
