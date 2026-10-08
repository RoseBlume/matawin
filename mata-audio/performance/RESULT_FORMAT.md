# Machine-readable result format

`mata-audio-perf` writes CSV with these columns:

- `kind`: decode, correctness, timeline, I/O, playback, SIMD, microbenchmark, or instrumented stage.
- `id`: corpus or benchmark identifier.
- `metric`: stable metric name.
- `value`: decimal measurement or exact hexadecimal correctness hash.
- `unit`: ns, KiB, count, bytes/s, alloc/frame, x, f32, or hash.
- `compiler`: `rustc --version` captured by the benchmark process.
- `features`: all relevant enabled crate features.
- `isa`: runtime-selected scalar/SSE/AVX/NEON tier.

Rows are deliberately long-form so additional metrics do not require a schema migration. `performance/scripts/compare_results.py` applies a default 5% regression threshold and treats correctness-hash changes as failures.
