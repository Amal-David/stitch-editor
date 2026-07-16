# Pinned toolchains

The bootstrap consumes only already-provisioned tools and never downloads a
dependency during configure, build, or test.

| Input | Pin / support contract | Capture rule |
| --- | --- | --- |
| Rust / Cargo | 1.97.0 | `rust-toolchain.toml` pins only the host toolchain. CI provisions `rustfmt` and `clippy` first; the bootstrap sets `RUSTUP_AUTO_INSTALL=0` and never requests cross targets/components. |
| CMake | 3.31.6 | `CMakePresets.json` and bootstrap version check. |
| Qt / native interfaces | Qt 6.11.1 | Set `Qt6_DIR`; exact config package required. Pin its installer/archive digest in the release manifest. |
| macOS | macOS 13+, Xcode 15+ with macOS 14 SDK+ | Capture Xcode, SDK, Metal feature set, and Qt build in every artifact. |
| Windows | Windows 10 1809+/Windows 11, MSVC 2022, Windows SDK 10.0.26100.0 | Capture MSVC, SDK, GPU driver, feature level, and Qt build in every artifact. |
| SQLite | 3.53.2 | `rusqlite 0.40.1` with default features disabled and `backup,bundled` bundles `libsqlite3-sys 0.38.1`. ProjectStore checks runtime version, source ID `2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24`, critical compile options, WAL mode, and `synchronous=FULL` before schema mutation. |
| Package inputs | Cargo.lock plus reviewed CMake configuration only | The exact runtime/build graph, license expressions, source checksums, owners, and target classifications are recorded in `docs/dependencies.md`. No package-manager source override, FetchContent, or build-time download. |

Qt 6.11.1 is the pinned standard-supported Qt release for this slice. Qt 6.8
LTS is commercial-only; do not substitute it without an architecture and legal
review. The canonical command is `./scripts/bootstrap.sh`; set `Qt6_DIR` to a
preinstalled Qt 6.11.1 kit before running its default `all` phase. When the Qt
installer action has provisioned `QT_ROOT_DIR`, the script derives and exports
`Qt6_DIR` only after finding `Qt6Config.cmake` under that root.

The supported Rust target triples are recorded in `toolchains/versions.toml`
for future release planning only. They are not listed in `rust-toolchain.toml`,
so a normal bootstrap cannot cause rustup to fetch unused cross targets.

CI first provisions pinned Rust components, CMake, and Qt using immutable action
commits, then runs the explicit locked `cargo fetch --config net.offline=false`
provisioning step. Only then does it invoke the offline bootstrap. That command captures
the actual Xcode/macOS SDK or Visual Studio/Windows SDK values into the build
evidence and fails if they do not meet the selected platform contract.

Local Homebrew verification may split Qt Base and Qt Declarative. In that case
set `Qt6_DIR=/opt/homebrew/opt/qtbase/lib/cmake/Qt6` and
`CMAKE_PREFIX_PATH=/opt/homebrew/opt/qtbase:/opt/homebrew/opt/qtdeclarative`.
This is local package-manager layout only; CI uses the consolidated
`QT_ROOT_DIR` layout supplied by the Qt installer action.

The shell selects and verifies Metal on macOS and Direct3D11 on Windows. A
software, WARP, or other Qt renderer backend fails the native-surface contract.
