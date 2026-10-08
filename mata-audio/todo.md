# mata-audio Performance TODO

> This roadmap is based on the current refactored `mata-audio` source. Phases are intentionally fine-grained so each can be implemented, benchmarked, and checked off independently.

> **P0–P651 implementation status:** all roadmap phase checkboxes are checked under the project owner’s implementation-complete rule. **This is not a claim of executed Cargo integration, real-time-device tests, sanitizers, profiler results or a frozen known-good baseline.** P602–P651 source/tests/harnesses are mapped in `performance/P602_P651_IMPLEMENTATION.md`; baseline promotion remains blocked by the mandatory measured gate in `performance/BASELINE_FREEZE.md`.

## Current architecture and feature layout

These items describe implemented architecture rather than benchmark phases, so they are not governed by the performance completion rule at the end of this file.

- [x] The crate is split into root `playback`, `error`, `demuxers`, `codecs`, `metadata`, and `simd` modules.
- [x] All SIMD implementations live under root `src/simd/`, including x86/x86_64, ARM/NEON, and the MPEG SIMD engine.
- [x] Codecs are individually feature gated: `codec-flac`, `codec-mp1`, `codec-mp2`, `codec-mp3`, `codec-pcm`, and `codec-vorbis`.
- [x] Demuxers are individually feature gated: `demux-aiff`, `demux-flac`, `demux-matroska`, `demux-ogg`, and `demux-wav`.
- [x] Metadata parsers are individually feature gated: `metadata-ape`, `metadata-id3v1`, `metadata-id3v2`, `metadata-iso-mp4`, `metadata-riff`, `metadata-vorbis-flac`, and `metadata-vorbis-ogg`.
- [x] `all-codecs`, `all-demuxers`, and `all-metadata` aggregate features are enabled by default.
- [x] `auto-detect` owns the full content-based `AudioDecoder` convenience API; direct codec types can be built independently without it.
- [x] FLAC and Vorbis codec features pull in only the demuxer they require (`demux-flac` and `demux-ogg` respectively).
- [x] Cargo feature-matrix, disabled-API, default-surface, compile-time, and code-size validation harnesses are implemented; execution on a Rust-enabled build host remains a separate integration step.

## Current performance observations

- `Decoder::decode_into()` is now the allocation-conscious primary API and writes into a caller-owned reusable `AudioFrameBuffer`; `next_frame()` remains the intentionally allocating compatibility wrapper.
- RIFF/WAVE and AIFF/AIFC demuxers reuse input-byte scratch, while PCM conversion writes directly into retained output storage and uses cached bytes-per-sample, block alignment, shifts, and normalization constants.
- FLAC now keeps a persistent 8 KiB buffered `BitReader` with a u64 reservoir, word-scanned unary decoding, incremental CRC-8/CRC-16, flat reusable channel/residual scratch, and no raw frame-byte capture.
- The shared MPEG Audio Layer I/II/III stream path now uses a head-indexed `ByteWindow`, geometric retained capacity, direct reads into spare capacity, and codec-specific refill watermarks instead of prefix `Vec::drain`.
- Vorbis now uses reusable O(N log N) FFT-backed IMDCT plans, retained floor/residue/time-domain arenas, multi-level codebook lookup tables, and SIMD coupling/overlap/interleave paths.
- Ogg now computes CRC incrementally and reuses page/lacing/continued-packet storage; page-local packets use borrowed slices and a bounded granule index supports Vorbis seeking.
- Metadata is kept out of decoder hot paths where practical; Vorbis comments are boxed/cold and error objects are returned rather than retained in decoder state.
- Metadata readers now share bounded seek/read helpers, preserve caller position, expose borrowed field/picture views, share Vorbis-comment iteration, and have feature-isolated benchmark/fuzz/size/compile-time harnesses.
- Seeking is a native optional `Decoder` capability: PCM containers seek exactly, FLAC/MPEG/Vorbis use learned/native indexes with bounded decode-forward to the exact requested sample, and playback retains buffers/scratch across stop, seek, and loop transitions.
- Playback now mixes packed generational voice slots, decodes expensive sources through bounded per-voice PCM SPSC producers, uses Q32 fixed-point resampling and device-period-aware adaptive block sizing, keeps the real-time callback allocation/lock/formatting free, and publishes hot source state through throttled atomics.
- Root SIMD dispatch is selected once with `OnceLock` and exposes scalar fallbacks plus x86/x86_64 SSE2/SSSE3/SSE4.1/AVX2/FMA and ARM NEON paths for the P91-P120 kernel set.
- Decoder memory accounting now reports persistent/scratch/compressed/index/metadata capacity and high-water marks; FLAC/Vorbis hot scratch is 64-byte aligned and stream-derived growth is bounded before allocation.
- Normal release remains portable/runtime-dispatched, while dedicated performance profiles/scripts cover O3/Os, Thin/Fat LTO, codegen-unit counts, optional `target-cpu=native`, assembly, binary size, instruction-cache behavior, and panic strategy.
- Offline high-throughput decoding is isolated from realtime playback and uses bounded read-ahead plus independent-stream/channel parallelism; `mmap`, `prefetch`, and `fast-math` are explicit experiments rather than default behavior.

## Rules for every phase

- Benchmark before and after; keep changes that measurably help a relevant workload or are required to unlock a measured later optimization.
- Preserve decoding correctness, streaming behavior, malformed-input handling, and scalar fallbacks.
- Keep real-time callback code allocation-free, lock-free, blocking-I/O-free, and logging/formatting-free.
- Prefer algorithmic improvements and buffer reuse before unsafe micro-optimization.
- Any unsafe/SIMD optimization must have scalar/reference equivalence tests.
- Keep memory growth bounded and avoid improving speed by retaining unbounded data.

## P0-P120 implementation pass

- [x] Source implementation and benchmark/test scaffolding for P0-P120 is present in this workspace.
- [x] The representative corpus and all reference SHA-256 hashes were verified with `performance/scripts/verify_corpus.py`.
- [x] Static validation passes for Cargo feature declarations/default closure, P0-P651 roadmap numbering, Rust delimiter consistency, corpus integrity, root-SIMD placement, and refactor invariants.
- [x] `performance/scripts/run_baseline.sh`, `validate_p0_p120.sh`, `check_feature_matrix.sh`, `profile_codecs.sh`, and `profile_playback.sh` provide reproducible validation/profiling entry points.
- [x] **P0-P120 marked complete by project-owner instruction.** The source implementation, static validation, corpus/reference checks, and benchmark/test scaffolding are present; a full Cargo integration/benchmark run may still be executed on a Rust-enabled host.

## Measurement, baselines, and regression gates

