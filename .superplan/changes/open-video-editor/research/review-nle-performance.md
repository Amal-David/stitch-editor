# Cross-review: NLE semantics and benchmark contract

**Reviewed:** `research/mediabunny-web.md`, `research/native-architecture.md`,
`research/nle-performance.md`, ADR-001, and the vertical-slice contract.

## Verdict: revise before implementation-ready acceptance

The selected native architecture remains coherent: keep the Rust editorial core,
native media/GPU bridges, a bounded scheduler, and a narrow initial format scope.
The documents also correctly reject a WebView/frame-IPC hot path and make public
performance claims conditional on disclosed, like-for-like evidence.

However, ADR-001 and the slice should be revised before they are treated as
implementation-ready. Three P0 contract conflicts would otherwise make the
determinism/recovery acceptance criteria either impossible or ambiguous:

1. exact rational time is incorrectly described as exactly convertible at every
   platform boundary;
2. "same render-plan digest" conflicts with a quality profile that legitimately
   changes source/proxy, resolution, and implementation; and
3. recovery promises durable acknowledged edits without pinning the SQLite
   durability/ownership policy that makes that promise meaningful.

This is a **revise**, not **reopen**, verdict. Reopen ADR-001 only if the required
native surface and real-time prototype evidence fails.

## Material objections and exact corrections

### P0 — Canonical rational time must not become platform-tick time

