# Locked dependency inventory

This is the reviewed engineering inventory for the exact Rust dependency graph
in [`Cargo.lock`](../Cargo.lock). It is an input to SPDX/CycloneDX generation,
not a substitute for the generated SBOM, complete third-party license texts, or
the dated legal/distribution review in `LEGAL_GATE.md`.

The `Cargo.lock` checksums below authenticate the crates.io source packages used
by Cargo. They are source-integrity and SBOM inputs, not hashes of release
binaries. Release artifacts must also record their own digests and the target
triple used to build them.

## Direct capability decisions

| Capability group | Security owner | Direct dependency | Why an operating-system API is not the replacement |
| --- | --- | --- | --- |
| Immutable model | Core/model owner | `im 15.1.0` | Structural sharing and deterministic ordered collections are part of the cross-platform project model. Windows and macOS do not expose a common immutable-collection ABI with these semantics. |
| Canonical integrity | Core/model owner | `sha2 0.10.9` | SHA-256 digests are part of the portable project/revision protocol. Platform crypto APIs have different FFIs and deployment behavior and would add platform divergence for a fixed, non-secret digest primitive. |
| Evidence serialization | Fixture/oracle owner | `serde 1.0.228`, with `derive`; `serde_json 1.0.149` | Versioned fixture, oracle, trace, render, and benchmark evidence needs one checked, cross-platform JSON schema implementation. Platform property-list and COM serializers do not provide a shared deterministic schema or portable Rust representation. |
| Transactional persistence | Persistence owner | `rusqlite 0.40.1`, with default features disabled and only `backup` and `bundled` enabled | The store requires one schema and recovery/backup contract on both platforms. System SQLite versions and platform database frameworks do not guarantee the pinned SQLite version, source ID, compile options, or identical WAL behavior. |
| Bundled SQLite FFI | Persistence owner | `libsqlite3-sys 0.38.1`, selected by `rusqlite` | This builds the reviewed SQLite amalgamation into the application. System linking is intentionally disabled so an older or differently compiled library cannot silently replace it. |
| Native C build | Build/release owner | `cc`, `find-msvc-tools`, `shlex`, `pkg-config`, and `vcpkg` as locked upstream build dependencies | Cargo needs a reproducible host-side build path for the bundled amalgamation. `pkg-config` and `vcpkg` are present in the upstream build graph but must not select a system SQLite in release builds. |

All transitive packages inherit the security owner and capability justification
of the group shown below. New capabilities, features, versions, sources, or
target scopes require a new review; a transitive relationship is not an
automatic approval.

## Exact Cargo graph

License expressions are the SPDX-style expressions published by the crates,
with legacy `MIT/Apache-2.0` metadata normalized to `MIT OR Apache-2.0`.
`MPL-2.0+` is preserved as published by the affected crates and must be handled
by the release notice/source-offer review.

