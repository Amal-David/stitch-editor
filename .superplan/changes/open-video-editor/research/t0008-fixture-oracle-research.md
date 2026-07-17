# T-0008 fixture, oracle, trace, and benchmark implementation research

**Status: implementation recommendation after the AAC baseline decision.**

This document resolves T-0008 into a deterministic implementation boundary. It
does not authorize redistribution of generated H.264/AAC files: until the
dated legal gate approves them, those remain local or in controlled evidence
storage. The public repository contains source, recipes, manifests, hashes,
small uncompressed audio test data, and tests only.

## Decision to implement

- The mandatory cross-platform audio baseline is **AAC-LC, 48 kHz, stereo,
  192 kb/s**. `320_kbps` is a separately reported *optional capability probe*,
  never a silent fallback and never a slice gate.
- Keep H.264/AAC encoded bytes platform-specific. Each accepted local artifact
  has its own byte digest and structure manifest; cross-platform correctness
  is judged by canonical decoded semantics, timestamp structure, and declared
  metadata.
- The fixture/oracle tools are a standalone test subsystem. They may call
  platform decoders, but share neither the editor's media-I/O code nor its
  demux/mux implementation.

Microsoft's inbox AAC MFT accepts 48 kHz stereo and only 12,000, 16,000,
20,000, or 24,000 encoded bytes/second; 24,000 is 192 kb/s. It also requires
valid input timestamps/durations and emits one AAC frame per 1,024 PCM samples.
This makes 192 kb/s the honest mandatory Windows baseline.
[Microsoft AAC encoder](https://learn.microsoft.com/en-us/windows/win32/medfound/aac-encoder)

**Contract correction applied:** the vertical-slice contract now requires
192 kb/s for `QHD-final` and `3K-final` and keeps 320 kb/s as an explicit
optional probe. Fixture code must reject any attempt to relabel the optional
profile as the mandatory baseline.

## Tool ownership and command boundary

Create three independent command-line tools under the T-0008-owned roots.
They communicate only through versioned files; importing an editor crate is a
test failure.

| Tool | Owns | Must not own |
| --- | --- | --- |
| `tools/fixtures/fixturegen` | deterministic raw video/audio, recipe validation, WAV oracle, encoder-input stream, manifest skeleton | container muxing, editor state, benchmark judgment |
| `tools/oracles/oracle-lock` | lock-file validation, ISO-BMFF/AVC/AAC structural scan, verifier-owned decode, metadata and decoded-semantic checks, deliberate-fault tests | editor demux/mux or renderer implementation |
| `tools/benchmark-harness` | run scheduling, capability disclosures, raw repetitions, trace emission, failure retention, regression calculation, disclosure-bundle schema | fixture truth or oracle tolerances |

Use `fixturegen v1` and `oracle-lock v1` as versioned executable interfaces,
with `--version-json` that includes source-tree digest, compiler identity,
dependency-lock digest, schema version, and executable digest. The runner must
record those values in every result; no mutable `latest` tool identity is
permitted.

## Deterministic raw truth

The generator is the sole source of fixture truth. It uses no wall clock,
random-device, host floating-point math, codec output, or platform color
conversion.

1. Represent every time as normalized signed `i128 { numerator, denominator }`
   and every audio position as a `u64` sample index at 48,000 Hz. Generate
   absolute frame PTS and duration once; adapters may not accumulate rounded
   durations.
2. Generate a canonical full-range 8-bit 4:2:0 Rec.709 frame with integer,
   specified rounding at every step. Frame-number text, color bars, gradients,
   moving-detail tiles, hard cuts, dissolve boundaries, and marker flashes are
   all functions of `(recipe_digest, frame_index)` using a specified integer
   PRNG. Do not use a font rasterizer for frame numbers; use a checked-in
   bitmap glyph table.
3. Generate signed 32-bit PCM stereo with integer recurrence/table-based
   waveforms, clicks, chirps, and one-sample impulses. Do not call libc math
   or platform DSP. Write a companion 48 kHz WAV and its per-block hash table.
4. Hash each generated video frame and each 4,096-sample audio block with the
   locked hash algorithm; hash the canonical ordered `(PTS, duration, hash)`
   ledger to make stale-generation and wrong-order faults detectable.

Use the following exact development timing recipes:

| Fixture | Video timing | Audio | Marker requirement |
| --- | --- | --- | --- |
| `QHD-I` | 2560x1440, 900 frames, `PTS=n/30`, duration `1/30`; all sync samples | 1,440,000 samples, AAC-LC request 192 kb/s plus WAV | flash/chirp/impulse at start, 10 s, 20 s, end-guard |
| `QHD-LGOP` | 2560x1440, 1,800 frames, `PTS=n/60`, duration `1/60`; requested closed GOP 60, actual structure recorded | same | markers on GOP boundary and one non-key-frame point |
| `3K-I` | 3072x1728, 900 frames, `PTS=n/30`, duration `1/30`; all sync samples | same | same as `QHD-I` |
| `3K-LGOP` | optional capability-only recipe; declare codec/profile/bit depth separately | same | same timeline rules |
| `VFR-AV` | 2560x1440; repeat durations `[1/24, 1/30]` 400 times, giving exactly 30 s and 800 frames | same | a flash/chirp/one-sample impulse at each selected VFR boundary |

The ten-minute release set is exactly the same recipe with a duration multiplier
of 20 and an explicit new recipe digest; it is not a separately hand-authored
asset. The required video encoder input is generated frame-by-frame and
streamed, never materialized as a giant raw-file fixture.

### Required recipe and artifact manifests

Write canonical JSON (sorted UTF-8 keys, no insignificant whitespace) and
hash the bytes. A `fixture-recipe/v1` must include at least:

```text
schema_version, fixture_id, duration_rational, generator_id, generator_digest,
seed, video {width, height, pixel_format, matrix, transfer, primaries, range,
frame_timeline[]}, audio {sample_rate, channels, pcm_format, block_frames,
sample_count}, markers[], source_ledger_digest, wav_digest, license {id, text_digest}
```

An `encoded-artifact/v1` additionally records:

```text
recipe_digest, artifact_sha256, artifact_size, generated_at_utc,
encode_request {video_codec, requested_profile, requested_level, bitrate_bps,
gop, closed_gop, all_idr, audio_codec, sample_rate, channels, bitrate_bps},
encode_result {actual_profile, actual_level, avcc_sha256, sps_sha256,
pps_sha256, aac_asc_sha256, encoder_delay_samples, encoder_padding_samples,
codec_path, platform_capability_digest},
mp4 {ftyp, moov_before_mdat, movie_timescale, tracks[], box_tree_digest},
sample_ledger {video[], audio[]}, decoded_oracle_digest
```

Every video sample ledger entry needs `decode_index`, `presentation_index`,
`dts`, `pts`, `duration`, `sync`, byte offset/length, and NAL-structure digest.
Every audio entry needs `sample_index`, `pts`, `duration`, byte offset/length,
and decoded PCM range. `tracks[]` pins `mdhd` timescale, `stsd` sample entry,
`avcC`/AudioSpecificConfig digest, `stts`, `ctts`, `stsc`, `stsz`, chunk offsets,
sync table, edit list, and color boxes when present. Absence is an explicit
value, never omitted ambiguity.

## Native capability records

Generation must first emit a `capability-result/v1`. Its status is only
`supported`, `unsupported`, or `error`; `unsupported` carries the rejected
configuration and native error, and must never produce an alternate asset.

### macOS

- Create the requested VideoToolbox session, set the requested profile, level,
  bitrate, GOP/keyframe policy, and color attachments, then record the accepted
  session properties. Query `EncoderID`,
  `UsingHardwareAcceleratedVideoEncoder`, and `UsingGPURegistryID` when
  available, together with OS build, hardware model, and GPU registry identity.
  Apple documents the hardware-selection property as a post-creation query.
  [VideoToolbox hardware encoder property](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_usinghardwareacceleratedvideoencoder)
- Record the returned format description/parameter-set bytes and independently
  scan the resulting `avcC`/SPS/PPS. The actual profile/level is evidence; it
  is not inferred from the request.
- For AAC, record input and output ASBD, magic-cookie digest, converter ID,
  leading/trailing priming frames, packet count, and output duration. Apple
  exposes priming information specifically because converters can require
  leading or trailing frames.
  [AudioConverter priming](https://developer.apple.com/documentation/audiotoolbox/audioconverterprimeinfo)

### Windows

- Enumerate candidate Media Foundation transforms with hardware and software
  paths separately. Record MFT friendly name, CLSID, activation attributes, and
  `MFT_ENUM_HARDWARE_URL_Attribute` when present; its presence identifies a
  hardware device transform. Do not label a path hardware merely because it
  uses a GPU-assisted software decoder.
  [MFT enumeration flags](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/ne-mfapi-_mft_enum_flag)
  · [hardware MFT identity](https://learn.microsoft.com/en-us/windows/win32/medfound/mft-enum-hardware-url-attribute)
- Set and record exact input/output media types, `ICodecAPI` values, accepted
  sequence header, selected rate-control mode, requested/actual GOP, and sample
  timestamps. The H.264 MFT documents its profile, rate-control, GOP, and
  force-keyframe controls, and notes that certified hardware encoders can
  replace the inbox encoder.
  [Microsoft H.264 encoder](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder)
- For AAC, record `MF_MT_USER_DATA` digest, profile-level indication, payload
  type, output frame count, actual bytes/second, and every input sample's
  nonzero timestamp/duration. Confirm the 1,024-samples-per-AAC-frame rule in
  the post-encode scan.

## Independent oracle lock

`oracle-lock/v1` is checked in and references immutable identities, not just
tool names:

```text
fixturegen {version, executable_digest, source_tree_digest},
cpu_reference {version, executable_digest, source_tree_digest},
bmff_avc_aac_scanner {version, executable_digest, source_tree_digest},
decoder_runner {version, executable_digest, source_tree_digest, backend},
metadata_checker {version, executable_digest, source_tree_digest},
tolerance_set {version, canonical_json_digest}, schema_digests[]
```

The CPU reference must re-evaluate the declared integer generator and canonical
project recipe from manifest input, not deserialize editor render-plan output.
The verifier first uses its own bounded ISO-BMFF/AVC/AAC parser to validate the
whitelisted MP4 structure and build a sample table. It then feeds those samples
to a verifier-owned OS decoder path, converts decoded output to the locked
canonical representation, and compares it with the CPU reference. It must not
link the editor's demuxer, muxer, render graph, or project crate.

The metadata checker verifies all declared color fields, sample entry/config,
dimensions, profile/level, timescales, fast-start box order, delay/padding,
sample/frame counts, and strictly monotonic DTS/PTS after applying the declared
edit list. It rejects unknown or duplicate critical boxes in the supported
fixture profile instead of accepting a loosely similar file.

Predeclare tolerance values *before* a candidate run. Integer source-ledger,
timestamp, metadata, marker position, and audio sample-count checks are exact.
Lossy decoded video uses named per-channel maximum error plus RMSE/PSNR on both
whole-frame and marker ROIs. Audio uses predeclared channel-wise RMS/peak,
windowed correlation, and marker-centroid bounds after delay/padding removal.
The first accepted baseline records the tolerance-set digest; changing it is a
new oracle-lock version, never a benchmark-side adjustment.

The lock test suite must inject and retain proof for: one wrong frame, a stale
generation ID, a one-tick PTS shift, non-monotonic PTS, wrong frame/sample
count, wrong color range/primaries, one-sample audio shift, fade-coefficient
change, malformed/relocated MP4 metadata, and a decoder/muxer identity change.
Each fault maps to the named oracle which rejects it.

## Perfetto-compatible telemetry

Emit two forms from the same event records:

1. `trace-events.json`: the Chrome Trace Event JSON subset (`B`, `E`, `X`, `C`,
   flow start/step/end, metadata), which Perfetto imports; and
2. `trace.ndjson`: one canonical event per line for lossless raw retention and
   schema validation.

Perfetto documents track events as monotonically ordered timeline events with
slices, counters, and flows; its importer also accepts Chrome Trace Event JSON.
[Perfetto track events](https://perfetto.dev/docs/instrumentation/track-events)
· [Perfetto external formats](https://perfetto.dev/docs/getting-started/other-formats)

Every event has `run_id`, `repetition_id`, `condition_id`, `phase` (`cold` or
`warm`), `fixture_digest`, `project_digest`, `revision_id`,
`semantic_plan_digest`, `execution_plan_digest`, `semantic_mix_digest`,
`generation`, monotonic `ts_ns`, and a stable process/thread/track identity.
Use these categories and required arguments:

| Category | Slice/counter | Required arguments |
| --- | --- | --- |
| `fixture` | generation, encode, decode | fixture/recipe/artifact digests, native path |
| `scheduler` | request, cancellation, stale discard | revision, generation, queue, reason |
| `queue` | per-queue counters | item limit/value, byte limit/value, producer action |
| `resource` | token and lease counters | kind, owned/limit, allocation/release reason |
| `adapter` | timestamp conversion | canonical rational, target ticks, rounding policy, exact error rational |
| `render` / `audio` | preview/export/mix spans | plan/mix digest, range, backend, proxy/original |
| `codec` / `mux` | native operation spans | codec config, hardware/software path, error code |
| `memory` | process and editor-owned counters | RSS, private bytes, GPU/unified observed, cache/decoder/queue bytes |
| `oracle` | compare/check/failure spans | oracle-lock digest, metric, threshold, observed result |

Trace flows connect one request/generation across decode, render, encode, mux,
and oracle work. Counter samples carry declared units. The event writer must
flush an incomplete trace plus a structured failure record on timeout, panic,
or oracle failure.

## Reproducible cold/warm schedule and regression judgment

Use a checked-in `benchmark-plan/v1` with a seed and at least ten valid paired
repetitions for each `(fixture, scenario, metric, cache_state)` comparison.
Each pair contains baseline and candidate with an independently seeded,
randomized order; pairs are randomly interleaved across fixtures/scenarios via
the plan's documented Fisher-Yates permutation. Record the permutation and do
not reshuffle failed attempts.

- A **cold** run launches a fresh process and an empty, run-owned editor cache
  directory. It may not claim to clear global OS file caches; record that fact,
  fixture volume, and the observed cache state instead.
- A **warm** run executes a declared warm-up, proves the expected app-owned
  cache entries exist, then measures in the same process/cache profile. Warm-up
  samples are not repetitions.
- A run that cannot produce a valid disclosure bundle is retained as a failed
  repetition with reason, logs, partial traces, and resource observations. It
  is not silently retried or discarded. Environmental invalidation (for example,
  a machine reboot) creates an explicit invalid result and starts a new plan;
  it does not replace a retained sample.
- Reject a comparison if fixture/project/oracle-lock/toolchain/platform
  identities differ between paired conditions. Preserve the raw samples even
  when comparison is rejected.

For lower-is-better measurements, calculate each paired ratio as
`candidate / baseline`; for higher-is-better measurements, use
`baseline / candidate`. Report raw medians, median paired ratio, all ratios,
valid-count, and failures. A same-machine regression is a correctness failure
or a median paired ratio greater than `1.05` with at least ten valid pairs.
This is a release gate, not a statistical significance claim. Hardware and
software codec paths remain separate series and are never pooled.

Every disclosure bundle must include the fixture/oracle/tool identities, recipe
and artifact manifests, platform capability record, machine/OS/driver/storage/
display/audio metadata, benchmark plan/seed, raw repetitions, failures, traces,
render manifest, semantic/execution/mix digests, queue/resource maxima, process
memory observations, and the derived regression report.

## Implementation sequence

1. Add canonical schema fixtures and pure generator/reference tests first;
   regenerate the development ledger twice and require byte-identical source
   manifests/WAV/ledgers.
2. Add the standalone structural scanner and metadata checker, then deliberate
   fault tests before connecting a native decoder.
3. Add native capability records and local-only encoding adapters; require
   exact disclosure of selected codec path and reject unavailable profiles.
4. Add verifier-owned decode comparison, trace writer, and disclosure validator.
5. Add the seeded paired runner and only then publish a local baseline evidence
   bundle. No generated H.264/AAC binary enters the public repository unless the
   legal gate is later approved.
