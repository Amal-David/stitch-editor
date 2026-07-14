# Terra High Sub-agent Attestation

- Verified: 2026-07-14
- Authority: local Codex thread state database
- Database: `/Users/amal/.codex/state_5.sqlite`
- Table: `threads`
- Required model: `gpt-5.6-terra`
- Required reasoning effort: `high`

## Evidence

The `threads` table is the authoritative local execution record for model and reasoning metadata. A read-only query selected the council thread IDs and returned:

| Agent path | Thread ID | Model | Reasoning effort | Thread source |
| --- | --- | --- | --- | --- |
| `/root/media_web_stack` | `019f5ffd-22fb-7a93-b157-ab7b1a3d5243` | `gpt-5.6-terra` | `high` | `subagent` |
| `/root/native_architecture` | `019f5ffd-507e-7e30-8c13-14dbe35c527f` | `gpt-5.6-terra` | `high` | `subagent` |
| `/root/nle_model_benchmarks` | `019f5ffd-7f23-7b31-b242-8a9be95b30ea` | `gpt-5.6-terra` | `high` | `subagent` |

The parent thread is separately recorded as `gpt-5.6-sol` with `xhigh` reasoning. Therefore, parent-thread settings are not a valid proxy for sub-agent settings. The earlier direct model override failed because multi-agent v2 does not accept app-server overrides; that failure did not mean the dispatcher had selected the wrong model.

## Dispatch Contract

For every future research or implementation sub-agent:

1. dispatch only through the bounded multi-agent runner requested by the user;
2. capture the returned `agentThreadId` from thread activity;
3. query the authoritative `threads` row for `model`, `reasoning_effort`, `thread_source`, and `agent_path` before accepting material output;
4. require exactly `gpt-5.6-terra`, `high`, and `subagent` for this project;
5. interrupt the worker and discard its material output if metadata is missing or mismatched; and
6. preserve the attestation with the worker's review or implementation evidence.

Self-reported model names, inherited-parent assumptions, prompt text, and failed override attempts are not acceptable attestation.

### Rejected implementation dispatch

The first newly spawned bootstrap implementation worker, thread `019f6022-f722-76a0-bfa7-684722f65df4` at `/root/bootstrap_repository`, was authoritatively recorded as `gpt-5.6-sol` with `xhigh` reasoning. It was interrupted immediately. A filesystem check found no Git repository and no new non-Superplan files, so none of its material output was accepted or retained. New multi-agent workers are therefore not assumed to inherit the earlier council's Terra assignment; implementation uses an already attested Terra High thread until a fresh worker can be explicitly verified.

## Council Review Round

The verified council threads are reused for a second, independent cross-review round. Each member reviews the other two reports plus ADR-001 and the vertical-slice contract, owns a separate review file, and may recommend acceptance, revision, or reopening. No council member may edit another member's report or review.

The round completed with three independent **REVISE, not reopen** verdicts:

| Verified council thread | Cross-review | Owned revision |
| --- | --- | --- |
| `/root/media_web_stack` | `review-media-web.md` | `mediabunny-web.md` |
| `/root/native_architecture` | `review-native-architecture.md` | `native-architecture.md` |
| `/root/nle_model_benchmarks` | `review-nle-performance.md` | `nle-performance.md` |

Fresh authoritative queries after the review/revision turns still record all three thread IDs as `gpt-5.6-terra`, `high`, and `subagent`. Their material objections were reconciled into ADR-001, the vertical-slice contract, the 16-task dependency graph, task contracts, program plan, decision log, and project lessons. The architecture remains selected for falsification; it is not performance-certified.
