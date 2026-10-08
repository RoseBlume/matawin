# P552-P574 backend / OS performance policy

`mata-playback` remains the default low-overhead backend; CPAL remains an opt-in comparison/fallback backend. Device capabilities are enumerated only when the engine starts or when the callback reports a device failure. The selected configuration and stream are cached in `OutputSession`.

On device failure, a new default device/configuration/ring is created and swapped into the mixer. Existing `DecodeProducer` instances are retained, so decoders, seek indexes and codec scratch are not reopened. Once source rates are known, a cold-path configuration pass prefers an output rate near the active-source average and still gives f32/native-format configurations the highest format preference.

Platform sizing defaults:
- Windows: 480-frame preferred period, ~120 ms ring.
- Linux direct/ALSA-style: 256-frame preferred period, ~120 ms ring.
- Linux PulseAudio/PipeWire hint: 512-frame preferred period, ~180 ms ring.
- Android: frames-per-burst from `MATA_AUDIO_ANDROID_FRAMES_PER_BURST` when supplied, otherwise 192; ring ~96 ms.

Real-time/affinity elevation is opt-in through `set_realtime_worker_tuning(true)` plus the installed worker hook. If no platform hook is installed the engine simply uses normal scheduling. Callback code itself remains allocation/locking/formatting free.

Subnormal PCM values are flushed to zero by the producer sanitizer; the math benchmark measures normal/subnormal arithmetic cost and reports how many subnormals were sanitized through playback telemetry.

## P552-P574 validation matrix

`benchmark_backends.sh` performs the mata-playback/CPAL A/B run on hosts where both backends are available. `platform_smoke.sh` executes the decoder/performance smoke suite on the native host and labels Windows x86_64, Linux x86_64, ARM64 and Android separately. `cross_target_smoke.sh` compile-checks the four target triples, while `.github/workflows/performance-smoke.yml` keeps Windows/Linux native and ARM64/Android compile-smoke coverage reproducible.

The playback engine caches its selected output configuration in `OutputSession`, prefers native `f32`, scores device sample rates against active source rates, and only re-enumerates on startup or cold-path device recovery. Windows, direct-Linux/ALSA, PulseAudio/PipeWire and Android each have explicit period/ring sizing policy. Android honors `MATA_AUDIO_ANDROID_FRAMES_PER_BURST` when supplied by the host integration. Output reconfiguration retains every source `DecodeProducer`, so decoder state is not reopened.

Real-time priority/affinity is opt-in through `set_realtime_worker_tuning` plus the installed worker-tuning hook. Failure or absence of a platform hook leaves normal scheduling intact instead of making playback startup fail. Denormals are measured by `benchmark_denormals.sh`/`mata-audio-math-perf`; playback's cold non-normal path flushes subnormals to zero and records the count in telemetry.
