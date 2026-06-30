#!/usr/bin/env bash
set -euo pipefail

BUNDLES="${BUNDLES:-app,dmg}"
TARGET="${TARGET:-}"
SKIP_INSTALL="${SKIP_INSTALL:-0}"
SKIP_TESTS="${SKIP_TESTS:-0}"
NO_SIGN="${NO_SIGN:-0}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must be run on macOS. Use scripts/package_cli/windows.bat on Windows." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

require_command pnpm
require_command cargo
require_command rustc
require_command xcodebuild

echo "==> Packaging macOS desktop app"
echo "Repo: $REPO_ROOT"
echo "Bundles: $BUNDLES"
if [[ -n "$TARGET" ]]; then
  echo "Target: $TARGET"
fi

if [[ "$SKIP_INSTALL" != "1" ]]; then
  echo "==> Installing JS dependencies"
  pnpm install --frozen-lockfile
fi

if [[ "$SKIP_TESTS" != "1" ]]; then
  echo "==> Running Rust tests"
  (cd src-tauri && cargo test)
fi

build_args=(tauri build --ci --bundles "$BUNDLES")
if [[ -n "$TARGET" ]]; then
  build_args+=(--target "$TARGET")
fi
if [[ "$NO_SIGN" == "1" ]]; then
  build_args+=(--no-sign)
fi

echo "==> Running: pnpm ${build_args[*]}"
pnpm "${build_args[@]}"

BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle"
echo "==> Build finished"
if [[ -d "$BUNDLE_DIR" ]]; then
  echo "Artifacts under: $BUNDLE_DIR"
  find "$BUNDLE_DIR" -maxdepth 4 \( -name "*.dmg" -o -name "*.app" -o -name "*.tar.gz" -o -name "*.zip" \) -print
  echo "==> Opening artifacts folder"
  open "$BUNDLE_DIR"
fi
