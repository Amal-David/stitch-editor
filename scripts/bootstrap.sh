#!/usr/bin/env bash
set -euo pipefail

readonly required_rust="1.97.0"
readonly required_cmake="3.31.6"
readonly required_qt="6.11.1"
readonly evidence_dir="build/bootstrap-qt611"

# Rustup proxies must not install a toolchain/components during this command.
export RUSTUP_AUTO_INSTALL=0

phase="${1:-all}"

fail() {
  printf 'bootstrap: %s\n' "$*" >&2
  exit 1
}

require_exact_version() {
  local tool="$1"
  local expected="$2"
  local actual="$3"

  [[ "$actual" == "$expected" ]] || fail "$tool $expected is required; found $actual. Install the pinned toolchain before retrying."
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required but unavailable. Provision the pinned toolchain before running the offline bootstrap."
}

major_version() {
  printf '%s\n' "${1%%.*}"
}

write_platform_evidence() {
  require_command cmake
  mkdir -p "$evidence_dir"

  {
    printf 'rustc=%s\n' "$(rustc --version 2>/dev/null || printf unavailable)"
    printf 'cargo=%s\n' "$(cargo --version 2>/dev/null || printf unavailable)"
    printf 'cmake=%s\n' "$(cmake --version | awk 'NR == 1')"
    printf 'qt_root_dir=%s\n' "${QT_ROOT_DIR:-unavailable}"
    printf 'qt6_dir_input=%s\n' "${Qt6_DIR:-unavailable}"

    if [[ "$(uname -s)" == "Darwin" ]]; then
      require_command xcodebuild
      require_command xcrun
      local xcode sdk
      xcode="$(xcodebuild -version | awk '/^Xcode / {print $2}')"
      sdk="$(xcrun --sdk macosx --show-sdk-version)"
      [[ "$(major_version "$xcode")" -ge 15 ]] || fail "Xcode 15+ is required; found $xcode."
      [[ "$(major_version "$sdk")" -ge 14 ]] || fail "macOS SDK 14+ is required; found $sdk."
      printf 'platform=macos\n'
      printf 'xcode=%s\n' "$xcode"
      printf 'macos_sdk=%s\n' "$sdk"
    elif [[ "${OS:-}" == "Windows_NT" ]]; then
      require_command vswhere.exe
      local visual_studio windows_sdk
      visual_studio="$(vswhere.exe -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationVersion | tr -d '\r')"
      windows_sdk="$(printf '%s' "${WindowsSDKVersion:-}" | tr -d '\r\\')"
      [[ "$(major_version "$visual_studio")" -eq 17 ]] || fail "MSVC 2022 (Visual Studio 17.x) is required; found ${visual_studio:-unavailable}."
      [[ "$windows_sdk" == "10.0.26100.0" ]] || fail "Windows SDK 10.0.26100.0 is required; found ${windows_sdk:-unavailable}."
      printf 'platform=windows\n'
      printf 'visual_studio=%s\n' "$visual_studio"
      printf 'windows_sdk=%s\n' "$windows_sdk"
    else
      fail "unsupported host $(uname -s); only macOS and Windows are supported."
    fi
  } >"$evidence_dir/toolchain-evidence.env"
}

resolve_qt6_dir() {
  if [[ -n "${Qt6_DIR:-}" && -f "${Qt6_DIR}/Qt6Config.cmake" ]]; then
    export Qt6_DIR
    return
  fi

  if [[ -n "${QT_ROOT_DIR:-}" ]]; then
    local candidate
    local candidates=()
    shopt -s nullglob
    candidates=(
      "$QT_ROOT_DIR/lib/cmake/Qt6"
      "$QT_ROOT_DIR/$required_qt"/*/lib/cmake/Qt6
      "$QT_ROOT_DIR/Qt/$required_qt"/*/lib/cmake/Qt6
    )
    shopt -u nullglob
    for candidate in "${candidates[@]}"; do
      if [[ -f "$candidate/Qt6Config.cmake" ]]; then
        export Qt6_DIR="$candidate"
        return
      fi
    done
  fi

  fail "Qt $required_qt is not configured. This bootstrap never downloads Qt; set Qt6_DIR to lib/cmake/Qt6 or provision QT_ROOT_DIR with the pinned Qt kit."
}

