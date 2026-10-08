# mata-audio — P602–P651 final performance report

- Generated (UTC): 2026-10-08T00:15:17+00:00
- Status: **NOT VALIDATED**
- Source SHA-256 (manifest of source bytes): `b6f5065391b2f3684f5f4c8b965eb9b50876f5b408756761f6b58a5e99b08550`
- Corpus manifest SHA-256: `993fa94b8c4ebf0f318a413174168470ef5ea2857901dfcaf6c25c3a0ffdc868`
- Cargo: `unavailable`
- Rust: `unavailable`
- OS/CPU: `Linux-6.18.44-x86_64-with-glibc2.41` / ``
- RUSTFLAGS: ``
- Test gate asserted: **True**
- Compiler/feature/ISA identifiers: 0 combinations in CSV

## Recorded inputs

- Corpus entries: 15
- Benchmark observations: 0
- Device-backed observations: 0
- Hardware fingerprint: `/mnt/data/mata-audio-p602-p651-work/mata-audio/performance/results/hardware-current.md` (missing)
- Corpus manifest: `performance/corpus/manifest.tsv`
- Runtime benchmark CSV: `/mnt/data/mata-audio-p602-p651-work/mata-audio/performance/results/current.csv`
- Playback/device CSV: `/mnt/data/mata-audio-p602-p651-work/mata-audio/performance/results/device-current.csv`

## Performance and correctness budgets

See `performance/budgets.json`; thresholds are provisional until calibrated on pinned hardware.

| Corpus ID | real-time factor | allocations/frame | peak RSS KiB | startup ns | PCM error |
|---|---:|---:|---:|---:|---:|
| aiff_s16_stereo_44100 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| aiff_s24_mono_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_s16_stereo_44100 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_s24_mono_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| mpeg_layer1_silence_64k_44100 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| mpeg_layer2_192k_44100 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| mpeg_layer3_128k_44100 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| vorbis_96k_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| wav_s16_stereo_44100 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| wav_s24_mono_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_predictor_constant_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_predictor_fixed_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_predictor_lpc8_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_rice_order0_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |
| flac_rice_order4_48000 | unmeasured | unmeasured | unmeasured | unmeasured | unmeasured |

## Open validation gates