- [x] **P0 — Create a reproducible performance baseline.**
- [x] **P1 — Build a representative corpus for WAV/AIFF PCM, FLAC, MPEG Layers I/II/III, and Ogg Vorbis.**
- [x] **P2 — Add full-file decode-throughput benchmarks.**
- [x] **P3 — Add next_frame latency benchmarks.**
- [x] **P4 — Add startup/open latency benchmarks.**
- [x] **P5 — Add allocations-per-frame benchmarks.**
- [x] **P6 — Add allocated-bytes-per-decoded-second benchmarks.**
- [x] **P7 — Add peak-RSS benchmarks.**
- [x] **P8 — Add playback mixer benchmarks at 1/2/8/32/128 voices.**
- [x] **P9 — Add resampling benchmarks for equal-rate and mismatched-rate playback.**
- [x] **P10 — Add mono-to-mono playback benchmarks.**
- [x] **P11 — Add mono-to-stereo playback benchmarks.**
- [x] **P12 — Add stereo-to-stereo playback benchmarks.**
- [x] **P13 — Add stereo-to-mono playback benchmarks.**
- [x] **P14 — Add underrun stress benchmarks.**
- [x] **P15 — Add command-to-audio latency benchmarks.**
- [x] **P16 — Add seek latency benchmarks.**
- [x] **P17 — Add loop-boundary latency benchmarks.**
- [x] **P18 — Add output correctness hashes to all performance tests.**
- [x] **P19 — Add per-codec numerical tolerance checks.**
- [x] **P20 — Add performance regression thresholds.**
- [x] **P21 — Add pinned benchmark-hardware documentation.**
- [x] **P22 — Add benchmark compiler/version recording.**
- [x] **P23 — Add benchmark feature-set recording, including individual codec/demuxer/metadata features and SIMD state.**
- [x] **P24 — Add optional per-stage timing instrumentation.**
- [x] **P25 — Create a machine-readable benchmark result format.**
- [x] **P26 — Create a human-readable performance summary.**
- [x] **P27 — Add baseline flamegraphs/profiles for each enabled codec path.**
- [x] **P28 — Add baseline flamegraph/profile for playback.**
- [x] **P29 — Add a benchmark for long-duration steady-state playback.**
- [x] **P30 — Add a benchmark for many simultaneous decoder instances.**

## Core decode API and allocation architecture

- [x] **P31 — Add Decoder::decode_into for caller-provided output buffers.**
- [x] **P32 — Keep Decoder::next_frame as a compatibility wrapper.**
- [x] **P33 — Introduce a reusable AudioFrameBuffer.**
- [x] **P34 — Separate frame metadata from owned PCM storage.**
- [x] **P35 — Add borrowed decoded-frame views.**
- [x] **P36 — Add decoder maximum-frame-size hints.**
- [x] **P37 — Add decoder preferred-frame-size hints.**
- [x] **P38 — Add persistent per-decoder scratch buffers.**
- [x] **P39 — Add persistent compressed-input scratch buffers.**
- [x] **P40 — Add persistent PCM-output scratch buffers.**
- [x] **P41 — Remove hot-path iterator collect calls.**
- [x] **P42 — Replace temporary Vec creation with clear-and-reuse.**
- [x] **P43 — Use geometric buffer growth and preserve capacity.**
- [x] **P44 — Avoid shrink_to_fit in steady-state decoding.**
- [x] **P45 — Add a contiguous planar-audio scratch arena.**
- [x] **P46 — Replace Vec<Vec<T>> channel scratch where practical.**
- [x] **P47 — Add an interleaved-output writer abstraction.**
- [x] **P48 — Add a planar-output writer abstraction.**
- [x] **P49 — Add direct decoder-to-voice-buffer decoding.**
- [x] **P50 — Cache channel count in usize form.**
- [x] **P51 — Cache sample-rate reciprocal.**
- [x] **P52 — Cache bytes-per-sample.**
- [x] **P53 — Cache block alignment.**
- [x] **P54 — Cache normalization reciprocals.**
- [x] **P55 — Cache format-specific conversion function pointers.**
- [x] **P56 — Move cold metadata away from hot decoder state.**
- [x] **P57 — Move error-only data away from hot decoder state.**
- [x] **P58 — Audit all PCM copies and document them.**
- [x] **P59 — Remove at least one full PCM copy from each codec where possible.**
- [x] **P60 — Make decoder reset preserve all reusable allocations.**

## I/O, compressed buffering, and generic bit reading

- [x] **P61 — Add tuned buffered File input.**
- [x] **P62 — Create a reusable byte-window buffer with head/tail indices.**
- [x] **P63 — Remove prefix Vec::drain from compressed-input hot paths.**
- [x] **P64 — Compact compressed buffers only when a threshold is reached.**
- [x] **P65 — Read directly into spare Vec capacity where practical.**
- [x] **P66 — Tune compressed read block sizes by codec.**
- [x] **P67 — Add sequential-read microbenchmarks.**
- [x] **P68 — Add prefix-consume microbenchmarks.**
- [x] **P69 — Add compact/refill microbenchmarks.**
- [x] **P70 — Add bit-reader microbenchmarks.**
- [x] **P71 — Implement a u64 bit reservoir abstraction.**
- [x] **P72 — Implement a u128 bit reservoir experiment.**
- [x] **P73 — Batch common 8/16/24/32-bit reads.**
- [x] **P74 — Add fast byte-aligned reads.**
- [x] **P75 — Add fast unaligned reads.**
- [x] **P76 — Add fast unary-prefix scanning.**
- [x] **P77 — Use leading_zeros/trailing_zeros for unary decoding where applicable.**
- [x] **P78 — Add coarse bounds checks around bit-decode loops.**
- [x] **P79 — Add tightly scoped unchecked indexing after validation.**
- [x] **P80 — Separate I/O errors from bitstream validation hot paths.**
- [x] **P81 — Avoid stream_position calls during steady-state sequential decode.**
- [x] **P82 — Retain known stream offsets instead of querying them repeatedly.**
- [x] **P83 — Avoid unnecessary Seek operations after initialization.**
- [x] **P84 — Add lazy byte-position indexing infrastructure.**
- [x] **P85 — Add sample-position indexing infrastructure.**
- [x] **P86 — Add direct reset-to-known-offset helpers.**
- [x] **P87 — Add prefetch/refill watermarks.**
- [x] **P88 — Add EOF fast paths that avoid repeated I/O calls.**
- [x] **P89 — Add small-file in-memory fast path as an opt-in strategy.**
- [x] **P90 — Benchmark mmap-style input as an optional local-file fast path.**

## SIMD and CPU dispatch infrastructure

- [x] **P91 — Benchmark current OnceLock/function-pointer SIMD dispatch overhead.**
- [x] **P92 — Cache the selected SIMD dispatch table efficiently.**
- [x] **P93 — Add SSE2 conversion kernels.**
- [x] **P94 — Add SSSE3 packed-byte shuffle kernels.**
- [x] **P95 — Add SSE4.1 integer conversion kernels.**
- [x] **P96 — Add AVX2 PCM conversion kernels.**
- [x] **P97 — Add AVX2 mix-add kernels.**
- [x] **P98 — Add AVX2 overlap-add kernels.**
- [x] **P99 — Add AVX2 interleave kernels.**
- [x] **P100 — Add AVX2 deinterleave kernels.**
- [x] **P101 — Add FMA interpolation kernels.**
- [x] **P102 — Add FMA transform butterfly kernels.**
- [x] **P103 — Add AArch64 NEON PCM conversion kernels.**
- [x] **P104 — Add AArch64 NEON mix-add kernels.**
- [x] **P105 — Add AArch64 NEON overlap-add kernels.**
- [x] **P106 — Add AArch64 NEON interleave kernels.**
- [x] **P107 — Add ARMv7 NEON fallbacks where supported.**
- [x] **P108 — Add SIMD packed 24-bit little-endian decoding.**
- [x] **P109 — Add SIMD packed 24-bit big-endian decoding.**
- [x] **P110 — Add SIMD 32-bit integer PCM decoding.**
- [x] **P111 — Add SIMD f32 sanitize/copy.**
- [x] **P112 — Add SIMD f64-to-f32 conversion.**
- [x] **P113 — Add SIMD scalar-gain application.**
- [x] **P114 — Add SIMD final limiter/clamp.**
- [x] **P115 — Add SIMD linear interpolation.**
- [x] **P116 — Add SIMD stereo averaging.**
- [x] **P117 — Add SIMD mono duplication.**
- [x] **P118 — Add unaligned-load tests for every SIMD kernel.**
- [x] **P119 — Add scalar-vs-SIMD randomized equivalence tests.**
- [x] **P120 — Add runtime ISA-selection benchmarks.**

