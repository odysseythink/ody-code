#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

TARGET="${TARGET:-$(rustc -vV | awk '/host:/ {print $2}')}"

# Ensure napi-build can find libnode.dll on Windows (looks in LIBNODE_PATH)
if [[ "$TARGET" == *windows* ]]; then
  if [[ -z "${LIBNODE_PATH:-}" ]]; then
    for candidate in \
      "$(dirname "$(node -e 'console.log(process.execPath)')")/lib" \
      "$(npm root -g)/../../lib"; do
      if [[ -d "$candidate" ]]; then
        export LIBNODE_PATH="$candidate"
        break
      fi
    done
  fi
fi

case "$TARGET" in
  x86_64-apple-darwin)   NPM_TARGET=darwin-x64 ;;
  aarch64-apple-darwin)  NPM_TARGET=darwin-arm64 ;;
  x86_64-unknown-linux-gnu) NPM_TARGET=linux-x64 ;;
  aarch64-unknown-linux-gnu) NPM_TARGET=linux-arm64 ;;
  x86_64-pc-windows-msvc) NPM_TARGET=win32-x64 ;;
  x86_64-pc-windows-gnullvm) NPM_TARGET=win32-x64 ;;
  *) echo "unsupported rust target $TARGET"; exit 1 ;;
esac

cargo build -p ody-crypto --release --target "$TARGET"

# Try lib-prefixed name first (Unix convention), then without prefix (Windows convention)
LIB_BASENAME="target/$TARGET/release/libody_crypto"
if [[ ! -f "${LIB_BASENAME}.dll" && ! -f "${LIB_BASENAME}.dylib" && ! -f "${LIB_BASENAME}.so" ]]; then
  LIB_BASENAME="target/$TARGET/release/ody_crypto"
fi
if [[ "$TARGET" == *windows* ]]; then
  LIB_PATH="${LIB_BASENAME}.dll"
elif [[ "$TARGET" == *apple* ]]; then
  LIB_PATH="${LIB_BASENAME}.dylib"
else
  LIB_PATH="${LIB_BASENAME}.so"
fi

DEST="../packages/ody-crypto-${NPM_TARGET}/ody-crypto.node"
mkdir -p "$(dirname "$DEST")"
cp "$LIB_PATH" "$DEST"
echo "==> produced $DEST"
ls -lh "$DEST"
