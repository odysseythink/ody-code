#!/usr/bin/env bash
# Build the Rust -> Wasm PoC and run the JS-vs-Wasm benchmark.
set -euo pipefail
cd "$(dirname "$0")"

echo "==> rustc: $(rustc --version)"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

echo "==> native unit tests"
cargo test --quiet

echo "==> building wasm (release)"
cargo build -p ody-rust --release --target wasm32-unknown-unknown
ls -la target/wasm32-unknown-unknown/release/ody_rust.wasm

# The repo pins node >=24.15 via engines; pnpm refuses lower. Pick a 24.x if the
# active node is older, so the benchmark can run without changing global state.
NODE_BIN="node"
if ! node --version | grep -qE 'v(2[4-9]|[3-9][0-9])\.'; then
  CANDIDATE=$(ls -d "$HOME"/.nvm/versions/node/v24.* 2>/dev/null | sort -V | tail -1 || true)
  [ -n "${CANDIDATE:-}" ] && NODE_BIN="$CANDIDATE/bin/node"
fi
echo "==> running benchmark with: $($NODE_BIN --version)"
"$NODE_BIN" ./node_modules/.bin/tsx ts/bench.ts 2>/dev/null \
  || (cd .. && PATH="$(dirname "$NODE_BIN"):$PATH" ./node_modules/.bin/tsx rust-ody/ts/bench.ts)
