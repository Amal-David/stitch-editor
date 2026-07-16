# T-0015 bootstrap verification

## Local macOS evidence (2026-07-14)

- Foundation commit `0b930c5e3ac09b5f20b1f23d18b61d54c8e6c0a4` was checked out detached into a new Git worktree. The canonical command passed there and `git status --short` remained empty after the build; generated Rust/CMake output stayed ignored.

- `rustc 1.97.0`, `cargo 1.97.0`, `cmake 3.31.6`, Xcode 26.4.1, and macOS SDK
  26.4 were present. `./scripts/bootstrap.sh platform` captured these values in
  the ignored build evidence and enforced the Xcode 15+/SDK 14+ contract.
- `./scripts/policy.sh` passed.
- `./scripts/bootstrap.sh rust` passed formatting, Clippy with warnings denied,
  and the workspace unit-test discovery/run.
- With `Qt6_DIR=/opt/homebrew/opt/qtbase/lib/cmake/Qt6` and the documented
  Homebrew split `CMAKE_PREFIX_PATH`, `./scripts/bootstrap.sh all` passed its
  policy, Rust, CMake configure, Qt shell build, and CTest phases without a
  download. The captured Qt6_DIR is the local Qt Base package path.
- CTest passed the linked `stitch_c_abi_lifecycle_test` (1/1). The Qt shell is
  compiled as an integration contract, not exercised as an editor or
  runtime-preview test.
- The compiled shell was launched on the local display and remained in its Qt
  event loop for more than five seconds until manually interrupted. Its
  scene-graph initialization assertion would terminate on any backend other
  than Metal, so this is a local Qt/Metal backend smoke check. It is not
  evidence of a decoder surface, frame lease, media presentation, or editor
  feature.

## Hosted macOS and Windows evidence (2026-07-16)

[Bootstrap run 29491779087](https://github.com/Amal-David/stitch-editor/actions/runs/29491779087)
passed from clean checkouts of commit
`e83bbadb5cd8d008ae50bcbd85d8a1e4ab11a38a` on both declared runners:

- The [macOS job](https://github.com/Amal-David/stitch-editor/actions/runs/29491779087/job/87599364276)
  used the `macos15` image `20260715.0234.1`, Xcode 16.4, macOS SDK 15.5,
  Qt 6.11.1, and AppleClang 17.0.0.17000013. Its verbose build log contains
  `STITCH_EXPECT_METAL=1`, and CTest passed the linked C ABI test (1/1).
- The [Windows job](https://github.com/Amal-David/stitch-editor/actions/runs/29491779087/job/87599364271)
  used the `win22` image `20260706.237.1`, Visual Studio 17.14.37411.7,
  Windows SDK 10.0.26100.0, Qt 6.11.1, and MSVC 19.44.35228.0. Its verbose
  build log contains `STITCH_EXPECT_D3D11=1`, and configuration-aware CTest
  passed the linked C ABI test (1/1).
- Uploaded artifacts `bootstrap-macos-15` and `bootstrap-windows-2022` retain
  the runner/toolchain evidence, resolved Qt and compiler paths, CMake cache
  and compiler state, verbose build log, and CTest logs.

Network-dependent action and dependency provisioning occurs before the
canonical command. `./scripts/bootstrap.sh all` uses locked, offline Cargo
operations and contains no Qt or CMake download path. This is compile/link and
C ABI lifecycle evidence; it does not claim Windows GUI launch, runtime D3D11
presentation, a media engine, a decoder surface, or an editor feature.

## Automated shell self-test presentation

`stitch_editor_shell --self-test` is visually hidden but not headless: it keeps
the real native window, Metal swapchain, and Qt scene graph needed by the gate,
while setting the window fully transparent and unable to accept focus. The
self-test also makes the native window transparent to input and fails if it
becomes active or focused. Use `--self-test-visible` only for an intentional
visual diagnostic run.

## CI action provenance (verified 2026-07-14)

The workflow pins every action to an immutable commit and retains the release
tag as a comment. The tag/ref was checked with `git ls-remote` before use:
checkout v7.0.0 (`9c091bb...`), rust-toolchain v1 (`e97e2d...`), setup-cpp
v1.8.1 dereferenced commit (`8170d66...`), install-qt-action v4.3.1
(`48d3ad6...`), and upload-artifact v4.6.2 (`ea165f8...`).

Qt 6.11 changed the public Windows repository layout after `aqtinstall` 3.3.0
was released. The install action therefore pins its documented `aqtsource`
input to immutable upstream merge commit `8c3695d...` (aqtinstall PR #1000),
which adds the Windows x64 Qt 6.11 layout. Remove that source pin only after a
released aqtinstall containing the same fix passes both hosted legs.

CI uses `macos-15` rather than the retiring `macos-14` image. Its selected
Xcode/macOS SDK is captured and checked against the minimum platform contract;
Windows runs setup-cpp with `compiler: msvc` and `vcvarsall: true` before the
offline bootstrap captures and validates Visual Studio 17.x and Windows SDK
10.0.26100.0.

The Windows job resolves `link.exe` from the activated Visual Studio tree and
exports that absolute path through Cargo's target-specific linker variable.
This prevents Git Bash's unrelated `/usr/bin/link.exe` from shadowing the MSVC
linker. The dependency policy uses `rg` when available and a tracked-file
`git grep` fallback otherwise, so a missing optional search binary cannot turn
failed checks into a misleading policy success.
