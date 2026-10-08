# P1 performance suite

`decode.rs` is a harness-free benchmark executable so it can report throughput,
allocation counts/bytes, peak live allocator bytes, process RSS high-water mark,
rescale cases, thread scaling, error paths, rich-decoder construction, and
stage-focused codec workloads in one reproducible run.

Run:

```sh
cargo bench --bench decode
```

Useful filters and profiling helpers:

```sh
MATAIMAGE_BENCH_FILTER=jpeg cargo bench --bench decode
scripts/compare-bench.sh
scripts/profile-bench.sh
scripts/thread-bench.sh
scripts/asm-bench.sh
```

The synthetic corpus lives in `benches/assets/`; `MANIFEST.tsv` records byte
sizes and SHA-256 hashes so benchmark inputs can be verified exactly.
