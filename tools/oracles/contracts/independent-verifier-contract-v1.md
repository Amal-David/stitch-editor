# Independent Verifier Contract v1

This repository currently pins a verifier contract, not a native demux/decode
implementation. A conforming verifier must use a demux/decode/container stack
that is independent from the editor implementation, then emit `DecodedEvidence`
for the oracle crate to validate frame/audio counts, rational timing, markers,
color metadata, sample entry, delay/padding, and artifact identity.