## P121-P216 implementation pass

- [x] WAV/AIFF PCM paths cache conversion plans, reuse byte/output storage, tune chunk sizes, and provide O(1) frame seeking.
- [x] AIFF-C `sowt` little-endian PCM and optimized 80-bit sample-rate parsing are implemented.
- [x] FLAC uses persistent buffered bit reading, incremental CRC, reusable flat scratch, specialized predictor/Rice paths, stereo restoration, SEEKTABLE seeking, and a bounded lazy frame index.
- [x] PCM bit-depth regressions, FLAC predictor/Rice corpus cases, and FLAC stage profiling hooks are included. See `performance/P121_P216_IMPLEMENTATION.md`.
- [x] P121-P216 are marked complete per project-owner instruction; Cargo integration/performance execution remains reproducible through `performance/scripts/validate_p0_p216.sh`.

## RIFF/WAVE demuxer + PCM codec

- [x] **P121 — Reuse WAV input-byte scratch between frames.**
- [x] **P122 — Decode WAV directly into reusable PCM output.**
- [x] **P123 — Precompute WAV valid-bit shifts.**
- [x] **P124 — Precompute WAV normalization reciprocals.**
- [x] **P125 — Optimize WAV unsigned 8-bit PCM.**
- [x] **P126 — Optimize WAV signed 16-bit PCM.**
- [x] **P127 — Optimize WAV packed 24-bit PCM.**
- [x] **P128 — Optimize WAV signed 32-bit PCM.**
- [x] **P129 — Optimize WAV 32-bit IEEE float.**
- [x] **P130 — Optimize WAV 64-bit IEEE float.**
- [x] **P131 — Add native-endian f32 bulk-copy fast path where safe.**
- [x] **P132 — Add SIMD 16-bit PCM path.**
- [x] **P133 — Add SIMD 24-bit PCM path.**
- [x] **P134 — Add SIMD 32-bit PCM path.**
- [x] **P135 — Add SIMD f32 sanitize path.**
- [x] **P136 — Add SIMD f64 conversion path.**
- [x] **P137 — Tune frames_per_chunk automatically.**
- [x] **P138 — Align WAV chunks with playback mix block sizes.**
- [x] **P139 — Align WAV chunks with cache-friendly byte sizes.**
- [x] **P140 — Avoid temporary format-validation work per frame.**
- [x] **P141 — Avoid redundant remaining-frame divisions.**
- [x] **P142 — Cache duration values.**
- [x] **P143 — Add direct O(1) WAV seeking.**
- [x] **P144 — Make WAV reset allocation-free.**
- [x] **P145 — Add WAV per-bit-depth performance regression tests.**

## AIFF/AIFC demuxer + PCM codec

- [x] **P146 — Reuse AIFF input-byte scratch between frames.**
- [x] **P147 — Decode AIFF directly into reusable PCM output.**
- [x] **P148 — Precompute AIFF sample-size shifts.**
- [x] **P149 — Precompute AIFF normalization reciprocals.**
- [x] **P150 — Optimize AIFF 8-bit integer PCM.**
- [x] **P151 — Optimize AIFF 16-bit big-endian PCM.**
- [x] **P152 — Optimize AIFF packed 24-bit big-endian PCM.**
- [x] **P153 — Optimize AIFF 32-bit big-endian PCM.**
- [x] **P154 — Optimize AIFC little-endian PCM variants.**
- [x] **P155 — Add SIMD AIFF 16-bit conversion.**
- [x] **P156 — Add SIMD AIFF 24-bit conversion.**
- [x] **P157 — Add SIMD AIFF 32-bit conversion.**
- [x] **P158 — Tune AIFF frames_per_chunk automatically.**
- [x] **P159 — Align AIFF chunks with playback block sizes.**
- [x] **P160 — Avoid temporary format validation per frame.**
- [x] **P161 — Cache AIFF duration values.**
- [x] **P162 — Add direct O(1) AIFF seeking.**
- [x] **P163 — Make AIFF reset allocation-free.**
- [x] **P164 — Add AIFF per-bit-depth performance tests.**
- [x] **P165 — Optimize 80-bit extended sample-rate parsing cold path.**

## FLAC decoder

- [x] **P166 — Batch FLAC BitReader byte input.**
- [x] **P167 — Replace per-bit read loops with a word-sized reservoir.**
- [x] **P168 — Optimize FLAC unary zero decoding.**
- [x] **P169 — Make FLAC BitReader reusable across frames.**
- [x] **P170 — Remove raw Vec byte capture used only for CRC.**
- [x] **P171 — Compute FLAC CRC-8 incrementally while reading.**
- [x] **P172 — Compute FLAC CRC-16 incrementally while reading.**
- [x] **P173 — Avoid second-pass CRC scans.**
- [x] **P174 — Reuse FLAC channel scratch buffers.**
- [x] **P175 — Flatten FLAC channel scratch into one contiguous allocation.**
- [x] **P176 — Reuse FLAC subframe output buffers.**
- [x] **P177 — Decode constant subframes into provided slices.**
- [x] **P178 — Decode verbatim subframes into provided slices.**
- [x] **P179 — Specialize fixed predictor order 0.**
- [x] **P180 — Specialize fixed predictor order 1.**
- [x] **P181 — Specialize fixed predictor order 2.**
- [x] **P182 — Specialize fixed predictor order 3.**
- [x] **P183 — Specialize fixed predictor order 4.**
- [x] **P184 — Optimize generic fixed-predictor restore.**
- [x] **P185 — Optimize LPC coefficient reading.**
- [x] **P186 — Store common LPC coefficients in fixed-size stack/local arrays.**
- [x] **P187 — Optimize LPC reconstruction loop.**
- [x] **P188 — Hoist LPC invariant checks.**
- [x] **P189 — Reduce i64 operations where validated sample ranges permit.**
- [x] **P190 — Optimize Rice partition loops.**
- [x] **P191 — Specialize Rice parameter zero.**
- [x] **P192 — Optimize Rice unary quotient extraction.**
- [x] **P193 — Optimize Rice remainder extraction.**
- [x] **P194 — Optimize escape-coded residuals.**
- [x] **P195 — Reuse residual scratch storage.**
- [x] **P196 — Avoid clearing untouched residual scratch ranges.**
- [x] **P197 — Fuse residual decode with predictor restore where practical.**
- [x] **P198 — Vectorize independent residual additions.**
- [x] **P199 — Optimize left-side stereo restoration.**
- [x] **P200 — Optimize right-side stereo restoration.**
- [x] **P201 — Optimize mid-side stereo restoration.**
- [x] **P202 — SIMD FLAC stereo decorrelation.**
- [x] **P203 — Fuse channel restoration with f32 normalization.**
- [x] **P204 — Fuse interleave with normalization.**
- [x] **P205 — Use multiply-by-reciprocal for normalization.**
- [x] **P206 — Pre-reserve scratch using STREAMINFO max_block_size.**
- [x] **P207 — Parse FLAC SEEKTABLE metadata.**
- [x] **P208 — Use FLAC SEEKTABLE for fast seek.**
- [x] **P209 — Build a lazy fallback FLAC seek index.**
- [x] **P210 — Cache observed frame byte/sample positions.**
- [x] **P211 — Make FLAC reset preserve indexes and scratch.**
- [x] **P212 — Add FLAC predictor-family benchmarks.**
- [x] **P213 — Add FLAC Rice-parameter benchmarks.**
- [x] **P214 — Add FLAC mono/stereo benchmarks.**
- [x] **P215 — Add FLAC high-bit-depth benchmarks.**
- [x] **P216 — Add FLAC stage-level profiling.**

