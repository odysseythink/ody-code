# Phase 1-A Wasm Hotspot Benchmark Report

Generated: 2026-06-24T10:13:15.966Z

> Tokenizer: Wasm BPE suspended — the embedded rank data (~5 MB) exceeded the 2 MB Wasm threshold. The JS heuristic remains the default.

## Summary

- Diff (similar vs JS LCS): average speedup 0.59x
- Glob (globset+picomatch vs picomatch): average speedup 0.09x

## Details

### Diff (similar vs JS LCS)

| name | size | iterations | JS | Wasm | speedup |
|---|---:|---:|---:|---:|---:|
| small | 200 | 50,000 | 920.0 ns | 10.70 µs | 11.63x slower |
| medium | 4096 | 10,000 | 85.83 µs | 115.69 µs | 1.35x slower |
| large | 65536 | 1,000 | 19.86 ms | 20.97 ms | 1.06x slower |

### Glob (globset+picomatch vs picomatch)

| name | size | iterations | JS | Wasm | speedup |
|---|---:|---:|---:|---:|---:|
| short-match | 11 | 200,000 | 326.3 ns | 8.79 µs | 26.93x slower |
| short-no-match | 11 | 200,000 | 334.2 ns | 8.71 µs | 26.07x slower |
| long-match | 47 | 200,000 | 1.83 µs | 15.84 µs | 8.68x slower |
| brace | 6 | 200,000 | 1.48 µs | 8.17 µs | 5.52x slower |

## Recommendations

- Diff: keep Wasm if it is faster or within 20% of JS; the unified diff from `similar` is higher quality than the JS fallback.
- Glob: the conservative implementation always falls back to picomatch, so expect overhead. If average overhead exceeds 2x, disable `wasm-glob` or add a supported-pattern fast-path.