append_cmake_evidence() {
  local cache="$evidence_dir/CMakeCache.txt"
  [[ -f "$cache" ]] || fail "CMake did not produce $cache."

  local compiler generator backend backend_definition compiler_id compiler_version compiler_state
  local compiler_states=()
  shopt -s nullglob
  compiler_states=("$evidence_dir"/CMakeFiles/*/CMakeCXXCompiler.cmake)
  shopt -u nullglob
  [[ "${#compiler_states[@]}" -eq 1 ]] || fail "CMake did not produce exactly one C++ compiler state file."
  compiler_state="${compiler_states[0]}"
  compiler="$(awk -F'"' '/^set\(CMAKE_CXX_COMPILER "/ {print $2; exit}' "$compiler_state")"
  compiler_id="$(awk -F'"' '/^set\(CMAKE_CXX_COMPILER_ID "/ {print $2; exit}' "$compiler_state")"
  compiler_version="$(awk -F'"' '/^set\(CMAKE_CXX_COMPILER_VERSION "/ {print $2; exit}' "$compiler_state")"
  generator="$(awk -F= '/^CMAKE_GENERATOR:INTERNAL=/ {print $2; exit}' "$cache")"
  [[ -n "$compiler" ]] || fail "CMake compiler state did not record the C++ compiler."
  [[ -n "$compiler_id" ]] || fail "CMake compiler state did not record the C++ compiler ID."
  [[ -n "$compiler_version" ]] || fail "CMake compiler state did not record the C++ compiler version."
  [[ -n "$generator" ]] || fail "CMake cache did not record the generator."

  if [[ "$(uname -s)" == "Darwin" ]]; then
    backend="Metal"
    backend_definition="STITCH_EXPECT_METAL=1"
  else
    backend="D3D11"
    backend_definition="STITCH_EXPECT_D3D11=1"
  fi

  {
    printf 'cmake_cxx_compiler=%s\n' "$compiler"
    printf 'cmake_cxx_compiler_id=%s\n' "$compiler_id"
    printf 'cmake_cxx_compiler_version=%s\n' "$compiler_version"
    printf 'cmake_generator=%s\n' "$generator"
    printf 'required_qt_backend=%s\n' "$backend"
    printf 'backend_compile_definition=%s\n' "$backend_definition"
  } >>"$evidence_dir/toolchain-evidence.env"
}

run_policy() {
  ./scripts/policy.sh
}

run_rust() {
  require_command rustc
  require_command cargo
  require_command rustfmt
  require_command clippy-driver
  require_exact_version "rustc" "$required_rust" "$(rustc --version | awk '{print $2}')"
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --locked --offline -- -D warnings
  cargo test --workspace --locked --offline
}

run_qt() {
  require_command cmake
  require_exact_version "CMake" "$required_cmake" "$(cmake --version | awk 'NR == 1 {print $3}')"
  write_platform_evidence
  resolve_qt6_dir
  printf 'qt6_dir=%s\n' "$Qt6_DIR" >>"$evidence_dir/toolchain-evidence.env"
  local cmake_args=(--preset bootstrap)
  if [[ -n "${CMAKE_PREFIX_PATH:-}" ]]; then
    local prefix
    local prefixes=()
    local original_ifs="$IFS"
    local prefix_separator=":"
    [[ "${OS:-}" == "Windows_NT" ]] && prefix_separator=";"
    IFS="$prefix_separator" read -r -a prefixes <<<"$CMAKE_PREFIX_PATH"
    IFS="$original_ifs"
    for prefix in "${prefixes[@]}"; do
      if [[ -f "$prefix/lib/cmake/Qt6Quick/Qt6QuickConfig.cmake" ]]; then
        cmake_args+=("-DQt6Quick_DIR=$prefix/lib/cmake/Qt6Quick")
        cmake_args+=("-DQT_ADDITIONAL_PACKAGES_PREFIX_PATH=$prefix")
      fi
    done
  fi
  cmake "${cmake_args[@]}"
  append_cmake_evidence
  require_command tee
  cmake --build --preset bootstrap --verbose 2>&1 \
    | tee "$evidence_dir/cmake-build.log"
  ctest --test-dir build/bootstrap-qt611 --build-config Debug --output-on-failure \
    --output-log "$evidence_dir/ctest.log"
}

case "$phase" in
  all)
    run_policy
    run_rust
    run_qt
    ;;
  policy)
    run_policy
    ;;
  rust)
    run_rust
    ;;
  platform)
    write_platform_evidence
    ;;
  qt)
    run_qt
    ;;
  *)
    fail "usage: ./scripts/bootstrap.sh [all|policy|rust|platform|qt]"
    ;;
esac