## MPEG Audio Layer I/II/III codec

- [x] **P217 — Persist the shared MPEG Audio PCM output buffer.**
- [x] **P218 — Reuse the MPEG layer-wrapper output path without an additional PCM allocation.**
- [x] **P219 — Replace MPEG compressed-input Vec::drain with a head index.**
- [x] **P220 — Replace MPEG preserve-tail Vec::drain with threshold compaction.**
- [x] **P221 — Read MPEG Audio bytes directly into spare capacity.**
- [x] **P222 — Tune MPEG Audio refill chunk size.**
- [x] **P223 — Tune MPEG Audio lookahead size.**
- [x] **P224 — Reduce duplicate work between probe and first decode.**
- [x] **P225 — Add a synchronized next-header fast path.**
- [x] **P226 — Skip ID3v2 efficiently before frame search.**
- [x] **P227 — Skip ID3v1/APEv2/trailing tags efficiently.**
- [x] **P228 — Cache frame format invariants.**
- [x] **P229 — Audit repeated get().unwrap_or() in hot loops.**
- [x] **P230 — Prevalidate table ranges before hot decoding.**
- [x] **P231 — Use unchecked table indexing in proven-safe inner loops.**
- [x] **P232 — Optimize Huffman cache refill.**
- [x] **P233 — Optimize Huffman codebook base lookup.**
- [x] **P234 — Optimize Huffman linbits handling.**
- [x] **P235 — Optimize count1 Huffman path.**
- [x] **P236 — Optimize scalefactor decoding.**
- [x] **P237 — Optimize scalefactor application.**
- [x] **P238 — Maximize MP3 pow(4/3) table coverage.**
- [x] **P239 — Remove fallback pow-like work from normal dequantization.**
- [x] **P240 — SIMD MP3 dequantization.**
- [x] **P241 — SIMD mid/side stereo.**
- [x] **P242 — SIMD intensity stereo.**
- [x] **P243 — Optimize short-block reorder.**
- [x] **P244 — SIMD short-block reorder where profitable.**
- [x] **P245 — SIMD antialias butterflies.**
- [x] **P246 — Optimize IMDCT36.**
- [x] **P247 — SIMD IMDCT36 butterflies.**
- [x] **P248 — Optimize IMDCT12.**
- [x] **P249 — SIMD IMDCT12.**
- [x] **P250 — Optimize overlap-save handling.**
- [x] **P251 — Optimize Layer III reservoir save/restore.**
- [x] **P252 — Avoid unnecessary reservoir copying.**
- [x] **P253 — Optimize Layer III side-info parsing.**
- [x] **P254 — Optimize synthesis DCT-II.**
- [x] **P255 — SIMD synthesis DCT-II.**
- [x] **P256 — Optimize synthesis-window multiply-accumulate.**
- [x] **P257 — SIMD synthesis-window multiply-accumulate.**
- [x] **P258 — Fuse synthesis with PCM scaling.**
- [x] **P259 — Specialize mono synthesis.**
- [x] **P260 — Specialize stereo synthesis.**
- [x] **P261 — Reuse large mp3dec_t scratch arrays.**
- [x] **P262 — Clear only used MP3 scratch ranges.**
- [x] **P263 — Add MPEG Layer I benchmarks.**
- [x] **P264 — Add MPEG Layer II benchmarks.**
- [x] **P265 — Add MPEG Layer III benchmarks.**
- [x] **P266 — Add MPEG Audio stage-level timing.**
- [x] **P267 — Add MPEG Audio sparse frame index.**
- [x] **P268 — Use sparse frame index for approximate seek.**
- [x] **P269 — Make MPEG Audio reset preserve buffer capacity.**

## Vorbis codec + Ogg demuxer

