# P217–P359 implementation map

The `todo.md` checkboxes represent implementation completion. The full Cargo integration/performance suite can be rerun with `performance/scripts/validate_p217_p359.sh` on a Rust-enabled host.

## P217–P269 — MPEG Audio Layer I/II/III

- The shared MPEG decoder and container-stream decoder retain `mp3dec_scratch_t` work areas instead of rebuilding the large Layer III granule/scalefactor/synthesis arrays for every frame. `MpegAudioStream::next_available_into` provides an allocation-conscious container path; the owned compatibility API is layered on top.
- The existing `ByteWindow` path remains head-indexed, threshold-compacted, geometrically grown, and filled through spare `Vec` capacity. MPEG-specific low/high watermarks and a 32 KiB read block provide lookahead without prefix drains.
- Leading ID3v2 tags are skipped before frame search using their synchsafe length. At EOF, bounded ID3v1 and APE footer recognition trims common trailing tags before frame scanning.
- Probe now runs through the persistent decoder/scratch and consumes only leading junk, leaving the first frame synchronized for the low-level next-header fast path.
- Stream sample rate, channels, and MPEG layer are cached and validated against subsequent frames.
- The Layer III core retains its bounded Huffman cache/refill path, table-base lookup, linbits/count1 specializations, prevalidated side-info/scalefactor tables, short-block reorder, antialias, IMDCT12/36, reservoir, DCT-II and synthesis specializations. The SIMD engine keeps runtime-dispatched mid/side, intensity stereo, antialias, sign/change and synthesis-window kernels.
- The normal Layer III pow(4/3) path now uses a process-wide lazily populated table covering all valid Huffman+linbits magnitudes (0–8206), generated from the decoder's existing approximation so output semantics remain consistent. Malformed out-of-range magnitudes retain the defensive approximation fallback.
- The SIMD MPEG core batches scalefactor application over decoded big-value spectral bands with AVX/NEON-capable dispatch instead of multiplying every decoded coefficient through the scalar hot loop.
- MPEG stage timers cover scalefactors, Huffman, reorder, IMDCT, reservoir restore/save, synthesis DCT and final synthesis when `perf-instrumentation` is enabled.
- `MpegSeekPoint` builds a bounded sparse byte/sample index every 32 decoded frames. Layer III seeks preroll several learned points to rebuild reservoir/overlap state. Reset retains `ByteWindow` and index capacity.
- The performance corpus already contains Layer I, II and III streams, so the main harness records per-layer decode/startup/allocation metrics.

## P270–P339 — Vorbis codec + Ogg demuxer

- The former O(N²) IMDCT is replaced by a reusable radix-2 FFT-backed DCT-IV plan. Small/large plans precompute bit-reversal and twiddle tables and retain all real/imaginary/lane scratch. Independent equal-twiddle butterflies use the root AVX2/NEON dispatcher.
- Both Vorbis window slopes are precomputed at decoder construction.
- Floor 0/1 state is retained. Floor-1 low/high neighbors and sorted render order are setup-time data; inverse-dB values are cached process-wide; floor-0 Bark/cosine mappings are setup-time data.
- Residue decoding uses flat channel-major arenas, retained classifications/combined type-2 scratch, precomputed setup book/pass tables, direct type 0/1/2 loops, and borrowed contiguous codebook vectors.
- Codebooks are flattened after setup and use an 8-bit first-level table plus optional second-level tables before falling back to tree traversal for long codewords. The LSB-first reservoir supports peek/skip so fast decoding avoids bit-at-a-time traversal on common codes.
- Coupling, floor multiply, overlap/window, stereo interleave and FFT butterflies route through root SIMD dispatch. Time-domain channels, overlap tails and final interleaved output remain allocated across packets/resets.
- Ogg CRC is updated incrementally over the page header, lacing values and body. Header/lacing/body/partial-packet buffers are reused. Complete page-local packets are exposed by borrowed slices; spanning packets recycle retained completion storage.
- Ogg keeps a bounded page/granule index and Vorbis translates sample seeks into indexed granule seeks with preroll. Reset verifies the three headers but reuses the already parsed comment/setup structures and decoder scratch.
- Stage timing records whole Vorbis packets, small/large IMDCT, residue types 0/1/2 and codebook decoding. These labels make the existing Vorbis corpus usable for the P335–P339 micro/stage regressions without adding benchmark-only decoder behavior.

## P340–P359 — demuxers

- A common `DemuxerSeek`/`SeekCapability` interface describes exact vs indexed/approximate seeking while keeping format-specific seek APIs available.
- WAV and AIFF use bounded chunk headers, seek over irrelevant payloads, cache data/SSND bounds and format invariants, and preserve even-byte chunk alignment. RF64 data length handling remains allocation-free for skipped chunks.
- FLAC metadata traversal bounds payload lengths and seeks over padding/unneeded blocks rather than retaining them. SEEKTABLE points power its indexed seek capability.
- Ogg uses fixed header storage plus retained lacing/body/continued-packet buffers and borrowed complete packets.
- Matroska/WebM variable integers are decoded using one fixed 8-byte scratch read rather than byte-at-a-time temporary reads. Unknown elements are seek-skipped, large leaf payloads are bounded, unknown-size handling is limited to Segment/Cluster streaming masters, and a bounded learned Cluster index implements approximate timeline seeking.
- `mata-audio-demux-perf` records demux-only startup and sequential throughput for WAV, AIFF, FLAC, Ogg and Matroska. `performance/demuxer_thresholds.tsv` provides per-demuxer regression gates.
- `mata-audio-demux-fuzz` deterministically mutates every demuxer corpus input with truncations, bit/size corruption and seek calls. It is dependency-free and intended as a fast malformed-input smoke fuzzer; deeper sanitizer/libFuzzer work remains in the final validation tranche.
