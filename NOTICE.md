# Notices

This project is GPL-3.0-or-later; see `docs/licensing.md` for the project intent
and `LICENSE` for the canonical GPL version 3 text.

The project has exact Cargo dependencies. `docs/dependencies.md` is the
human-reviewed inventory; `Cargo.lock` is the source-integrity input. The direct
runtime dependencies are `im 15.1.0`, `sha2 0.10.9`, and `rusqlite 0.40.1`.
`rusqlite` selects `libsqlite3-sys 0.38.1` with default features disabled and
only the `backup` and `bundled` features enabled. The resulting binary contains
SQLite 3.53.2, source ID
`2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24`.

rusqlite and libsqlite3-sys are MIT licensed. A distribution must preserve the
rusqlite developers' copyright and MIT permission notice. The remaining Cargo
graph contains MIT, Apache-2.0, dual MIT/Apache-2.0, and published `MPL-2.0+`
components; the complete package-by-package expressions and checksums are in
`docs/dependencies.md`. SQLite itself is public-domain software and says no
license is required, but it remains an explicit SBOM component because its
source is embedded inside libsqlite3-sys rather than represented separately in
Cargo.lock.

This file is an engineering notice record, not completed legal approval. A
public release must generate an SPDX or CycloneDX SBOM, include the complete
selected third-party license/notice texts, satisfy any MPL source obligations,
and pass the dated review in `LEGAL_GATE.md`. No generated media, FFmpeg,
NodeAV, libavcodec, libavformat, or bundled codec binary is present or approved.

System-provided frameworks (VideoToolbox, CoreVideo, Metal, CoreAudio, Media
Foundation, D3D11, and WASAPI) are not redistributed by this repository. Qt's
notice and distribution obligations are determined by the selected reviewed
Qt license route and recorded in `LEGAL_GATE.md` before a public binary ships.