- [x] **P270 — Replace the current O(N^2) Vorbis IMDCT with an O(N log N) implementation.**
- [x] **P271 — Create reusable IMDCT plans for small and large block sizes.**
- [x] **P272 — Precompute IMDCT trigonometric tables.**
- [x] **P273 — Precompute IMDCT bit-reversal/order tables.**
- [x] **P274 — Reuse IMDCT scratch buffers.**
- [x] **P275 — SIMD Vorbis IMDCT butterflies.**
- [x] **P276 — Precompute both Vorbis window slopes.**
- [x] **P277 — Reuse decoded-floor storage.**
- [x] **P278 — Reuse floor0 coefficient scratch.**
- [x] **P279 — Reuse floor1 y scratch.**
- [x] **P280 — Reuse floor1 final_y scratch.**
- [x] **P281 — Reuse floor1 step2 scratch.**
- [x] **P282 — Reuse floor1 rendered scratch.**
- [x] **P283 — Precompute floor1 low-neighbor relationships.**
- [x] **P284 — Precompute floor1 high-neighbor relationships.**
- [x] **P285 — Precompute floor1 sorted render order.**
- [x] **P286 — Cache inverse-dB table values.**
- [x] **P287 — Remove per-packet floor1 sorting work.**
- [x] **P288 — Reduce per-packet floor0 cosine work.**
- [x] **P289 — Cache floor0 Bark/cosine mapping fully.**
- [x] **P290 — Reuse Vorbis residue channel buffers.**
- [x] **P291 — Flatten residue channel buffers into one arena.**
- [x] **P292 — Reuse residue classification buffers.**
- [x] **P293 — Reuse submap channel-index buffers.**
- [x] **P294 — Reuse submap skip buffers.**
- [x] **P295 — Reuse coupling scratch.**
- [x] **P296 — Precompute residue book/pass plans.**
- [x] **P297 — Flatten nested residue book structures.**
- [x] **P298 — Optimize residue type 0.**
- [x] **P299 — Optimize residue type 1.**
- [x] **P300 — Optimize residue type 2.**
- [x] **P301 — Batch codebook vector writes.**
- [x] **P302 — Optimize codebook prefix decoding.**
- [x] **P303 — Add a first-level Huffman lookup table.**
- [x] **P304 — Add multi-level Huffman lookup tables.**
- [x] **P305 — Keep fallback tree traversal for long codes.**
- [x] **P306 — Store decoded codebook vectors contiguously.**
- [x] **P307 — Return borrowed slices for codebook vectors.**
- [x] **P308 — Optimize Vorbis bit reservoir.**
- [x] **P309 — Reduce bounds checks in codebook decoding.**
- [x] **P310 — Optimize inverse coupling.**
- [x] **P311 — SIMD inverse coupling.**
- [x] **P312 — Fuse floor multiplication into residue spectrum handling.**
- [x] **P313 — SIMD floor multiplication.**
- [x] **P314 — Reuse time-domain channel buffers.**
- [x] **P315 — Avoid per-channel to_vec for overlap tails.**
- [x] **P316 — Keep persistent previous/future overlap storage.**
- [x] **P317 — Fuse overlap/window operations.**
- [x] **P318 — SIMD overlap/window operations.**
- [x] **P319 — Reuse final interleaved output storage.**
- [x] **P320 — SIMD planar-to-interleaved conversion.**
- [x] **P321 — Specialize stereo interleave.**
- [x] **P322 — Avoid Ogg crc_data concatenation.**
- [x] **P323 — Compute Ogg CRC incrementally over header/lacing/body.**
- [x] **P324 — Reuse Ogg lacing storage.**
- [x] **P325 — Reuse Ogg page-body storage.**
- [x] **P326 — Reuse partial-packet storage.**
- [x] **P327 — Avoid reallocating partial packets across pages.**
- [x] **P328 — Use direct packet slices for single-page packets where lifetimes allow.**
- [x] **P329 — Reduce Ogg packet copy count.**
- [x] **P330 — Make Vorbis reset avoid reparsing setup data.**
- [x] **P331 — Make Vorbis reset avoid reparsing comments.**
- [x] **P332 — Make Vorbis reset preserve all scratch capacity.**
- [x] **P333 — Add Ogg page index by granule position.**
- [x] **P334 — Use Ogg page index for Vorbis seeking.**
- [x] **P335 — Add Vorbis small-block benchmarks.**
- [x] **P336 — Add Vorbis large-block benchmarks.**
- [x] **P337 — Add Vorbis residue-type benchmarks.**
- [x] **P338 — Add Vorbis codebook benchmarks.**
- [x] **P339 — Add Vorbis packet-stage timing.**

## Demuxer performance and streaming behavior

- [x] **P340 — Add demuxer-only startup and throughput benchmarks for every enabled container.**
- [x] **P341 — Add bounded reusable scratch for common demuxer header reads.**
- [x] **P342 — Avoid payload allocation when RIFF/WAVE chunks can be skipped by seek.**
- [x] **P343 — Optimize RIFF/RF64 chunk scanning and alignment handling.**
- [x] **P344 — Cache RIFF/WAVE data boundaries and format invariants after open.**
- [x] **P345 — Avoid payload allocation when AIFF/AIFC chunks can be skipped by seek.**
- [x] **P346 — Optimize AIFF/AIFC chunk scanning and even-byte alignment handling.**
- [x] **P347 — Cache AIFF/AIFC SSND boundaries and COMM invariants after open.**
- [x] **P348 — Stream FLAC metadata-block traversal without retaining unneeded block payloads.**
- [x] **P349 — Bound FLAC metadata-block allocation before reading payloads.**
- [x] **P350 — Reuse Ogg page-header and lacing scratch storage.**
- [x] **P351 — Reuse Ogg page-body and continued-packet storage.**
- [x] **P352 — Add zero-copy Ogg packet slices for complete single-page packets where lifetimes permit.**
- [x] **P353 — Optimize Matroska/WebM EBML variable-integer decoding.**
- [x] **P354 — Skip unknown Matroska/WebM elements without payload allocation.**
- [x] **P355 — Optimize Matroska cluster/block traversal for sequential audio reads.**
- [x] **P356 — Add bounded handling for unknown-size EBML elements.**
- [x] **P357 — Add a common demuxer seek/index capability interface.**
- [x] **P358 — Fuzz every demuxer with corrupt sizes, nesting, truncation, and seek boundaries.**
- [x] **P359 — Add per-demuxer performance regression thresholds.**

## Metadata parsing and feature-isolated builds

- [x] **P360 — Add parser-only benchmarks for every enabled metadata family.**
- [x] **P361 — Benchmark `metadata::read_all()` with the full default set and minimal feature sets.**
- [x] **P362 — Verify every metadata reader preserves the caller's stream position when documented to do so.**
- [x] **P363 — Add shared bounded-read helpers for metadata lengths and offsets.**
- [x] **P364 — Minimize seeks while probing APEv1/APEv2 tails.**
- [x] **P365 — Reduce temporary allocation while parsing APE items.**
- [x] **P366 — Keep ID3v1 parsing to one bounded tail read.**
- [x] **P367 — Optimize ID3v2 synchsafe integer decoding and frame iteration.**
- [x] **P368 — Reuse scratch for ID3v2 unsynchronization where required.**
- [x] **P369 — Avoid unnecessary ID3v2 frame payload copies.**
- [x] **P370 — Optimize ISO/MP4 atom traversal without reading media payload atoms.**
- [x] **P371 — Add fast skipping for large ISO/MP4 atoms and extended-size atoms.**
- [x] **P372 — Reduce allocation in ISO/MP4 item-list metadata parsing.**
- [x] **P373 — Optimize RIFF INFO/LIST metadata traversal independently of PCM decoding.**
- [x] **P374 — Share bounded Vorbis-comment parsing between FLAC and Ogg metadata paths.**
- [x] **P375 — Bound embedded-picture allocations before copying image payloads.**
- [x] **P376 — Reduce metadata key/value `String` allocation where borrowing is practical.**
- [x] **P377 — Evaluate borrowed metadata field views alongside the owned compatibility API.**
- [x] **P378 — Fuzz each metadata feature independently, including malformed length/count fields.**
- [x] **P379 — Add per-metadata-family performance and allocation regression tests.**

## Cargo feature matrix and code-size control

- [x] **P380 — `cargo check --no-default-features` with only the common error/core API compiled.**
- [x] **P381 — Check every individual codec feature in isolation.**
- [x] **P382 — Check every individual demuxer feature in isolation.**
- [x] **P383 — Check every individual metadata feature in isolation.**
- [x] **P384 — Check the `all-codecs` aggregate feature.**
- [x] **P385 — Check the `all-demuxers` aggregate feature.**
- [x] **P386 — Check the `all-metadata` aggregate feature.**
- [x] **P387 — Check each relevant codec matrix with SIMD enabled and disabled.**
- [x] **P388 — Check `auto-detect` independently and verify it enables exactly its required decode/demux features.**
- [x] **P389 — Check both playback backend feature paths against the feature-gated decoder API.**
- [x] **P390 — Add compile-fail/API tests proving disabled codec/demuxer/metadata modules are not exposed.**
- [x] **P391 — Add a test that the default feature set exposes every supported codec, demuxer, and metadata parser.**
- [x] **P392 — Track binary-size contribution of each codec/demuxer/metadata feature.**
- [x] **P393 — Track compile-time contribution of each aggregate feature group.**
- [x] **P394 — Document supported minimal feature combinations and feature dependencies.**
- [x] **P395 — Add CI jobs for no-default, scalar-only, SIMD, default, and representative minimal builds.**

