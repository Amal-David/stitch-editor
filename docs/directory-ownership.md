# Directory ownership

T-0015 owns the workspace roots and build contract. Later tasks add code only
inside the named root; they do not create a parallel workspace, CMake entrypoint,
or package manager.

| Root | Owner/task | Contract |
| --- | --- | --- |
| `crates/editor-core/` | T-0007 | Pure Rust editorial model, reducer, render planning, and scheduler; no platform pointers or unsafe code. |
| `crates/project-store/` | T-0007 | SQLite-backed project durability implementation after its contract is accepted. |
| `crates/contracts/` | T-0007/T-0009 | Versioned Rust-side schema/serialization contracts; not the C ABI. |
| `native/c-abi/` | T-0009 | Stable C header and bridge boundary; T-0015 provides layout-only declarations. |
| `desktop/qt/` | T-0009 | Qt windows, input, DPI/accessibility, and render-thread shell contract. |
| `native/macos/` | T-0010 | Objective-C++ AVFoundation/VideoToolbox/CoreVideo/Metal bridge. |
| `native/windows/` | T-0011 | C++ Media Foundation/D3D11/COM bridge. |
| `tools/fixtures/` and `tools/oracles/` | T-0008 | Fixture recipes, manifests, independent oracle tools, and corpus data; never generated media binaries. |
| `benchmarks/` | T-0014 | Reproducible harnesses and retained raw evidence metadata. |
| `packaging/` | T-0014 | Signed/notarized package definitions, SBOM manifests, and clean-machine checks. |
| `docs/`, `scripts/`, `toolchains/`, root build files | T-0015 | Shared policy, toolchain, CI, and bootstrap contract. |
| `.superplan/changes/` | Architecture council | Durable plans, specifications, research, and tasks. Never edit `.superplan/runtime/`. |

The eventual FFI crate may use narrow, reviewed unsafe code. It must be separate
from `editor-core`, document every unsafe boundary, and never make platform
object ownership part of the Rust editorial core.