`native-architecture.md` allows "i64 ticks or rational timebase", while the NLE
report specifies rationals and says audio is converted "exactly at the graph
boundary." These are not equivalent. In particular, Media Foundation sample and
clock times are integer 100-nanosecond units, so a 24000/1001 CFR frame duration
cannot be represented exactly in that API. Microsoft explicitly specifies 100 ns
sample times and permits rounded-down calculated durations
([`IMFSample::SetSampleTime`](https://learn.microsoft.com/en-us/windows/win32/api/mfobjects/nf-mfobjects-imfsample-setsampletime),
[MFT timestamps](https://learn.microsoft.com/en-us/windows/win32/medfound/time-stamps-and-durations)).

**Replace the time contract in ADR-001, the native report, slice, and my NLE
report with this exact rule:**

- The canonical project and render plan use normalized rationals only; remove
  the `i64 ticks` alternative except as a private cache/index optimization.
- A platform adapter converts each canonical **absolute** PTS/duration to its
  required integer unit once, using a named policy and checked 128-bit arithmetic.
  It must not accumulate rounded relative durations.
- The adapter returns and logs `canonical_time`, `adapter_tick`, `rounding_policy`,
  and `error_num/error_den`; each output track's PTS sequence is strictly
  monotonic and has bounded conversion error of at most half an adapter tick
  (or the documented `floor` rule when an API requires it).
- Container timestamps are produced from canonical output time/track timescale,
  not reconstructed from rounded decoder callback times. Audio mix positions stay
  integer sample indices at 48 kHz; device/MF timestamps are an adapter view.

Add a 10,000-frame 23.976/29.97/59.94 round-trip test and a VFR PTS test to the
functional gate. Its oracle is the exact rational sequence plus the bounded
adapter-error log, not equality of Windows and macOS integer callback timestamps.

### P0 — Define semantic equivalence separately from execution equivalence

The slice requires preview and export to have the "same render-plan digest," yet
the same document permits a `QualityProfile` to select proxies, resolution, and
implementation quality. My NLE report also includes `QualityProfile` in cache
keys. Therefore the existing acceptance criterion is either unsatisfiable or
would conceal a real plan difference.

**Replace it with two required digests:**

- `semantic_plan_digest`: revision, timeline ranges/time maps, effect order and
  sampled parameters, compositing, audio routing/automation, color intent, and
  missing-media policy. It **must match** between preview and export for the
  same revision/range.
- `execution_plan_digest`: semantic digest plus proxy/original selection,
  resolution, pixel format/precision, backend, shader/encoder build, and cache
  policy. It may differ and must be emitted in every trace/render manifest.

The acceptance oracle is then: final export uses `final` execution policy and
original assets; preview using original media is numerically compared with final
at selected frames; proxy preview is compared after declared rescale/precision
tolerance; and no preview-only effect/time-map/color branch is allowed.

### P0 — Recovery needs an acknowledged-durability definition

SQLite atomicity prevents a partially committed transaction from becoming a
valid state, but it does not by itself define which UI edits are durable after
power loss. SQLite documents that WAL has side files that are persistent state
and must remain with the database; it also documents WAL checkpointing and the
current fixed versions for the 2026 WAL-reset bug
([SQLite WAL](https://www3.sqlite.org/wal.html)).

**Add a `ProjectStore v1` contract:**

- Project databases are local single-writer files; opening a live project on a
  network/shared filesystem is unsupported until separately validated.
- Bundle a fixed SQLite release >= 3.51.3 (or documented fixed backport), record
  its version and compile options, and verify the configured `journal_mode` and
  `synchronous` setting on open. Do not rely on a misspelled/unknown pragma:
  SQLite notes unknown pragmas can be ignored
  ([SQLite pragma documentation](https://www.sqlite.org/pragma.html)).
- A command is **acknowledged** only after the transaction containing command,
  immutable revision, and head pointer commits under the documented durability
  setting. For the stated power-loss claim, use `synchronous=FULL` (and document
  the storage caveat); `NORMAL` may be offered only with an explicitly weaker
  "not yet durable" UI state. SQLite itself describes FULL as the every-commit
  sync setting ([SQLite internals](https://sqlite.org/talks/howitworks-20240624.pdf)).
- Persist the database, `-wal`, and `-shm` as one live unit; archive only after
  verified checkpoint/close. One persistence worker owns writes and compaction;
  compaction is a cancel-safe transaction, never an in-place file replacement.
- Crash tests cut power/kill before commit, after WAL commit but before response,
  during snapshot, during checkpoint, and during archive. Pre-commit failure must
  leave the prior head with no receipt. Post-commit ambiguity must retain the new
  head and durable receipt so retrying the same caller-stable `RequestId` returns
  it without applying the command twice. A returned receipt is never silently
  lost and the head is never silently altered. Physical power-loss claims remain
  conditional on the operating system, VFS, filesystem, and device honoring sync
  semantics; process-kill tests alone do not prove arbitrary hardware behavior.

### P1 — The fixture matrix currently conflicts with the declared codec scope

The NLE research fixture uses 10-bit 4:2:2 all-intra material, whereas the slice
deliberately narrows v1 to 8-bit 4:2:0 AVC. The slice also uses AVC+PCM in MOV
for `QHD-I`, but ADR-001 promises only capability-probed MP4/MOV and does not
prove that exact MOV/PCM demux path on both OS media stacks. Finally,
`3K-final` permits a typed unsupported result even though the Outcome says the
slice exports a 3K deliverable.

**Correct the contract as follows:**

- Make the slice's cross-platform baseline a generated `MP4/AVC all-IDR + AAC`
  asset with a separate lossless WAV/oracle source, pinning profile, level,
  chroma, bitrate, GOP=1/closed-GOP, audio encoder delay/padding, container
  timescale, and exact artifact hash. Keep MOV+PCM and 10-bit/4:2:2 as named
  post-slice probes, not baseline requirements.
- State that an all-IDR fixture tests decode/editability, not the app's proxy
  encoder. The fixed proxy profile must pin codec, GOP/intra policy, dimensions,
  audio, PTS map, and parent/profile digest. Pre-generated proxies test playback;
  proxy *generation* is a separate capability/throughput result. If the selected
  platform encoder cannot create the required profile, it is a typed capability
  failure and the 3K-proxy gate cannot be claimed as passed.
- Resolve the `3K-final` contradiction: either require capability-proven AVC
  export on both baseline machines, or redefine slice success as a valid
  lossless-oracle intermediate plus a separately reported optional platform
  delivery export. Do not call the latter a completed cross-platform 3K export.
- Keep the VFR asset's exact per-sample PTS/duration manifest authoritative;
  codec-declared nominal frame rate must not override it.

### P1 — Make real-time audio and preview/export audio equivalence testable

The architecture correctly says the audio clock is preview master and the device
callback must not allocate, lock, wait, log, or do I/O. The slice does not yet
define its device/buffer configuration or how those prohibitions are observed;
two tracks also under-test the declared audio mixer. The device path cannot be
bit-identical to offline audio when it resamples or buffers, even though the
editorial mix must be identical.

**Add these gates:**

- Test 48 kHz stereo at an explicitly recorded 128- and 256-frame device buffer
  (shared/exclusive mode and fallback reported) with 16 active tracks, fades,
  gain automation, and one declared-latency effect. Device-change/unplug is a
  required recovery test.
- The audio callback owns fixed preallocated buffers and lock-free bounded rings;
  it receives no ownership-changing UI/project commands. Add debug allocator,
  lock, I/O, and logger intercept counters around the callback; all must be zero
  in the playback run. This turns a desirable coding rule into evidence.
- Define `semantic_mix_digest` shared by preview and export. Preview may add only
  device conversion/buffering after that mix; offline export uses canonical blocks.
  Compare both to the same pre-device PCM oracle after compensating the measured
  device latency. Media Foundation itself recommends audio as the time source for
  A/V playback ([presentation clock](https://learn.microsoft.com/en-us/windows/win32/medfound/presentation-clock)).

### P1 — Replace machine-ambiguous resource gates with owned-budget gates

`RSS <= 4 GiB` and `VRAM <= 2 GiB on a 16 GiB baseline machine` are not portable
on unified-memory Macs and do not distinguish editor-owned resources from Qt,
driver, and OS caching. Likewise 16.7 ms is only one refresh interval at 60 Hz.

**Replace with:** a checked-in baseline-machine manifest that states physical
memory, unified/local GPU budget, display refresh, power mode, and cache limits;
hard gates for editor-owned CPU-frame, GPU-texture, decode, and queue-byte token
budgets; process RSS/GPU process memory as required observations; and input
event-dequeue-to-command-accepted p99 <= one reported refresh interval (16.7 ms
only on 60 Hz). Retain the <=2% second-run growth gate for tracked editor-owned
resources; report allocator/driver retention separately.

### P1 — Oracles must be independently executable, not merely described

The documents correctly require predeclared floating tolerances and decoded
output checks, but they do not name a separately implemented verifier, version,
or source of the CPU reference. A renderer cannot certify itself.

**Before any comparative claim, add an `oracle-lock` manifest** naming pinned
versions/hashes for: fixture generator, canonical CPU reference evaluator,
independent demux/decode/container verifier, expected output metadata checker,
and comparison tolerances. The verifier may be a test-only dependency and need
not be a production FFmpeg dependency, but it must not share the app's demux/mux
implementation. Export checks require both semantic-oracle comparison and an
independent decode/container parse. Store failed frames/audio windows/traces.

## Missing prototype evidence (must exist before acceptance)

1. **Native-surface proof:** a retained `CVPixelBuffer -> CVMetalTexture` and
   `IMF/MFT -> ID3D11Texture2D` frame survives until its actual GPU completion;
   trace shows zero steady-state GPU-to-CPU readbacks after warm-up. Qt scene
   graph integration, device loss, DPI/multi-monitor, and a resize lifecycle are
   included, not assumed.
2. **Time-adapter proof:** 10,000 CFR frames at 24000/1001 and a VFR sequence
   convert to CMTime and 100-ns paths with monotonicity and the specified bounded
   error; encode/mux/redecode retains the canonical PTS manifest.
3. **Persistence fault proof:** the exact ProjectStore configuration passes every
   scripted crash point on APFS and NTFS, validates the database, and restores
   the acknowledged revision chain.
4. **Audio proof:** the 16-track 128/256-frame test has zero callback
   allocation/lock/I/O/log counters and no underruns; device offset and drift are
   separately reported.
5. **Codec/proxy proof:** both named baseline machines actually decode the fixed
   QHD and 3K fixtures and generate/play the fixed proxy profile, with all real
   hardware/software capability selections recorded.
6. **Oracle proof:** an intentional time-map, color, audio-fade, and stale-epoch
   bug each fails the correct independent oracle. Passing only happy-path exports
   is not evidence that the oracle is meaningful.

## Supported claims after the corrections

- Rust as the deterministic core and Qt/native bridges as the vertical-slice
  baseline is a supported *architecture hypothesis*, contingent on the six
  prototypes above; it is not a performance result.
- Electron/MediaBunny is useful as a bounded browser comparator but must not own
  editorial truth or be represented as the production native media path.
- The project can honestly claim non-destructive editing, exact canonical
  rational editorial time, shared semantic preview/export plans, deterministic
  undo/reopen/recovery, and bounded work **only after** their listed oracles and
  fault tests pass.
- Until a public, identical cross-editor suite exists, the only allowed
  performance wording is: "On this disclosed machine and configuration, this
  build achieved this metric under this fixture." "Most efficient," "fastest,"
  and cross-editor percentage claims remain unsupported.
