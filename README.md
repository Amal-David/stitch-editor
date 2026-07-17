# Stitch Editor

[![bootstrap](https://github.com/Amal-David/stitch-editor/actions/workflows/bootstrap.yml/badge.svg)](https://github.com/Amal-David/stitch-editor/actions/workflows/bootstrap.yml)

Stitch Editor is an experimental, free, open-source desktop video editor for
macOS and Windows. The long-term product target is a fast native editor for
2K/3K timelines, stitching, music and audio editing, fades, transitions, and
basic effects without bundling FFmpeg or a browser media engine.

> **Development status:** early architecture and vertical-slice work. This is
> not yet a usable replacement for CapCut or Filmora, and there are no release
> binaries. The repository currently proves the deterministic editor core,
> durable project storage, native ABI, and a synthetic macOS Metal preview
> boundary. Deterministic fixture recipes, evidence schemas, oracle checks, and
> a reproducible benchmark scheduler now exist, but native encoded-fixture
> generation and independent media decoding are not implemented yet. Real
> media import, audio playback, effects, export, and the Windows D3D11 preview
> path are still being built.

## Architecture

- **Rust** owns deterministic project and timeline state, exact rational time,
  commands, undo/redo semantics, render-plan contracts, and SQLite persistence.
- A small, versioned **C ABI** separates the safe core from platform media and
  GPU objects.
- **Qt Quick** provides the cross-platform desktop shell.
- **macOS** will use AVFoundation, VideoToolbox, CoreVideo, Metal, and
  CoreAudio.
- **Windows** will use Media Foundation, D3D11, and WASAPI.
- **MediaBunny** is a comparison/reference harness only; it is not the
  production timeline or media engine.

The design deliberately reuses operating-system codecs and GPU APIs while
writing the editor-specific scheduling, project, timeline, and correctness
layers from scratch. See the
[architecture decision](.superplan/changes/open-video-editor/specs/architecture-decision.md)
and [vertical-slice contract](.superplan/changes/open-video-editor/specs/vertical-slice.md)
for the exact boundaries and acceptance gates.

## What works today

- deterministic immutable project revisions and typed command batches;
- exact-time and cross-boundary contracts;
- a single-writer bundled-SQLite project store with durable idempotent request
  receipts;
- a stable C ABI with explicit ownership, thread, epoch, and frame-lease rules;
- a Qt Quick shell and synthetic same-device Metal preview self-test on macOS;
- streaming QHD/3K/VFR fixture recipes with exact timestamps, duration-spanning
  visual/audio markers, source hashes, and WAV-oracle generation;
- isolated oracle evidence checks and a source-verifiable oracle lock;
- deterministic cold/warm benchmark scheduling, complete disclosure schemas,
  Perfetto-compatible trace JSON, failure retention, and paired regression
  analysis;
- offline policy, format, lint, unit-test, CMake, Qt, and CTest bootstrap checks;
- pinned macOS and Windows GitHub Actions bootstrap environments.

The fixture/oracle layer is still a foundation rather than end-to-end media
evidence: generated AVC/AAC files, native capability records, independent
BMFF/decode verification, and real benchmark-run collection remain active work.

## Build the foundation

The canonical bootstrap is intentionally offline. Provision the pinned tools
and fetch the locked Cargo graph first; the bootstrap itself must not download
or install dependencies.

Required versions:

- Rust 1.97.0 with `rustfmt` and `clippy`
- CMake 3.31.6
- Qt 6.11.1 with Qt Quick and Qt Quick Controls
- macOS: Xcode 15+ and macOS SDK 14+
- Windows: MSVC 2022 and Windows SDK 10.0.26100.0

```bash
cargo fetch --locked
Qt6_DIR=/absolute/path/to/Qt6/lib/cmake/Qt6 \
  ./scripts/bootstrap.sh all
```

Some Qt installations split Quick/Quick Controls into another prefix. In that
case also set `CMAKE_PREFIX_PATH`. The full setup, dependency, and evidence
contracts live in [docs/toolchains.md](docs/toolchains.md),
[docs/dependency-policy.md](docs/dependency-policy.md), and
[docs/bootstrap-verification.md](docs/bootstrap-verification.md).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the
directory ownership rules in [docs/directory-ownership.md](docs/directory-ownership.md)
before changing shared contracts. Performance claims require public fixtures,
correctness checks, raw evidence, and disclosed hardware; the project does not
claim to be the fastest editor before those measurements exist.

Stitch Editor is licensed under [GPL-3.0-or-later](LICENSE). Binary distribution
and codec support remain subject to the legal gate in [LEGAL_GATE.md](LEGAL_GATE.md).
