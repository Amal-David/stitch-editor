# Lessons

## 2026-07-14: Sub-agent model selection

- Use GPT-5.6 Terra with high reasoning for every research and implementation sub-agent in this project.
- Do not infer the child model from the parent task or from whether an app-server override succeeds. Multi-agent v2 may select the required child model while rejecting direct overrides.
- Capture each `agentThreadId` and verify `model`, `reasoning_effort`, `thread_source`, and `agent_path` in the authoritative local `threads` table before accepting material output.
- For this project require exactly `gpt-5.6-terra`, `high`, and `subagent`; interrupt the worker and discard material output when metadata is missing or mismatched.
- Preserve the attestation beside the worker's research, implementation, or review evidence.
- A fresh multi-agent worker may receive the parent model even when earlier council workers were Terra High. Attest immediately after spawn, before it has time to make material changes.
- When a fresh worker fails attestation, interrupt it, verify its write surface is unchanged, and reuse an already attested Terra High sub-agent thread only with a new bounded task and exclusive write ownership.

## 2026-07-14: Native UI self-tests must not disrupt the desktop

- Do not run repeated native-window self-tests while they visibly open, activate, or resize application windows on the user's desktop.
- Keep the real GPU and scene-graph path, but make the default automated self-test non-activating and visually hidden; require an explicit opt-in flag for a visible diagnostic run.
- Before interpreting repeated launch/exit churn as a current crash, inspect process state and timestamped crash reports. Stop the repetition harness first, then distinguish historical crashes from the current build.
