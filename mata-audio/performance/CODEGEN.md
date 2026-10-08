# P527-P551 compiler/code-generation matrix

Normal `release` remains portable: `opt-level=3`, ThinLTO, four codegen units, runtime SIMD dispatch, and unwind panics. The `perf*` profiles exist only for measurement. Native CPU specialization is never required by the library and is enabled only by `RUSTFLAGS=-C target-cpu=native` in the benchmark script.

The matrix exercises:
- `perf`: O3 / ThinLTO / one CGU
- `perf-size`: `opt-level=s` / ThinLTO / one CGU
- `perf-fat`: O3 / fat LTO / one CGU
- `perf-parallel`: O3 / ThinLTO / eight CGUs
- portable versus `target-cpu=native`
- unwind versus abort panic strategy through Cargo profile environment overrides

Assembly inspection emits per-profile `.s` files and searches the generated assembly for PCM conversion, FLAC Rice/LPC, MPEG Huffman/dequant/synthesis, Vorbis IMDCT and mixer/resampler symbols. Binary-size and Linux `perf stat` scripts record the specialization/code-size/instruction-cache tradeoff.

Inlining policy is intentionally narrow: tiny arithmetic/dispatch helpers stay `#[inline]`/`#[inline(always)]`; large parsing/formatting/error paths are not forced inline, and error formatting/public construction is cold + `inline(never)`. Decoder setup validates stream-derived invariants before entering hot kernels, and PCM chooses its conversion function once in `PcmDecodePlan`.