## Playback engine, mixer, resampling, and real-time behavior

- [x] **P396 — Remove per-mix-block failures Vec allocation.**
- [x] **P397 — Keep Mixer::mix_buffer in place instead of mem::take.**
- [x] **P398 — Clear only the active mix-buffer span.**
- [x] **P399 — Preserve mix-buffer capacity permanently.**
- [x] **P400 — Maintain active voice count instead of rescanning HashMap.**
- [x] **P401 — Replace HashMap hot iteration with contiguous voice storage.**
- [x] **P402 — Separate source-ID lookup from packed hot voice storage.**
- [x] **P403 — Use a slab/generational voice store.**
- [x] **P404 — Batch command processing without starving audio generation.**
- [x] **P405 — Bound commands processed per mixer iteration.**
- [x] **P406 — Replace fixed 1 ms polling sleep with event/timeout-driven waiting.**
- [x] **P407 — Wake mixer based on ring-fill needs.**
- [x] **P408 — Make target mix-ahead adaptive.**
- [x] **P409 — Make mix block size adaptive.**
- [x] **P410 — Tune mix block size to device period.**
- [x] **P411 — Tune mix block size to active voice count.**
- [x] **P412 — Prefer native f32 device output.**
- [x] **P413 — Specialize SPSC read for f32 output.**
- [x] **P414 — Specialize SPSC read for i16 output.**
- [x] **P415 — SIMD f32-to-i16 callback conversion.**
- [x] **P416 — Expose SPSC wrap as at most two contiguous spans.**
- [x] **P417 — Bulk-copy f32 ring spans.**
- [x] **P418 — Bulk-convert non-f32 ring spans.**
- [x] **P419 — Pad SPSC read/write indices to separate cache lines.**
- [x] **P420 — Audit SPSC atomic memory orderings.**
- [x] **P421 — Use weakest correct SPSC orderings.**
- [x] **P422 — Compute ring occupancy from one index snapshot.**
- [x] **P423 — Avoid repeated available/free ring calculations.**
- [x] **P424 — Keep device callback strictly allocation-free.**
- [x] **P425 — Keep device callback strictly lock-free.**
- [x] **P426 — Keep device callback free of formatting/logging.**
- [x] **P427 — Defer backend error formatting to non-real-time code.**
- [x] **P428 — Replace callback Arc<Mutex<Option<String>>> error reporting.**
- [x] **P429 — Use atomic/status-code error publication.**
- [x] **P430 — Replace common AudioSourceState Mutex reads with atomics/snapshots.**
- [x] **P431 — Publish source state at UI cadence rather than every mix block.**
- [x] **P432 — Coalesce sync_state updates.**
- [x] **P433 — Use fixed-point resampler phase.**
- [x] **P434 — Remove f64 floor from the per-output-frame hot loop.**
- [x] **P435 — Specialize source_rate == output_rate.**
- [x] **P436 — Specialize mono-to-mono mixing.**
- [x] **P437 — Specialize mono-to-stereo mixing.**
- [x] **P438 — Specialize stereo-to-stereo mixing.**
- [x] **P439 — Specialize stereo-to-mono mixing.**
- [x] **P440 — Precompute generic channel-routing matrices.**
- [x] **P441 — SIMD equal-rate gain+mix.**
- [x] **P442 — SIMD linear interpolation.**
- [x] **P443 — Batch interpolation indices/fractions.**
- [x] **P444 — Fuse interpolation with gain.**
- [x] **P445 — Fuse interpolation with channel mapping.**
- [x] **P446 — Fuse interpolation/channel mapping/gain/mix-add.**
- [x] **P447 — Avoid Voice staging Vec::drain.**
- [x] **P448 — Use ring/head-tail Voice staging.**
- [x] **P449 — Decode into reusable per-voice producer/staging capacity without central-mixer decode allocation.**
- [x] **P450 — Adapt source decode granularity to output demand.**
- [x] **P451 — Prefetch the next compressed frame before staging empties.**
- [x] **P452 — Reduce ensure_frame calls inside the per-output-frame loop.**
- [x] **P453 — Ensure several source frames per refill.**
- [x] **P454 — Cache source/output rate ratio.**
- [x] **P455 — Cache resampler step in fixed-point form.**
- [x] **P456 — Cache gain in vector-friendly form.**
- [x] **P457 — Apply gain in blocks.**
- [x] **P458 — SIMD voice mix-add.**
- [x] **P459 — Track conservative block peak while mixing.**
- [x] **P460 — Skip limiter when mathematically safe.**
- [x] **P461 — Replace expensive limiter math if a cheaper equivalent is acceptable.**
- [x] **P462 — SIMD final limiter.**
- [x] **P463 — Make limiter quality selectable if needed.**
- [x] **P464 — Add per-voice decoded PCM producer option.**
- [x] **P465 — Move expensive compressed decoders off the central mixer thread.**
- [x] **P466 — Use per-voice SPSC queues for decoded PCM.**
- [x] **P467 — Add a bounded decoder worker pool.**
- [x] **P468 — Parallelize independent source decoding.**
- [x] **P469 — Keep central mixer deterministic and lock-free.**
- [x] **P470 — Add an optional worker-tuning hook for platform-specific audio/decoder priority elevation.**
- [x] **P471 — Support opt-in CPU-affinity experiments through the worker-tuning hook.**
- [x] **P472 — Expose mixer CPU-time telemetry.**
- [x] **P473 — Expose decoder worker CPU-time telemetry.**
- [x] **P474 — Expose ring-fill telemetry.**
- [x] **P475 — Expose underrun telemetry.**
- [x] **P476 — Expose callback-period telemetry.**
- [x] **P477 — Add automatic buffer tuning from underrun history.**

## Seeking, looping, and timeline efficiency

- [x] **P478 — Add optional Decoder native-seek capability.**
- [x] **P479 — Add sample-index seek API.**
- [x] **P480 — Add timestamp seek API.**
- [x] **P481 — Use direct WAV byte seeking.**
- [x] **P482 — Use direct AIFF byte seeking.**
- [x] **P483 — Use FLAC SEEKTABLE seeking.**
- [x] **P484 — Use lazy FLAC frame index fallback.**
- [x] **P485 — Use MP3 sparse frame index.**
- [x] **P486 — Use Vorbis granule/page index.**
- [x] **P487 — Keep seek dispatch feature-aware so disabled codecs add no seek branches or dead format dependencies.**
- [x] **P488 — Avoid reopening files on every seek.**
- [x] **P489 — Avoid decoding from the beginning on every seek.**
- [x] **P490 — Reuse Voice buffers across seek.**
- [x] **P491 — Reuse decoder scratch across seek.**
- [x] **P492 — Preserve compressed-input capacity across seek.**
- [x] **P493 — Make stop-to-zero use native reset/seek.**
- [x] **P494 — Make looping use native reset/seek.**
- [x] **P495 — Avoid reopening decoder at loop boundary.**
- [x] **P496 — Prebuffer short loop boundaries.**
- [x] **P497 — Cache exact loop-start PCM/decoder continuation state where practical.**
- [x] **P498 — Add seek accuracy tests.**
- [x] **P499 — Add repeated-random-seek performance tests.**
- [x] **P500 — Add loop-gap tests.**
- [x] **P501 — Add long-duration timeline drift tests.**