| Package | Capability group | Build/target classification | License expression | `Cargo.lock` checksum |
| --- | --- | --- | --- | --- |
| `bitflags 2.13.0` | Persistence | Runtime, all supported targets | `MIT OR Apache-2.0` | `b4388bee8683e3d04af747c73422af53102d2bd24d9eadb6cbc100baef4b43f8` |
| `bitmaps 2.1.0` | Immutable model | Runtime, all supported targets | `MPL-2.0+` | `031043d04099746d8db04daf1fa424b2bc8bd69d92b25962dcde24da39ab64a2` |
| `block-buffer 0.10.4` | Canonical integrity | Runtime, all supported targets | `MIT OR Apache-2.0` | `3078c7629b62d3f0439517fa394996acacc5cbc91c5a20d8c658e77abd503a71` |
| `cc 1.2.67` | Native C build | Build host only; compiles bundled SQLite for each target | `MIT OR Apache-2.0` | `e17dd265a7d0f31ef544e1b20e03add05d3b45b491b633b10d67145d2acc1a38` |
| `cfg-if 1.0.4` | Canonical integrity | Runtime, target selection | `MIT OR Apache-2.0` | `9330f8b2ff13f34540b44e946ef35111825727b38d33286ef986142615121801` |
| `cpufeatures 0.2.17` | Canonical integrity | Runtime, target-specific CPU feature detection | `MIT OR Apache-2.0` | `59ed5838eebb26a2bb2e58f6d5b5316989ae9d08bab10e0e6d103e656d1b0280` |
| `crypto-common 0.1.7` | Canonical integrity | Runtime, all supported targets | `MIT OR Apache-2.0` | `78c8292055d1c1df0cce5d180393dc8cce0abec0a7102adb6c7b1eef6016d60a` |
| `digest 0.10.7` | Canonical integrity | Runtime, all supported targets | `MIT OR Apache-2.0` | `9ed9a281f7bc9b7576e61468ba615a66a5c8cfdff42420a70aa82701a3b1e292` |
| `fallible-iterator 0.3.0` | Persistence | Runtime, all supported targets | `MIT OR Apache-2.0` | `2acce4a10f12dc2fb14a218589d4f1f62ef011b2d0cc4b3cb1bba8e94da14649` |
| `fallible-streaming-iterator 0.1.9` | Persistence | Runtime, all supported targets | `MIT OR Apache-2.0` | `7360491ce676a36bf9bb3c56c1aa791658183a54d2744120f27285738d90465a` |
| `find-msvc-tools 0.1.9` | Native C build | Build host only; active for MSVC discovery | `MIT OR Apache-2.0` | `5baebc0774151f905a1a2cc41989300b1e6fbb29aff0ceffa1064fdd3088d582` |
| `generic-array 0.14.7` | Canonical integrity | Runtime plus package build script, all supported targets | `MIT` | `85649ca51fd72272d7821adaf274ad91c288277713d9c18820d8499a7ff69e9a` |
| `im 15.1.0` | Immutable model | Direct runtime, all supported targets | `MPL-2.0+` | `d0acd33ff0285af998aaf9b57342af478078f53492322fafc47450e09397e0e9` |
| `itoa 1.0.17` | Evidence serialization | Runtime, all supported targets | `MIT OR Apache-2.0` | `92ecc6618181def0457392ccd0ee51198e065e016d1d527a7ac1b6dc7c1f09d2` |
| `libc 0.2.186` | Canonical integrity | Target-conditional runtime via `cpufeatures` | `MIT OR Apache-2.0` | `68ab91017fe16c622486840e4c83c9a37afeff978bd239b5293d61ece587de66` |
| `libsqlite3-sys 0.38.1` | Bundled SQLite FFI | Runtime FFI plus package build script, all supported native targets | `MIT` | `f6c19a05435c21ac299d71b6a9c13db3e3f47c520517d58990a462a1397a61db` |
| `memchr 2.8.0` | Evidence serialization | Runtime, all supported targets | `Unlicense OR MIT` | `f8ca58f447f06ed17d5fc4043ce1b10dd205e060fb3ce5b979b8ed8e59ff3f79` |
| `pkg-config 0.3.33` | Native C build | Build host only; locked upstream helper, system SQLite selection prohibited | `MIT OR Apache-2.0` | `19f132c84eca552bf34cab8ec81f1c1dcc229b811638f9d283dceabe58c5569e` |
| `proc-macro2 1.0.106` | Evidence serialization | Build host only; derive-macro expansion | `MIT OR Apache-2.0` | `8fd00f0bb2e90d81d1044c2b32617f68fcb9fa3bb7640c23e9c748e53fb30934` |
| `quote 1.0.46` | Evidence serialization | Build host only; derive-macro expansion | `MIT OR Apache-2.0` | `dfbc457d0c7a0759a614551b11a6409e5951f6c7537be1f1b7682b9ae9230368` |
| `rand_core 0.6.4` | Immutable model | Runtime, all supported targets | `MIT OR Apache-2.0` | `ec0be4795e2f6a28069bec0b5ff3e2ac9bafc99e6a9a7dc3547996c5c816922c` |
| `rand_xoshiro 0.6.0` | Immutable model | Runtime, all supported targets | `MIT OR Apache-2.0` | `6f97cdb2a36ed4183de61b2f824cc45c9f1037f28afe0a322e9fff4c108b5aaa` |
| `rusqlite 0.40.1` | Persistence | Direct runtime, all supported native targets; `backup,bundled` only | `MIT` | `11438310b19e3109b6446c33d1ed5e889428cf2e278407bc7896bc4aaea43323` |
| `serde 1.0.228` | Evidence serialization | Direct runtime with derive support, all supported targets | `MIT OR Apache-2.0` | `9a8e94ea7f378bd32cbbd37198a4a91436180c5bb472411e48b5ec2e2124ae9e` |
| `serde_core 1.0.228` | Evidence serialization | Runtime, all supported targets | `MIT OR Apache-2.0` | `41d385c7d4ca58e59fc732af25c3983b67ac852c1a25000afe1175de458b67ad` |
| `serde_derive 1.0.228` | Evidence serialization | Build host only; proc-macro expansion | `MIT OR Apache-2.0` | `d540f220d3187173da220f885ab66608367b6574e925011a9353e4badda91d79` |
| `serde_json 1.0.149` | Evidence serialization | Direct runtime, all supported targets | `MIT OR Apache-2.0` | `83fc039473c5595ace860d8c4fafa220ff474b3fc6bfdb4293327f1a37e94d86` |
| `sha2 0.10.9` | Canonical integrity | Direct runtime, all supported targets | `MIT OR Apache-2.0` | `a7507d819769d01a365ab707794a4084392c824f54a7a6a7862f8c3d0892b283` |
| `shlex 2.0.1` | Native C build | Build host only; compiler-argument parsing | `MIT OR Apache-2.0` | `f8fadd59c855ef2080decdef8ff161eb6661b86933c9d82e5ba29dc602a55aba` |
| `sized-chunks 0.6.5` | Immutable model | Runtime, all supported targets | `MPL-2.0+` | `16d69225bde7a69b235da73377861095455d298f2b970996eec25ddbb42b3d1e` |
| `smallvec 1.15.2` | Persistence | Runtime, all supported targets | `MIT OR Apache-2.0` | `8ed6a63f02c8539c91a8685a86f4099661ba3da017932f6ebbea6de3f0fa7c90` |
| `syn 2.0.118` | Evidence serialization | Build host only; derive-macro parsing | `MIT OR Apache-2.0` | `1b9ae57f904213ebb649ce6895b8a66c66f0203b9319718f69a5612a065b1422` |
| `typenum 1.20.1` | Immutable model and canonical integrity | Runtime, all supported targets | `MIT OR Apache-2.0` | `b6f5e870be6c3b371b77fe0ee0bafb859fa4964b4404c27de1d380043c4dda20` |
| `unicode-ident 1.0.24` | Evidence serialization | Build host only; derive-macro parsing | `(MIT OR Apache-2.0) AND Unicode-3.0` | `e6e4313cd5fcd3dad5cafa179702e2b244f760991f45397d14d4ebf38247da75` |
| `vcpkg 0.2.15` | Native C build | Build host only; locked upstream helper, system SQLite selection prohibited | `MIT OR Apache-2.0` | `accd4ea62f7bb7a82fe23066fb0957d48ef677f6eeb8215f372f52e48bb32426` |
| `version_check 0.9.5` | Immutable model and canonical integrity | Build host only; Rust-version feature selection | `MIT OR Apache-2.0` | `0b928f33d975fc6ad9f86c8f283853ad26bdd5b10b7f1542aa2fa15e2289105a` |
| `zmij 1.0.21` | Evidence serialization | Runtime numeric formatting, all supported targets | `MIT` | `b8848ee67ecc8aedbaf3e4122217aff892639231befc6a1b58d29fff4c2cabaa` |

