#!/usr/bin/env bash
set -euo pipefail

# Thin wrapper: parses --target <npm-target> and delegates to rust-ody/build-crypto.sh
# npm targets: darwin-arm64, darwin-x64, linux-arm64, linux-x64, win32-arm64, win32-x64

script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"

npm_target=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      npm_target="$2"
      shift 2
      ;;
    --)
      # pnpm passes -- through; skip it
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

# Map npm-style target to Rust triplet
case "${npm_target:-}" in
  darwin-arm64)  rust_target="aarch64-apple-darwin" ;;
  darwin-x64)    rust_target="x86_64-apple-darwin" ;;
  linux-arm64)   rust_target="aarch64-unknown-linux-gnu" ;;
  linux-x64)     rust_target="x86_64-unknown-linux-gnu" ;;
  win32-arm64)   rust_target="aarch64-pc-windows-msvc" ;;
  win32-x64)     rust_target="x86_64-pc-windows-gnullvm" ;;
  "")
    echo "No --target specified; will auto-detect from rustc host triple." >&2
    rust_target=""
    ;;
  *)
    echo "Unsupported npm target: $npm_target" >&2
    exit 1
    ;;
esac

if [[ -n "$rust_target" ]]; then
  TARGET="$rust_target" exec bash "$repo_root/rust-ody/build-crypto.sh"
else
  exec bash "$repo_root/rust-ody/build-crypto.sh"
fi
