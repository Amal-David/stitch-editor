# Contributing

## Branches and commits

- `main` is the protected integration branch. Create a focused branch from it;
  do not commit directly to `main`.
- Keep the configured human Git author identity. Do not replace `user.name` or
  `user.email` with an agent identity.
- A commit materially authored by Codex ends with
  `Co-authored-by: Codex <noreply@openai.com>`. Preserve trailers for every
  material contributor.
- Keep generated media, build output, credentials, and local tool state out of
  commits. Version fixture recipes, manifests, expected hashes, and source
  assets that are small enough to audit instead.

## Bootstrap checks

Run `./scripts/bootstrap.sh` on a provisioned macOS or Windows machine. The
entrypoint is offline by default and never installs Rust, Qt, CMake, codecs, or
system packages. See `docs/toolchains.md` and `docs/dependency-policy.md` for
the pinned inputs and review process.

## Changes to shared contracts

The directory owner named in `docs/directory-ownership.md` reviews changes to
that directory. Do not add a competing workspace root, build entrypoint, media
stack, or plugin ABI without an approved architecture decision.
