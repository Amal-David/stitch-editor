# Material-output attestation

T-0015 shared bootstrap output is accepted only from an attested implementation
worker. The authoritative record is
`.superplan/changes/open-video-editor/research/terra-high-attestation.md`, which
attests this worker as model `gpt-5.6-terra`, reasoning effort `high`, and
subagent source. Future implementation/research output must be verified against
the same metadata contract before acceptance; missing or mismatched metadata
invalidates the output rather than being inferred from a parent task.
