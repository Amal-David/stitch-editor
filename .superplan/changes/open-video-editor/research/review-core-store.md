# T-0007 deterministic core and ProjectStore review evidence

Date: 2026-07-14

## Scope and host

This review covers the UI- and media-independent Rust contracts, history, and
SQLite project store on one macOS host. It does not represent Windows execution
or physical power-loss proof.

- Host: MacBook Pro `Mac16,8`, Apple M4 Pro (14 cores), 48 GB memory
- OS: macOS 26.3 build 25D122
- Rust: `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  `aarch64-apple-darwin`, LLVM 22.1.6
- Store runtime: bundled SQLite 3.53.2, source ID
  `2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24`

## Determinism fixtures

- Rational property seed: `0x9e3779b97f4a7c15`, 10,000 generated cases.
- History sequence seeds: every integer in `0..1000`, with full undo, redo,
  and replay of each five-command sequence.
- Time vectors: 10,000 absolute timestamps at 24000/1001, 30000/1001, and
  60000/1001 frames per second.
- Fixed mixed 1,000-command revision hash:
  `5eaa34d5cfb37f9d3270723a8f299ac501fe8e42c41619acbc7300e595a2c81e`.
- Fixed empty-project semantic digest:
  `36bb938b6c56a2a9a23b62057eded25c41ef1f30a1d2afd8e104bb5699d0c3c2`.
- Fixed root revision hash for the canonical AddTrack golden:
  `b06750d0e62ece073cf006ceea9380318af42465c046e04ad8f0f55653efbe26`.

## Verification results

All commands used the locked offline graph.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Passed with no issues |
| `cargo test --workspace --locked --offline` | 37 passed across 6 suites in 1.00 s |
| `cargo test --workspace --release --locked --offline` | 37 passed across 6 suites in 0.93 s |
| Six concurrent runs of the targeted archive-fault test | 6/6 passed; each test run reported 0.06-0.10 s |
| Canonical `./scripts/bootstrap.sh all` with the pinned Rust and Homebrew Qt 6.11.1 paths | Policy passed; 18 contract, 4 history, and 15 store tests passed; Qt shell configured and built; CTest ran and honestly reported no registered CMake tests |

The ProjectStore matrix includes deterministic errors before commit and after
commit/before response; boundary faults before, during, and after snapshot,
checkpoint, compaction, and archive; tampered schema/log/head/receipt/snapshot
rejection; and abnormal child `abort()` plus reopen for pre-commit,
post-commit/pre-response, and post-maintenance boundaries. Receipt queries stay
available after a sticky snapshot failure, and automatic snapshot work runs
after the committed receipt has been queued to the caller.

## Failure history and correction

The first independent workspace audit failed one archive-fault test because
two concurrent test processes could choose the same timestamp-only temporary
directory. Test paths now include the process ID and a monotonic process-local
counter. The full debug/release suites and six parallel targeted reruns then
passed. The initial failure is retained here rather than represented as an
always-green run.

## Open acceptance evidence

- Windows has not run the canonical vectors or compared their project and
  revision hashes with macOS. The local host has no Windows runner, Wine/MinGW,
  or Windows Rust target, and this repository has no remote for hosted CI.
- The child-abort matrix is process-termination evidence, not a claim about
  faulty storage controllers, lying VFS implementations, torn writes outside
  SQLite's guarantees, or physical power removal.
- Archive publication fsyncs the containing directory on Unix. The safe Rust
  standard-library implementation makes no equivalent Windows directory-fsync
  claim; Windows durability remains an explicit platform-validation item.

T-0007 therefore remains open even though the implementation and macOS review
are accepted.
