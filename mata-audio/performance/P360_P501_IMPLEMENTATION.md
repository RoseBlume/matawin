# P360–P501 implementation map

The `todo.md` checkboxes represent source implementation completion. The full Cargo integration/performance matrix can be rerun with `performance/scripts/validate_p360_p501.sh` on a Rust-enabled host.

## P360–P379 — metadata parsing

- `src/metadata/support.rs` centralizes bounded length/range validation, fixed-tail reads, exact bounded allocation, absolute seeks, and caller-position preservation. Public metadata readers restore the caller's stream position, including the aggregate `metadata::read_all()` path.
- APE probing uses one bounded tail window to recognize APEv1/APEv2 around optional ID3v1 data and validates item/tag lengths before allocation. ID3v1 remains a single 128-byte tail read.
- ID3v2 uses inline synchsafe decoding, borrowed frame IDs, in-place unsynchronization removal, and bounded APIC/PIC payload handling. Text payloads are interpreted from frame slices instead of copied solely for iteration.
- ISO/MP4 metadata walks atom headers/extended sizes and seeks across media/unknown atoms rather than loading their payloads. Item keys use borrowed/static names where possible and metadata/picture lengths are bounded before copying.
- RIFF metadata reuses scratch for INFO/bext/iXML traversal. FLAC/Ogg Vorbis-comment readers share `vorbis_common::CommentIter`, keeping comment iteration over byte slices and only creating owned compatibility fields at the API boundary.
- `Metadata::as_view()` exposes `MetadataView`, `MetadataFieldRef`, and `PictureRef` borrowed views while preserving the existing owned `Metadata` API.
- `mata-audio-metadata-perf` benchmarks each enabled parser and `read_all`; `mata-audio-metadata-fuzz` deterministically mutates malformed length/count data. `run_metadata_matrix.sh` runs both tools per metadata feature and `performance/metadata/thresholds.tsv` records regression budgets.

## P380–P395 — Cargo feature matrix and code-size control

- `check_feature_matrix.sh` checks no-default, every individual codec/demuxer/metadata feature, aggregate groups, scalar/SIMD codec variants, auto-detect, both playback backends, and defaults.
- `verify_disabled_api.sh` builds a temporary no-default consumer and verifies disabled feature APIs fail to resolve. `tests/default_feature_surface.rs` verifies that the default feature closure exposes every supported component family.
- `measure_feature_costs.sh` records isolated/aggregate check time and release rlib size in a machine-readable CSV, covering feature compile-time and code-size attribution.
- `performance/FEATURE_COMBINATIONS.md` documents supported minimal combinations and dependency edges. `.github/workflows/feature-matrix.yml` supplies representative no-default, scalar, SIMD and default CI jobs.
- `check_feature_surface.py` performs source-level feature-surface validation even when Cargo is unavailable.

## P396–P477 — playback, mixer, resampling and real-time behavior

- `Mixer` retains its mix/failure buffers and clears only the active mix span. `VoiceStore` uses packed slots plus generation keys; a separate `HashMap` is only the ID-to-slot lookup, so hot mixing iterates contiguous slots and active voice count is maintained explicitly.
- Commands are bounded per mixer iteration. The central worker uses command-driven `recv_timeout` waiting instead of fixed 1 ms polling, and ring fill determines when work is produced.
- Mix-ahead grows after underruns and decays toward a bounded baseline. Mix block size adapts to active-voice count and the selected device period.
- f32 output is preferred. The SPSC ring has cache-line-padded indices, acquire/release SPSC ordering, at-most-two contiguous spans, bulk f32 copy, and specialized SIMD f32→i16 conversion. Other output formats keep a generic bulk callback conversion path.
- Real-time callbacks allocate nothing, take no mutex, perform no I/O/log formatting, and publish errors through `RT_ERROR_CODE`; non-real-time worker code converts that status into a user-facing error.
- `SourceFastState` stores hot playing/paused/stopped/looping, gain and position snapshots atomically. Playback position publishing is throttled to roughly 30 Hz while command state changes publish immediately.
- Resampling uses a cached Q32 phase/step, avoiding per-frame `f64::floor`. Equal-rate/same-layout voices use a contiguous SIMD gain+mix fast path. Mono→mono, mono→stereo, stereo→mono and stereo→stereo are specialized; generic layouts use a precomputed routing matrix. Interpolation index/fraction batches are generated once per output block and interpolation, channel mapping, gain and mix are fused in the common path.
- Voice staging uses a head index plus threshold `copy_within` compaction and never drains consumed prefixes. Decoder demand is published to each producer so worker refills scale to the requested deficit, and low-water prefetch schedules decode before the queue empties.
- Each voice owns a bounded decoded-PCM SPSC producer. The global decoder pool shards jobs across one bounded atomic SPSC job ring per worker; central-mixer submission uses only atomics plus `Thread::unpark`, avoiding the former `sync_channel` lock path. The pool is initialized before steady-state mixing. `set_background_decode(false)` retains an inline decode option. The optional worker tuning hook can apply OS-specific priority/affinity policy without adding platform dependencies; `usize::MAX` identifies the central audio worker and decoder workers use their zero-based index.
- Gain is cached in vector-friendly form. SIMD equal-rate mix-add and final hard clamp are available; conservative peak tracking skips the limiter when safe, and limiter quality is selectable (`Disabled`, `HardClip`, `Soft`).
- `PlaybackTelemetry` exposes cumulative mixer/decoder CPU nanoseconds, decoded/output ring fill, underruns and callback period. Buffer tuning reacts automatically to underrun history.

## P478–P501 — seeking, looping and timeline efficiency

- `Decoder` has optional native `seek_to_sample`, `seek_to_seconds` and reset capability. `AudioDecoder` feature-gates dispatch so disabled codecs do not contribute seek branches.
- WAV/AIFF use exact byte/frame seeking, FLAC uses SEEKTABLE with its lazy frame-index fallback, MPEG uses its bounded sparse index with Layer III preroll, and Vorbis uses its Ogg granule/page index.
- `DecodeProducer::seek` retains decoder/output/ring allocations and uses native codec state. Indexed codecs may return an earlier synchronization point; only the bounded gap is decoded and the unconsumed frame tail is queued, making the externally observed seek exact without reopening or decoding from stream start.
- Stop-to-zero and looping route through the same native seek path. `Voice` buffers and producer/decoder scratch retain capacity across seek/reset.
- Whole-stream looping captures a bounded exact PCM prefix. At a loop boundary the decoder seeks to the frame immediately after that prefix while the prefix is replayed from retained PCM, avoiding a reopen/redecode stall at the boundary.
- `tests/seek_timeline.rs` covers repeated native seek bounds, exact PCM repeat after seek, and long-duration Q32 timeline drift. `mata-audio-seek-perf` performs deterministic repeated-random seek timing across WAV, AIFF, FLAC, MPEG Layer III and Vorbis; `run_seek_validation.sh` is the one-command runner on a Rust-enabled host.
