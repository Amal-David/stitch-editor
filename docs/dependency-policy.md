# Dependency policy

This bootstrap has no production dependencies beyond the pinned Rust/CMake/Qt
toolchains. Later additions require a reviewed lockfile/config update, SPDX
license record, source hash, SBOM entry, security-owner record, and a statement
of why the capability cannot be supplied by the selected operating-system API.

The following are prohibited in build manifests and release bundles unless
ADR-001 is formally reopened: FFmpeg, NodeAV, libavcodec, libavformat, bundled
codec binaries, build-time network downloads, and unreviewed copyleft or
license-conflicting dependencies.

System frameworks are wrapped rather than redistributed: VideoToolbox,
CoreVideo, Metal, CoreAudio, Media Foundation, D3D11, and WASAPI. Qt is
distributed only under the chosen reviewed GPL/LGPL/commercial route. The
policy check scans build manifests and rejects forbidden media dependencies and
CMake download primitives; it deliberately allows this documentation to name
prohibited software.

Run `./scripts/policy.sh` before review. This is an engineering gate, not legal
advice; `LEGAL_GATE.md` remains the release decision record.
