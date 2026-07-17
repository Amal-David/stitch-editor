# T-0008 implementation acceptance review

**Verdict: BLOCK / revise.**

This read-only review ran `cargo test --workspace`, `cargo clippy --workspace
--all-targets -- -D warnings`, and `git diff --check`; all passed. The delivered
code is a useful schema/test foundation, but not the fixture, independent
oracle, and evidence harness required by T-0008.

## P1 blockers

1. **No encoded fixture corpus is produced.** The only executable prints a
   recipe, not local AVC/AAC MP4 output or a native capability record:
   [fixture-recipe.rs](../../../tools/fixtures/src/bin/fixture-recipe.rs:6).
   `EncodedArtifactManifest` is schema-only
   ([fixture library](../../../tools/fixtures/src/lib.rs:343)); all checked-in
   artifact digests are null ([corpus](../../../tools/fixtures/recipes/corpus-v1.json:39),
   [corpus](../../../tools/fixtures/recipes/corpus-v1.json:51),
   [corpus](../../../tools/fixtures/recipes/corpus-v1.json:63),
   [corpus](../../../tools/fixtures/recipes/corpus-v1.json:76), and
   [corpus](../../../tools/fixtures/recipes/corpus-v1.json:91)). The source
   writers output raw RGBA and integer PCM WAV, not 8-bit 4:2:0 AVC/AAC MP4
   ([fixture library](../../../tools/fixtures/src/lib.rs:644),
   [fixture library](../../../tools/fixtures/src/lib.rs:894)); the slice oracle
   also specifies 32-bit float audio blocks ([vertical slice](../specs/vertical-slice.md:44)).

2. **`oracle-lock` does not pin working independent components.** Its CPU
   reference, independent verifier, and metadata checker source paths are
   Markdown contracts ([oracle lock](../../../tools/oracles/oracle-lock.json:12),
   [oracle lock](../../../tools/oracles/oracle-lock.json:26),
   [oracle lock](../../../tools/oracles/oracle-lock.json:33)). The verifier
   contract explicitly says native demux/decode is not implemented
   ([contract](../../../tools/oracles/contracts/independent-verifier-contract-v1.md:3)).
   `verify` only compares caller-provided `DecodedEvidence`, rather than MP4,
   AVC, AAC, or independently decoded pixels/samples
   ([oracle library](../../../tools/oracles/src/lib.rs:292)). Frame semantic
   digests hash metadata and never generated pixels
   ([fixture library](../../../tools/fixtures/src/lib.rs:803)), while the
   tolerance set contains only zero/exact values and no RMSE/PSNR/per-channel
   visual comparison ([tolerance set](../../../tools/oracles/contracts/tolerance-set-v1.json:1)).

3. **The harness does not emit observed evidence.** It is a library whose
   documented role is accepting adapter-supplied measurements
   ([harness library](../../../tools/benchmark-harness/src/lib.rs:1)). Its
   trace constructor manufactures spans at timestamps 0 through 15
   ([harness library](../../../tools/benchmark-harness/src/lib.rs:767)), and
   validation checks that synthetic B/E shape
   ([harness library](../../../tools/benchmark-harness/src/lib.rs:800)). The
   bundle holds one trace/render manifest for all runs
   ([harness library](../../../tools/benchmark-harness/src/lib.rs:321),
   [harness library](../../../tools/benchmark-harness/src/lib.rs:1016)), so
   per-run semantic/execution/mix identity is not comparable. Evidence
   references are only required to be nonempty strings, not retained files
   ([harness library](../../../tools/benchmark-harness/src/lib.rs:559)).

## Acceptance criteria

