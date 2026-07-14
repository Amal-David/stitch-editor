# Tooling surface classification

Repository-local instruction/skill/config surfaces are durable and trackable:
`.agents/`, `.amazonq/rules/`, `.claude/skills/`, `.codex/skills/`,
`.cursor/skills/`, `.gemini/commands/`, `.opencode/skills/`, and
`.superplan/changes/`, `.superplan/context/`, `.superplan/skills/`, and
`.superplan` configuration.

Local runtime, cache, session, and generated control state is ignored by
`.gitignore`: each tool's cache/session/state directories and
`.superplan/runtime/`. Do not broaden an ignore rule to hide durable project
instructions or specifications. New tool integrations must add their durable
configuration to this document and their mutable runtime paths to `.gitignore`.
