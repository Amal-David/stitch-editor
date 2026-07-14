# T-0015 bootstrap verification

## Local macOS evidence (2026-07-14)

- Foundation commit `0b930c5e3ac09b5f20b1f23d18b61d54c8e6c0a4` was checked out detached into a new Git worktree. The canonical command passed there and `git status --short` remained empty after the build; generated Rust/CMake output stayed ignored.

- `rustc 1.90.0`, `cargo 1.90.0`, `cmake 3.31.6`, Xcode 26.4.1, and macOS SDK
  26.4 were present. `./scripts/bootstrap.sh platform` captured these values in
  the ignored build evidence and enforced the Xcode 15+/SDK 14+ contract.
- `./scripts/policy.sh` passed.
- `./scripts/bootstrap.sh rust` passed formatting, Clippy with warnings denied,
  and the workspace unit-test discovery/run.
- With `Qt6_DIR=/opt/homebrew/opt/qtbase/lib/cmake/Qt6` and the documented
  Homebrew split `CMAKE_PREFIX_PATH`, `./scripts/bootstrap.sh all` passed its
  policy, Rust, CMake configure, Qt shell build, and CTest phases without a
  download. The captured Qt6_DIR is the local Qt Base package path.
- CTest reported “No tests were found.” That is expected at T-0015: the only
  executable test discovery/run is the Rust bootstrap-contract test. The Qt
  shell is compiled as an integration contract, not exercised as an editor or
  runtime-preview test.
- The compiled shell was launched on the local display and remained in its Qt
  event loop for more than five seconds until manually interrupted. Its
  scene-graph initialization assertion would terminate on any backend other
  than Metal, so this is a local Qt/Metal backend smoke check. It is not
  evidence of a decoder surface, frame lease, media presentation, or editor
  feature.

## Windows and Qt limitations

Windows/MSVC 2022, the exact Windows SDK 10.0.26100.0, Qt 6.11.1, and actual
D3D11 runtime verification were not available in this local macOS workspace.
The Windows CI job provisions the pinned Rust components, CMake, and Qt before
invoking the same offline canonical command. The command fails if it cannot
find Visual Studio 17.x or exactly Windows SDK 10.0.26100.0, and its artifact
captures those values before the Windows path is accepted.

The hosted CI/build artifact must still include the installed
QT_ROOT_DIR-derived Qt6_DIR, CMake cache, and shell backend assertion result.
No result here claims that a media engine, decoder surface, or editor feature
exists.

## CI action provenance (verified 2026-07-14)

The workflow pins every action to an immutable commit and retains the release
tag as a comment. The tag/ref was checked with `git ls-remote` before use:
checkout v7.0.0 (`9c091bb...`), rust-toolchain v1 (`e97e2d...`), setup-cpp
v1.8.1 dereferenced commit (`8170d66...`), install-qt-action v4.3.1
(`48d3ad6...`), and upload-artifact v4.6.2 (`ea165f8...`).

CI uses `macos-15` rather than the retiring `macos-14` image. Its selected
Xcode/macOS SDK is captured and checked against the minimum platform contract;
Windows runs setup-cpp with `compiler: msvc` and `vcvarsall: true` before the
offline bootstrap captures and validates Visual Studio 17.x and Windows SDK
10.0.26100.0.
