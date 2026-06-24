# rust-ody — Rust→Wasm→(SEA) PoC

Validates the chain **Rust hot-path logic → `wasm32-unknown-unknown` → loaded by
TS → embeddable in a Node SEA single binary**, using the smallest, lowest-risk
target in the backend: `estimateTokens` from
`packages/agent-core/src/utils/tokens.ts`.

Chosen because it is a pure function (`string → number`), has zero runtime
dependencies, touches no I/O, is a real hot path (context management, compaction,
every turn), and is trivial to verify for correctness.

## Layout

- `src/lib.rs` — Rust reimplementation. No `wasm-bindgen`; a raw ABI
  (`alloc` / `dealloc` / `estimate_tokens(ptr,len)`) makes the JS↔Wasm boundary
  cost explicit. Includes native unit tests pinning it to the TS heuristic.
- `ts/wasm-tokens.ts` — dual-track loader. Returns a function with the same
  signature as the JS `estimateTokens`; callers fall back to JS if the Wasm
  fails to load. In a SEA build the only change is sourcing the bytes from
  `sea.getAsset('ody_rust.wasm')` instead of `readFile`.
- `ts/bench.ts` — correctness check (Wasm must equal JS) + perf across sizes.
- `build.sh` — one command: test → build wasm → benchmark.

## Run

```bash
./rust-ody/build.sh
```

## Results (Apple Silicon, Node 24, release wasm ~17KB)

Correctness: **all inputs match JS exactly** (ASCII, CJK, emoji, edge cases).

| input size | JS | Wasm | verdict |
|---|---|---|---|
| tiny (12 B)  | 33 ns   | 174 ns  | Wasm **5.3× slower** |
| small (200 B)| 872 ns  | 720 ns  | Wasm 1.2× faster |
| medium (4 KB)| 17.9 µs | 11.7 µs | Wasm 1.5× faster |
| large (64 KB)| 286 µs  | 183 µs  | Wasm 1.6× faster |
| huge (512 KB)| 2.27 ms | 1.47 ms | Wasm 1.5× faster |

## Takeaways

1. **The chain works**: Rust→Wasm→TS, identical results, ~17 KB platform-neutral
   artifact — one `.wasm` serves every SEA target platform.
2. **The boundary tax is real**: on tiny inputs the UTF-8 copy into linear memory
   dwarfs the compute, so Wasm loses badly. Crossover is in the low-hundreds of
   bytes; the win plateaus around **1.5×** for large inputs.
3. **Implication for adoption**: only move a function to Wasm when it does heavy
   compute per call on sizable data. A ~1.5× win on token estimation is unlikely
   to matter end-to-end (the agent is network-bound), so this specific function
   is a good *experiment* but a poor *production* candidate. Use the same harness
   to evaluate heavier targets (e.g. a real BPE tokenizer, large-file diffs).