| Criterion | Result | Evidence |
| --- | --- | --- |
| Fixture variants generate exact AVC/AAC baselines, WAV oracles, and fully pinned manifests | **Fail** | No encode/mux/capability implementation; all artifact hashes null. [fixture executable](../../../tools/fixtures/src/bin/fixture-recipe.rs:6), [artifact schema](../../../tools/fixtures/src/lib.rs:343) |
| `oracle-lock` pins independent generator, CPU reference, demux/decode/container verifier, metadata checker, tolerances, and detects faults | **Fail** | Key roles are contracts; verifier consumes synthetic evidence. [lock](../../../tools/oracles/oracle-lock.json:12), [contract](../../../tools/oracles/contracts/independent-verifier-contract-v1.md:3), [verify](../../../tools/oracles/src/lib.rs:292) |
| Harness emits full raw/trace/digest/manifest/resource/path/platform evidence | **Fail** | Schema-only library and fabricated trace; no evidence producer. [harness](../../../tools/benchmark-harness/src/lib.rs:1), [trace](../../../tools/benchmark-harness/src/lib.rs:767) |
| Cold/warm runs are reproducible, interleaved, retained, and >5% regressions flagged | **Pass for the harness model** | Seeded interleaving, raw-failure retention, exact sequence binding, candidate-correctness failure, and paired-median comparison are all implemented. Real-run collection remains blocked under the preceding harness-evidence criterion. [schedule](../../../tools/benchmark-harness/src/lib.rs:64), [sequence binding](../../../tools/benchmark-harness/src/lib.rs:379), [candidate failure](../../../tools/benchmark-harness/src/lib.rs:1189), [comparison](../../../tools/benchmark-harness/src/lib.rs:1264) |
| Fixture/oracle tests are independent of editor implementation | **Pass** | T-0008 crates have no editor-crate path dependencies. [workspace](../../../Cargo.toml:1), [fixture manifest](../../../tools/fixtures/Cargo.toml:1), [oracle manifest](../../../tools/oracles/Cargo.toml:1) |

## Required verification bullets

| Verification | Result | Evidence |
| --- | --- | --- |
| Regenerate development fixtures twice and compare decoded/metadata oracles | **Fail** | Test repeats an in-memory source digest; no encoded/decode/metadata pass. [fixture tests](../../../tools/fixtures/src/lib.rs:989) |
| Inject frame, sample, PTS, color, fade, container, stale-generation faults through independent oracles | **Fail** | Tests mutate synthetic `DecodedEvidence`; no generated media/container or independent decoder runs. [oracle tests](../../../tools/oracles/src/lib.rs:531) |
| Validate a complete disclosure bundle against the vertical-slice schema | **Fail** | Test fabricates placeholder in-memory data; it does not validate retained referenced evidence or the full slice contract. [harness tests](../../../tools/benchmark-harness/src/lib.rs:1612) |

## Clean areas

- The mandatory AAC-LC 48 kHz stereo 192 kb/s baseline and 320 kb/s optional
  probe are consistently corrected ([task](../tasks/T-0008.md:24),
  [vertical slice](../specs/vertical-slice.md:28)).
- Generated media and raw PCM are excluded from git
  ([.gitignore](../../../.gitignore:1)).
- The currently named lock hashes are checked and pass
  ([oracle test](../../../tools/oracles/src/lib.rs:519)).
- The >5% threshold is strict and uses a paired-ratio median with a ten-pair
  floor ([harness library](../../../tools/benchmark-harness/src/lib.rs:1264)).
- Every raw record now has to match the published randomized sequence
  ([harness library](../../../tools/benchmark-harness/src/lib.rs:379)), and a
  candidate correctness failure is retained as a dedicated result flag even if
  the baseline also failed ([harness library](../../../tools/benchmark-harness/src/lib.rs:1157),
  [harness library](../../../tools/benchmark-harness/src/lib.rs:1189)). The
  latter case has a focused regression test ([harness tests](../../../tools/benchmark-harness/src/lib.rs:1513));
  `cargo test -p stitch-benchmark-harness` passed all 16 tests during this
  follow-up review.

## Revision required

Implement the local-only native encode/mux and capability adapters; concrete
independent CPU reference, ISO-BMFF/AVC/AAC verifier, metadata checker, and
decoded visual/audio comparator; a real per-run evidence producer; and
end-to-end artifact fault tests.
