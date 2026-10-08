# P602–P651 hardening implementation map

**Scope:** deterministic test and verification infrastructure, not a claim that all integrations have been executed.

| Phases | Implementation / executable entry point |
|---|---|
| P602–P603 | `tests/hardening_decode.rs`: corpus-versus-independent-reference PCM checks, digest and exact PCM comparison; `performance/scripts/run_legacy_comparison.sh` plus `compare_legacy_pcm.py` perform direct old-versus-new binary differential; `performance/corpus/manifest.tsv` pins source/reference SHA-256 |
| P604 | `src/codecs/flac/bit_reader.rs`: seeded MSB-first 0–64-bit differential tests, unary/seek cases |
| P605 | `src/codecs/vorbis/bit_reader.rs`: seeded LSB-first offsets, 64-bit and EOF cases |
| P606 | `src/codecs/support/buffer.rs`: 12,000 random byte-window operations versus naive `Vec` reference |
| P607–P610 | `tests/hardening_decode.rs`: decoder buffer sizes, repeated partial decode/reset/seek/loop, bounded mutation input |
| P611–P617 | `src/playback.rs`: wraparound SPSC, concurrent 200k samples, 128 queue synthetic underrun/EOF stress; `performance/scripts/validate_p602_p651.sh`: real device/worker test stage. These tests do not prove scheduling guarantees on a real device. |
| P618–P619 | `performance/scripts/run_unsafe_validation.sh`: conditional Miri and ASan/UBSan paths, honest skip reporting |
| P620–P623 | `src/simd/mod.rs` existing dispatch-kernel tests plus exhaustive short/tail/unaligned/nonfinite randomized cases |
| P624 | `tests/hardening_decode.rs`: corrupted length and size fields, bounded allocation checks |
| P625–P626 | Existing `tests/seek_timeline.rs` Q32 eight-hour source/output drift and seek/loop tests; full corpus seek reset stress extended in `tests/hardening_decode.rs` |
| P627 | `performance/scripts/benchmark_thermal.sh` sustained >=60s repeated release workloads, CSV over iterations |
| P628–P631 | `performance/scripts/profile_final.sh` and existing per-codec/profile scripts; `rank_hotspots.py` requires actual sampled symbols |
| P632–P639 | `performance/budgets.json` metric limits and `src/bin/mata-audio-device-perf.rs` physical telemetry; budget gate in `report_hardening.py` |
| P640–P645 | `performance/scripts/report_hardening.py` aggregates host compiler/flags/features/ISA, corpus hashes, decode timing/RSS/allocations, device files into Markdown |
| P646–P647 | `performance/CHANGELOG.md`, `performance/benchmark-notes/TEMPLATE.md`, `check_benchmark_notes.py` |
| P648 | Production instrumentation stays behind `perf-instrumentation` feature; checked by static validation |
| P649–P650 | `profile_final.sh`, `rank_hotspots.py`, generated follow-on top-five proposals contingent on profiler samples |
| P651 | `performance/BASELINE_FREEZE.md` and guarded `report_hardening.py --freeze --tests-passed`; no fabricated known-good baseline |

## Execution status

Source/static checks and reference-file SHA-256 can be performed in this environment. Cargo/rustc are absent, as is the sibling `../mata-playback` dependency inside the ZIP; consequently the Rust integration tests, Miri/sanitizers, host-specific performance results, physical callback metrics and final sampled profiler runs are **not claimed as executed**. P602–P651 checkbox status follows the user's previous implementation-complete convention; the verified baseline lockfile remains pending until a measured run passes the enforced gate.
