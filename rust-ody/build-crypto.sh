#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

TARGET="${TARGET:-$(rustc -vV | awk '/host:/ {print $2}')}"

case "$TARGET" in
  x86_64-apple-darwin)   NPM_TARGET=darwin-x64 ;;
  aarch64-apple-darwin)  NPM_TARGET=darwin-arm64 ;;
  x86_64-unknown-linux-gnu) NPM_TARGET=linux-x64 ;;
  aarch64-unknown-linux-gnu) NPM_TARGET=linux-arm64 ;;
  x86_64-pc-windows-msvc) NPM_TARGET=win32-x64 ;;
  *) echo "unsupported rust target $TARGET"; exit 1 ;;
esac

cargo build -p ody-crypto --release --target "$TARGET"

LIB_BASENAME="target/$TARGET/release/libody_crypto"
if [[ "$TARGET" == *windows* ]]; then
  LIB_PATH="${LIB_BASENAME}.dll"
elif [[ "$TARGET" == *apple* ]]; then
  LIB_PATH="${LIB_BASENAME}.dylib"
else
  LIB_PATH="${LIB_BASENAME}.so"
fi

DEST="packages/ody-crypto-${NPM_TARGET}/ody-crypto.node"
mkdir -p "$(dirname "$DEST")"
cp "$LIB_PATH" "$DEST"
echo "==> produced $DEST"
ls -lh "$DEST"