## P502-P601 implementation pass

- [x] Decoder and playback retained/high-water memory accounting, bounded/aligned scratch, explicit cold trimming, hour-long flatness harnesses, and 128-source scaling probes are implemented.
- [x] Portable and benchmark-focused Cargo profiles plus codegen, assembly, binary-size, instruction-cache, and panic-strategy harnesses are implemented.
- [x] Backend/device policy now caches output configuration, preserves decoders across device changes, prefers native f32/source-near sample rates, and exposes platform period/ring policies plus opt-in realtime worker tuning.
- [x] Offline decode has bounded read-ahead, independent-stream parallelism, thresholded channel parallelism, optional mmap, and separate non-default prefetch/fast-math experiments.
- [x] `performance/scripts/validate_p502_p601.sh` performs all available static/corpus/script validation and runs Cargo tests/benchmarks when a Rust toolchain is present.
- [x] **P502-P601 marked complete by project-owner instruction.** Machine-specific integration/benchmark numbers remain an optional follow-up on a Rust-enabled host.

## Memory footprint and cache locality

- [x] **P502 — Create a memory budget report for every decoder.**
- [x] **P503 — Measure persistent decoder state size.**
- [x] **P504 — Measure peak scratch size per decoder.**
- [x] **P505 — Measure compressed-buffer high-water marks.**
- [x] **P506 — Measure playback Voice buffer high-water marks.**
- [x] **P507 — Flatten nested channel allocations.**
- [x] **P508 — Use compact index integer types where validated.**
- [x] **P509 — Use compact codebook metadata layouts.**
- [x] **P510 — Use compact FLAC metadata layouts.**
- [x] **P511 — Separate hot state from cold metadata.**
- [x] **P512 — Keep comments/tags out of hot decoder cache lines.**
- [x] **P513 — Align hot float buffers for SIMD.**
- [x] **P514 — Align hot integer scratch for SIMD.**
- [x] **P515 — Avoid pathological reserve on corrupt sizes.**
- [x] **P516 — Validate sizes before allocation.**
- [x] **P517 — Bound scratch high-water marks.**
- [x] **P518 — Provide explicit non-real-time shrink/reset APIs if needed.**
- [x] **P519 — Keep default steady-state buffers reusable.**
- [x] **P520 — Add hour-long decode memory-flatness tests.**
- [x] **P521 — Add hour-long playback memory-flatness tests.**
- [x] **P522 — Add 128-source memory-scaling tests.**
- [x] **P523 — Audit Arc usage in playback state.**
- [x] **P524 — Audit PathBuf duplication.**
- [x] **P525 — Audit String allocation in error state.**
- [x] **P526 — Audit temporary Vec allocations with an allocation profiler.**

## Compiler, code generation, and build profiles

- [x] **P527 — Add a benchmark-focused release profile.**
- [x] **P528 — Benchmark opt-level=3.**
- [x] **P529 — Benchmark opt-level=s.**
- [x] **P530 — Benchmark thin LTO.**
- [x] **P531 — Benchmark fat LTO.**
- [x] **P532 — Benchmark codegen-units=1.**
- [x] **P533 — Benchmark moderate codegen-unit counts.**
- [x] **P534 — Document target-cpu=native benchmark builds.**
- [x] **P535 — Keep portable runtime-dispatched release builds.**
- [x] **P536 — Add compile-time specialized CPU builds as an optional benchmark path.**
- [x] **P537 — Audit #[inline] on tiny hot helpers.**
- [x] **P538 — Remove harmful over-inlining.**
- [x] **P539 — Mark cold error functions #[cold] where appropriate.**
- [x] **P540 — Use #[inline(never)] on large cold formatting paths where useful.**
- [x] **P541 — Split validation from hot decode kernels.**
- [x] **P542 — Pass validated invariants into inner kernels.**
- [x] **P543 — Inspect generated assembly for PCM conversion.**
- [x] **P544 — Inspect generated assembly for FLAC Rice/LPC.**
- [x] **P545 — Inspect generated assembly for MP3 Huffman/dequant.**
- [x] **P546 — Inspect generated assembly for MP3 synthesis.**
- [x] **P547 — Inspect generated assembly for Vorbis IMDCT.**
- [x] **P548 — Inspect generated assembly for mixer resampling.**
- [x] **P549 — Track binary-size impact of SIMD variants.**
- [x] **P550 — Track instruction-cache impact of specialization.**
- [x] **P551 — Benchmark panic strategy choices for release artifacts.**

## Cross-platform backend and OS efficiency

- [x] **P552 — Benchmark mata-playback versus CPAL on Windows.**
- [x] **P553 — Benchmark mata-playback versus CPAL on Linux where both are usable.**
- [x] **P554 — Prefer low-overhead native backend paths.**
- [x] **P555 — Cache device configuration after engine startup.**
- [x] **P556 — Avoid re-enumerating output formats during normal playback.**
- [x] **P557 — Handle output device changes without reopening source decoders.**
- [x] **P558 — Prefer native device sample format.**
- [x] **P559 — Prefer a device sample rate that minimizes total resampling cost.**
- [x] **P560 — Tune Windows output period size.**
- [x] **P561 — Tune Windows output ring size.**
- [x] **P562 — Tune Linux ALSA period size.**
- [x] **P563 — Tune Linux ALSA buffer size.**
- [x] **P564 — Tune PulseAudio buffering.**
- [x] **P565 — Tune Android frames-per-burst handling where available.**
- [x] **P566 — Tune Android ring size where available.**
- [x] **P567 — Add opt-in platform real-time worker scheduling/priority tuning.**
- [x] **P568 — Gracefully fall back when priority elevation is unavailable.**
- [x] **P569 — Measure denormal-number penalties.**
- [x] **P570 — Add denormal mitigation if profiles justify it.**
- [x] **P571 — Add x86_64 Windows performance smoke tests.**
- [x] **P572 — Add x86_64 Linux performance smoke tests.**
- [x] **P573 — Add ARM64 performance smoke tests.**
- [x] **P574 — Add Android performance smoke tests when supported.**

## Advanced algorithmic and parallel performance work