- missing measured ('decode', 'aiff_s16_stereo_44100', 'realtime_factor')
- missing measured ('decode', 'aiff_s16_stereo_44100', 'allocations_per_frame')
- missing measured ('decode', 'aiff_s16_stereo_44100', 'peak_rss')
- missing measured ('decode', 'aiff_s16_stereo_44100', 'open_latency')
- aiff_s16_stereo_44100: no decoded PCM checksum
- aiff_s16_stereo_44100: no reference error
- missing measured ('decode', 'aiff_s24_mono_48000', 'realtime_factor')
- missing measured ('decode', 'aiff_s24_mono_48000', 'allocations_per_frame')
- missing measured ('decode', 'aiff_s24_mono_48000', 'peak_rss')
- missing measured ('decode', 'aiff_s24_mono_48000', 'open_latency')
- aiff_s24_mono_48000: no decoded PCM checksum
- aiff_s24_mono_48000: no reference error
- missing measured ('decode', 'flac_s16_stereo_44100', 'realtime_factor')
- missing measured ('decode', 'flac_s16_stereo_44100', 'allocations_per_frame')
- missing measured ('decode', 'flac_s16_stereo_44100', 'peak_rss')
- missing measured ('decode', 'flac_s16_stereo_44100', 'open_latency')
- flac_s16_stereo_44100: no decoded PCM checksum
- flac_s16_stereo_44100: no reference error
- missing measured ('decode', 'flac_s24_mono_48000', 'realtime_factor')
- missing measured ('decode', 'flac_s24_mono_48000', 'allocations_per_frame')
- missing measured ('decode', 'flac_s24_mono_48000', 'peak_rss')
- missing measured ('decode', 'flac_s24_mono_48000', 'open_latency')
- flac_s24_mono_48000: no decoded PCM checksum
- flac_s24_mono_48000: no reference error
- missing measured ('decode', 'mpeg_layer1_silence_64k_44100', 'realtime_factor')
- missing measured ('decode', 'mpeg_layer1_silence_64k_44100', 'allocations_per_frame')
- missing measured ('decode', 'mpeg_layer1_silence_64k_44100', 'peak_rss')
- missing measured ('decode', 'mpeg_layer1_silence_64k_44100', 'open_latency')
- mpeg_layer1_silence_64k_44100: no decoded PCM checksum
- mpeg_layer1_silence_64k_44100: no reference error
- missing measured ('decode', 'mpeg_layer2_192k_44100', 'realtime_factor')
- missing measured ('decode', 'mpeg_layer2_192k_44100', 'allocations_per_frame')
- missing measured ('decode', 'mpeg_layer2_192k_44100', 'peak_rss')
- missing measured ('decode', 'mpeg_layer2_192k_44100', 'open_latency')
- mpeg_layer2_192k_44100: no decoded PCM checksum
- mpeg_layer2_192k_44100: no reference error
- missing measured ('decode', 'mpeg_layer3_128k_44100', 'realtime_factor')
- missing measured ('decode', 'mpeg_layer3_128k_44100', 'allocations_per_frame')
- missing measured ('decode', 'mpeg_layer3_128k_44100', 'peak_rss')
- missing measured ('decode', 'mpeg_layer3_128k_44100', 'open_latency')
- mpeg_layer3_128k_44100: no decoded PCM checksum
- mpeg_layer3_128k_44100: no reference error
- missing measured ('decode', 'vorbis_96k_48000', 'realtime_factor')
- missing measured ('decode', 'vorbis_96k_48000', 'allocations_per_frame')
- missing measured ('decode', 'vorbis_96k_48000', 'peak_rss')
- missing measured ('decode', 'vorbis_96k_48000', 'open_latency')
- vorbis_96k_48000: no decoded PCM checksum
- vorbis_96k_48000: no reference error
- missing measured ('decode', 'wav_s16_stereo_44100', 'realtime_factor')
- missing measured ('decode', 'wav_s16_stereo_44100', 'allocations_per_frame')
- missing measured ('decode', 'wav_s16_stereo_44100', 'peak_rss')
- missing measured ('decode', 'wav_s16_stereo_44100', 'open_latency')
- wav_s16_stereo_44100: no decoded PCM checksum
- wav_s16_stereo_44100: no reference error
- missing measured ('decode', 'wav_s24_mono_48000', 'realtime_factor')
- missing measured ('decode', 'wav_s24_mono_48000', 'allocations_per_frame')
- missing measured ('decode', 'wav_s24_mono_48000', 'peak_rss')
- missing measured ('decode', 'wav_s24_mono_48000', 'open_latency')
- wav_s24_mono_48000: no decoded PCM checksum
- wav_s24_mono_48000: no reference error
- missing measured ('decode', 'flac_predictor_constant_48000', 'realtime_factor')
- missing measured ('decode', 'flac_predictor_constant_48000', 'allocations_per_frame')
- missing measured ('decode', 'flac_predictor_constant_48000', 'peak_rss')
- missing measured ('decode', 'flac_predictor_constant_48000', 'open_latency')
- flac_predictor_constant_48000: no decoded PCM checksum
- flac_predictor_constant_48000: no reference error
- missing measured ('decode', 'flac_predictor_fixed_48000', 'realtime_factor')
- missing measured ('decode', 'flac_predictor_fixed_48000', 'allocations_per_frame')
- missing measured ('decode', 'flac_predictor_fixed_48000', 'peak_rss')
- missing measured ('decode', 'flac_predictor_fixed_48000', 'open_latency')
- flac_predictor_fixed_48000: no decoded PCM checksum
- flac_predictor_fixed_48000: no reference error
- missing measured ('decode', 'flac_predictor_lpc8_48000', 'realtime_factor')
- missing measured ('decode', 'flac_predictor_lpc8_48000', 'allocations_per_frame')
- missing measured ('decode', 'flac_predictor_lpc8_48000', 'peak_rss')
- missing measured ('decode', 'flac_predictor_lpc8_48000', 'open_latency')
- flac_predictor_lpc8_48000: no decoded PCM checksum
- flac_predictor_lpc8_48000: no reference error
- missing measured ('decode', 'flac_rice_order0_48000', 'realtime_factor')
- missing measured ('decode', 'flac_rice_order0_48000', 'allocations_per_frame')
- missing measured ('decode', 'flac_rice_order0_48000', 'peak_rss')
- missing measured ('decode', 'flac_rice_order0_48000', 'open_latency')
- flac_rice_order0_48000: no decoded PCM checksum
- flac_rice_order0_48000: no reference error
- missing measured ('decode', 'flac_rice_order4_48000', 'realtime_factor')
- missing measured ('decode', 'flac_rice_order4_48000', 'allocations_per_frame')
- missing measured ('decode', 'flac_rice_order4_48000', 'peak_rss')
- missing measured ('decode', 'flac_rice_order4_48000', 'open_latency')
- flac_rice_order4_48000: no decoded PCM checksum
- flac_rice_order4_48000: no reference error
- device-backed mixer/underrun/command/seek/loop metrics not recorded

## Tooling/limitations

- Stage profiles: `performance/scripts/profile_final.sh`
- Thermal/stability: `performance/scripts/benchmark_thermal.sh`
- Sanitizer/Miri: `performance/scripts/run_unsafe_validation.sh`
- Full integrated gate: `performance/scripts/validate_p602_p651.sh`
- A missing device-backed metric is a hard freeze blocker, not a fabricated zero.
