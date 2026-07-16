#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'policy: %s\n' "$*" >&2
  exit 1
}

for required in LICENSE NOTICE.md CONTRIBUTING.md SECURITY.md LEGAL_GATE.md docs/dependency-policy.md docs/toolchains.md; do
  [[ -f "$required" ]] || fail "required bootstrap file is missing: $required"
done

grep -Fq "GNU GENERAL PUBLIC LICENSE" LICENSE || fail "LICENSE is not GPL-3.0 text"
grep -Fq "Qt 6.11.1" docs/toolchains.md || fail "Qt 6.11.1 is not pinned"
grep -Fq "not completed legal advice" LEGAL_GATE.md || fail "legal gate must remain explicitly incomplete"

forbidden='ffmpeg|nodeav|libavcodec|libavformat|bundled[[:space:]-]*codec'
if command -v rg >/dev/null 2>&1; then
  if rg -n -i \
    --glob 'Cargo.toml' --glob 'Cargo.lock' --glob 'CMakeLists.txt' --glob '*.cmake' \
    --glob 'vcpkg.json' --glob 'conanfile.*' --glob 'package.json' --glob 'pnpm-lock.yaml' \
    "$forbidden" .; then
    fail "forbidden media dependency found in a build manifest"
  fi

  if rg -n 'FetchContent|ExternalProject_Add|file\(DOWNLOAD' --glob 'CMakeLists.txt' --glob '*.cmake' .; then
    fail "CMake download mechanism found; bootstrap must remain offline"
  fi

  if rg -n --glob 'Cargo.toml' '^\s*(git|path)\s*=' .; then
    fail "unreviewed Cargo source override found"
  fi

  if rg -n --pcre2 '^\s*(?:-\s*)?uses:\s+[^@]+@(?!(?:[0-9a-f]{40})(?:\s|$))' .github/workflows; then
    fail "GitHub Actions must be pinned to immutable 40-character commit SHAs"
  fi

  if rg -n 'steps\.qt\.outputs\.dir' .github/workflows; then
    fail "Qt action output dir is not a supported bootstrap contract; use QT_ROOT_DIR discovery"
  fi
else
  command -v git >/dev/null 2>&1 || fail "policy scanning requires either rg or git"

  if git grep -n -I -i -E "$forbidden" -- \
    ':(glob)**/Cargo.toml' ':(glob)**/Cargo.lock' \
    ':(glob)**/CMakeLists.txt' ':(glob)**/*.cmake' \
    ':(glob)**/vcpkg.json' ':(glob)**/conanfile.*' \
    ':(glob)**/package.json' ':(glob)**/pnpm-lock.yaml'; then
    fail "forbidden media dependency found in a build manifest"
  fi

  if git grep -n -I -E 'FetchContent|ExternalProject_Add|file\(DOWNLOAD' -- \
    ':(glob)**/CMakeLists.txt' ':(glob)**/*.cmake'; then
    fail "CMake download mechanism found; bootstrap must remain offline"
  fi

  if git grep -n -I -E '^[[:space:]]*(git|path)[[:space:]]*=' -- \
    ':(glob)**/Cargo.toml'; then
    fail "unreviewed Cargo source override found"
  fi

  if git grep -n -I -E '^[[:space:]]*(-[[:space:]]*)?uses:[[:space:]]+[^@]+@' -- \
    ':(glob).github/workflows/**' |
    awk '
      {
        ref = $0
        sub(/^.*@/, "", ref)
        sub(/[[:space:]#].*$/, "", ref)
        if (length(ref) != 40 || ref !~ /^[0-9a-f]+$/) {
          print $0
          found = 1
        }
      }
      END { exit found ? 0 : 1 }
    '; then
    fail "GitHub Actions must be pinned to immutable 40-character commit SHAs"
  fi

  if git grep -n -I -E 'steps\.qt\.outputs\.dir' -- \
    ':(glob).github/workflows/**'; then
    fail "Qt action output dir is not a supported bootstrap contract; use QT_ROOT_DIR discovery"
  fi
fi

grep -Fq 'compiler: apple-clang' .github/workflows/bootstrap.yml || fail "macOS CI compiler contract is missing"
grep -Fq 'compiler: msvc' .github/workflows/bootstrap.yml || fail "Windows MSVC compiler contract is missing"
grep -Fq 'vcvarsall: true' .github/workflows/bootstrap.yml || fail "Windows vcvarsall activation is missing"

printf 'policy: bootstrap dependency and license checks passed\n'