- [x] **P575 — Pipeline compressed reading and decode.**
- [x] **P576 — Pipeline decode and playback staging.**
- [x] **P577 — Use bounded queues between pipeline stages.**
- [x] **P578 — Parallelize independent channels only when blocks are large enough.**
- [x] **P579 — Parallelize offline decoding where codec dependencies allow it.**
- [x] **P580 — Add an offline high-throughput decode mode.**
- [x] **P581 — Keep real-time mode latency-first.**
- [x] **P582 — Select specialized kernels once per stream.**
- [x] **P583 — Cache function pointers for bit depth/endian combinations.**
- [x] **P584 — Cache function pointers for channel layout combinations.**
- [x] **P585 — Cache function pointers for resampler cases.**
- [x] **P586 — Cache function pointers for limiter cases.**
- [x] **P587 — Evaluate explicit prefetch in large MP3 tables.**
- [x] **P588 — Evaluate explicit prefetch in Vorbis codebook tables.**
- [x] **P589 — Use branchless common-case sample sanitization.**
- [x] **P590 — Move rare malformed-data checks out of inner loops after coarse validation.**
- [x] **P591 — Evaluate fixed-point resampling arithmetic.**
- [x] **P592 — Evaluate fixed-point timeline arithmetic.**
- [x] **P593 — Reduce unnecessary f64 DSP calculations.**
- [x] **P594 — Benchmark f32 replacements with error bounds.**
- [x] **P595 — Add optional fast-math feature only with quantified error limits.**
- [x] **P596 — Keep exact/default math path unchanged.**
- [x] **P597 — Experiment with wider transform blocking for cache locality.**
- [x] **P598 — Experiment with small-stack scratch for tiny blocks.**
- [x] **P599 — Experiment with arena allocation for decoder setup structures.**
- [x] **P600 — Evaluate memory mapping only as an optional fast path.**
- [x] **P601 — Add decoder-specific prefetch distances only if profiling supports them.**

## Validation, fuzzing, and final hardening

- [x] Deterministic corpus-differential, bit-reader, byte-window, SIMD and playback-SPSC hardening tests are present.
- [x] Host-dependent Miri/sanitizer, thermal-stability, physical-device, profiler and freeze commands are implemented.
- [ ] **Execution-only verification (separate from phase completion):** run Rust tests, nightly unsafe checks, real-device metrics and final top-down profiling on a supported host; freeze a measured baseline only after validation.


- [x] **P602 — Add differential output tests against pre-optimization decoder output.**
- [x] **P603 — Add reference-file PCM checksums.**
- [x] **P604 — Fuzz FLAC bit-reader fast paths.**
- [x] **P605 — Fuzz Vorbis bit-reader fast paths.**
- [x] **P606 — Fuzz MP3 compressed-buffer head-index logic.**
- [x] **P607 — Fuzz decode_into buffer sizing.**
- [x] **P608 — Fuzz reset after partial decode.**
- [x] **P609 — Fuzz seek after partial decode.**
- [x] **P610 — Fuzz loop/reset transitions.**
- [x] **P611 — Stress SPSC wraparound.**
- [x] **P612 — Stress SPSC producer/consumer races.**
- [x] **P613 — Stress command queue saturation.**
- [x] **P614 — Stress many simultaneous EOF transitions.**
- [x] **P615 — Stress many simultaneous decoder errors.**
- [x] **P616 — Stress device callback underruns.**
- [x] **P617 — Stress decoder-worker starvation.**
- [x] **P618 — Run Miri on non-SIMD unsafe infrastructure where feasible.**
- [x] **P619 — Run address/undefined-behavior sanitizers where feasible.**
- [x] **P620 — Test unaligned SIMD inputs.**
- [x] **P621 — Test tiny SIMD inputs.**
- [x] **P622 — Test SIMD tail handling.**
- [x] **P623 — Test NaN/Inf sample conversion.**
- [x] **P624 — Test corrupt size fields for bounded allocation.**
- [x] **P625 — Test long-duration resampler drift.**
- [x] **P626 — Test long-duration source-position drift.**
- [x] **P627 — Add sustained multi-minute thermal benchmarks.**
- [x] **P628 — Re-profile after every major milestone.**
- [x] **P629 — Re-rank hotspots after Vorbis IMDCT replacement.**
- [x] **P630 — Re-rank hotspots after buffer-reuse conversion.**
- [x] **P631 — Re-rank hotspots after mixer specialization.**
- [x] **P632 — Define per-codec minimum real-time factors.**
- [x] **P633 — Define per-codec maximum allocations per second.**
- [x] **P634 — Define per-codec peak-memory budgets.**
- [x] **P635 — Define mixer maximum CPU budget.**
- [x] **P636 — Define maximum acceptable underrun rate.**
- [x] **P637 — Define startup latency budgets.**
- [x] **P638 — Define seek latency budgets.**
- [x] **P639 — Define command-to-audio latency budget.**
- [x] **P640 — Create a final performance report.**
- [x] **P641 — Record benchmark hardware and OS in the final report.**
- [x] **P642 — Record compiler and flags in the final report.**
- [x] **P643 — Record feature sets in the final report.**
- [x] **P644 — Record corpus hashes in the final report.**
- [x] **P645 — Record throughput/latency/allocation/memory results.**
- [x] **P646 — Create a performance changelog section.**
- [x] **P647 — Require benchmark notes for future hot-path changes.**
- [x] **P648 — Keep debug instrumentation compiled out of normal release builds.**
- [x] **P649 — Perform a final top-down profiler pass.**
- [x] **P650 — Create follow-on phases from the final top five hotspots.**
- [x] **P651 — Freeze a known-good performance baseline.**

## Suggested implementation order

The highest-return early work is usually: measurement → Cargo feature-matrix validation → reusable decode buffers → compressed-buffer head indices → FLAC buffered bit reading → shared MPEG Audio persistent PCM → Vorbis O(N log N) IMDCT → demuxer/metadata allocation reduction → mixer/resampler specialization → native indexed seeking → final profiler-driven cleanup.

## Group ranges

- **P0–P30:** Measurement, baselines, and regression gates
- **P31–P60:** Core decode API and allocation architecture
- **P61–P90:** I/O, compressed buffering, and generic bit reading
- **P91–P120:** SIMD and CPU dispatch infrastructure
- **P121–P145:** RIFF/WAVE demuxer + PCM codec
- **P146–P165:** AIFF/AIFC demuxer + PCM codec
- **P166–P216:** FLAC decoder
- **P217–P269:** MPEG Audio Layer I/II/III codec
- **P270–P339:** Vorbis codec + Ogg demuxer
- **P340–P359:** Demuxer performance and streaming behavior
- **P360–P379:** Metadata parsing and feature-isolated builds
- **P380–P395:** Cargo feature matrix and code-size control
- **P396–P477:** Playback engine, mixer, resampling, and real-time behavior
- **P478–P501:** Seeking, looping, and timeline efficiency
- **P502–P526:** Memory footprint and cache locality
- **P527–P551:** Compiler, code generation, and build profiles
- **P552–P574:** Cross-platform backend and OS efficiency
- **P575–P601:** Advanced algorithmic and parallel performance work
- **P602–P651:** Validation, fuzzing, and final hardening

## Completion rule

A roadmap phase may be marked complete when implementation and test/benchmark infrastructure are present, even when host-dependent Rust execution has not yet been run, per the project owner’s instruction. **The phase checkboxes describe implementation status, not passing integration or a verified performance baseline.** Use the separate execution-only gate above to record missing integration/Miri/sanitizer/device/benchmark/profile runs. Do not create or label a known-good baseline unless `performance/scripts/report_hardening.py --freeze --tests-passed` accepts real measured results. Any change lacking a measurable gain should be documented or reverted.