## Embedded SQLite component

`libsqlite3-sys 0.38.1` contains and compiles SQLite **3.53.2**. SQLite is
therefore a shipped component even though it has no separate entry in
`Cargo.lock`; the generated SBOM must add it explicitly and relate it to
`libsqlite3-sys` with an SPDX `CONTAINS` relationship (or the CycloneDX
equivalent).

- SQLite version number: `3053002`
- SQLite source ID: `2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24`
- Provenance: bundled `sqlite3.c`/`sqlite3.h` in `libsqlite3-sys 0.38.1`
- License status: public domain; SQLite says no license is required
- Release rule: runtime version, source ID, critical compile options,
  `journal_mode=WAL`, and `synchronous=FULL` must match the ProjectStore
  contract. A system SQLite substitution is a release failure.

Primary sources: [rusqlite 0.40.1 build and license notes](https://docs.rs/crate/rusqlite/0.40.1),
[rusqlite 0.40.1 manifest](https://github.com/rusqlite/rusqlite/blob/v0.40.1/Cargo.toml),
[libsqlite3-sys bundled header](https://github.com/rusqlite/rusqlite/blob/v0.40.1/libsqlite3-sys/sqlite3/sqlite3.h),
[libsqlite3-sys build flags](https://github.com/rusqlite/rusqlite/blob/v0.40.1/libsqlite3-sys/build.rs),
[rusqlite MIT license](https://github.com/rusqlite/rusqlite/blob/v0.40.1/LICENSE),
and [SQLite public-domain statement](https://sqlite.org/copyright.html).

## Release notice and SBOM rules

- Preserve the rusqlite/libsqlite3-sys MIT copyright and permission notice in
  distributions. Apply the selected MIT or Apache-2.0 route consistently for
  dual-licensed crates and collect the corresponding complete texts.
- Record and satisfy the source/notice obligations for `im`, `bitmaps`, and
  `sized-chunks` under their published `MPL-2.0+` terms. This inventory does
  not decide that legal question.
- List SQLite 3.53.2 as public-domain software even though SQLite does not
  require attribution. In jurisdictions that do not recognize public-domain
  dedication, the dated legal review decides whether additional assurance is
  required.
- Include runtime, target-conditional, and build-host components in the source
  SBOM, labeling their scope. A shipped-binary SBOM may omit tools not present
  in the artifact only when the source/build SBOM retains them.
- Regenerate the inventory and notices whenever `Cargo.lock`, feature
  selection, target set, or the embedded SQLite source changes.
