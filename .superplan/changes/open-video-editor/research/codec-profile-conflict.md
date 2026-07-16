# Native codec profile and fixture-distribution conflict

Verified: 2026-07-14

## Blocking profile mismatch

The vertical-slice export profiles currently require AAC-LC stereo at 48 kHz
and 320 kb/s on every Windows and macOS acceptance machine. Microsoft's
documented inbox Media Foundation AAC encoder accepts only 12,000, 16,000,
20,000, or 24,000 encoded bytes per second for mono or stereo. The highest
documented setting is therefore 192 kb/s, not 320 kb/s.

Primary source: [Microsoft Media Foundation AAC Encoder](https://learn.microsoft.com/en-us/windows/win32/medfound/aac-encoder).

The H.264 inbox encoder exposes the required input formats, profiles, GOP, rate
control, and force-keyframe controls, but Microsoft also documents that a
certified hardware encoder will generally replace it when present. Encoded-byte
identity is consequently not a portable oracle; the exact encoder and actual
output structure must be recorded and verified.

Primary source: [Microsoft Media Foundation H.264 Video Encoder](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder).

## Required decision

1. Make AAC-LC 192 kb/s the mandatory cross-platform baseline and retain
   320 kb/s as a probed capability. This preserves the system-codec/no-bundled-
   codec architecture and is the recommended path.
2. Bundle or separately obtain another AAC encoder. This reopens the dependency,
   distribution, maintenance, and licensing decisions and contradicts the
   current no-bundled-codec baseline.
3. Change the mandatory audio codec/container promise. This is a larger product
   compatibility change and requires a new format decision.

Until one path is selected, the exact baseline fixtures and export profile
cannot be implemented honestly for Windows.

## Distribution boundary

Using system encoders avoids redistributing codec binaries, but it does not by
itself grant all rights to distribute generated H.264 material. Both operating
system notices limit the included AVC portfolio license to stated personal and
non-commercial cases and say other use may require separate licensing.

- [Microsoft Product Terms H.264/AVC notice](https://www.microsoft.com/licensing/terms/en-US/product/Notices/OVSES)
- [macOS Tahoe software license agreement](https://www.apple.com/legal/sla/docs/macOSTahoe.pdf)

This is an engineering risk record, not legal advice. The safe repository
boundary remains generator source, recipes, manifests, semantic hashes, and
small uncompressed test data. Generated H.264/AAC fixtures stay local or in
controlled evidence storage until the dated legal gate approves distribution.

## Architecture retained regardless of bitrate decision

- A portable deterministic streaming generator owns raw video/audio truth,
  exact rational timestamps, markers, and incremental semantic hashes.
- Thin AVFoundation/VideoToolbox/AudioConverter and Media Foundation adapters
  consume that stream and report typed capability results; no codec binary is
  bundled.
- A separate verifier parses ISO-BMFF/H.264 structure, decodes through an
  independent code path, and compares timing, color, frames, samples, markers,
  priming, and padding against the raw oracle.
- Each generated artifact records its own digest and platform/encoder identity.
  Cross-platform acceptance compares decoded semantics and structure, not
  encoded bytes.
