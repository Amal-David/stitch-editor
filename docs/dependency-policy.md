# Dependency policy

The approved Rust graph is exact-pinned in `Cargo.lock` and inventoried in
`docs/dependencies.md`. Its direct runtime dependencies are `im 15.1.0` for the
immutable model, `sha2 0.10.9` for canonical SHA-256 identities,
`serde 1.0.228` and `serde_json 1.0.149` for versioned
fixture/oracle/benchmark evidence, and `rusqlite 0.40.1` with default features
disabled and only `backup,bundled` enabled for transactional persistence. The
selected `libsqlite3-sys 0.38.1` contains SQLite 3.53.2. Transitive runtime and
build-host packages are approved only for those declared capabilities and
target scopes.

Any addition, feature change, version change, source override, or target-scope
change requires a reviewed manifest and lockfile update, SPDX license record,
`Cargo.lock` source checksum, generated SBOM entry, security owner, and a
statement explaining why the selected operating-system API cannot supply the
capability. Cargo checksums authenticate source packages; release binaries
receive separate artifact digests.

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

Release builds must compile the bundled SQLite selected by rusqlite. Unreviewed
`LIBSQLITE3_SYS_USE_PKG_CONFIG`, `LIBSQLITE3_FLAGS`, `SQLITE3_LIB_DIR`,
`SQLITE3_INCLUDE_DIR`, source replacement, or Cargo patch/config overrides are
prohibited. `pkg-config` and `vcpkg` remain locked upstream build dependencies,
but they may not substitute a system SQLite. ProjectStore verifies SQLite
version `3.53.2`, the pinned source ID, critical compile options, WAL mode, and
`synchronous=FULL` before schema mutation.

The core/model security owner reviews `im`, `sha2`, and their transitive graph;
the fixture/oracle security owner reviews `serde`, `serde_json`, and their
transitive graph;
the persistence security owner reviews rusqlite, libsqlite3-sys, the embedded
SQLite source, and data-recovery behavior; the build/release security owner
reviews build-only crates, checksums, SBOM scope, and distributed notices. A
transitive dependency is not implicitly approved for a new direct use.

rusqlite and libsqlite3-sys require preservation of their MIT notice. SQLite is
public domain but remains a separately declared SBOM component. Published
`MPL-2.0+` components and every selected MIT/Apache-2.0 route remain subject to
the complete notice/source review. These records do not constitute legal
approval; `LEGAL_GATE.md` remains authoritative for release.

Run `./scripts/policy.sh` before review. This is an engineering gate, not legal
advice; `LEGAL_GATE.md` remains the release decision record.
