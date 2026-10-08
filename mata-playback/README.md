# mata-playback

`mata-playback` is a focused low-level audio input/output crate derived from CPAL 0.18.2. It keeps the CPAL-style device and stream API while limiting platform support to the backends requested for the Mata ecosystem.

## Supported platforms

| Platform | Backends |
| --- | --- |
| Linux | ALSA, PulseAudio |
| Windows | WASAPI through `windows` / `windows-core` |
| Android | AAudio through the existing NDK backend |

All other platform backends have been removed from the source tree. Unsupported targets fail at compile time.

## Features

```toml
[dependencies]
mata-playback = { path = "../mata-playback" }
```

PulseAudio is enabled by default on Linux, with ALSA always available. To build Linux with ALSA only:

```toml
mata-playback = { path = "../mata-playback", default-features = false }
```

Enable native realtime/low-latency behavior with:

```toml
mata-playback = { path = "../mata-playback", features = ["realtime"] }
```

The `realtime` feature adds **no Cargo dependencies**.

- **Linux / ALSA:** direct `hw:` streams attempt `SCHED_FIFO` promotion using the already-required `libc` API. If Linux denies realtime scheduling because of normal `rtprio`/permission limits, the stream reports `ErrorKind::RealtimeDenied` and continues without pretending promotion succeeded. ALSA plugin devices are intentionally not promoted because they may forward into a userspace sound server.
- **Windows / WASAPI:** the audio worker uses `SetThreadPriority(..., THREAD_PRIORITY_TIME_CRITICAL)` through the existing `windows` crate.
- **Android / AAudio:** the existing `AudioPerformanceMode::LowLatency` behavior is kept.

PulseAudio remains server-driven; the crate does not add an extra realtime scheduling dependency for it.

## Target-specific dependencies

Windows intentionally contains only `windows` and `windows-core` as Windows-specific dependencies. There is no ASIO or JACK support.

Linux contains only ALSA and PulseAudio as audio backends. The PulseAudio implementation still uses its existing support crates (`futures`, plus `portable-atomic` only on targets without native 64-bit atomics), and Linux uses `libc` for the ALSA worker and native realtime scheduling.

Android keeps the existing AAudio/NDK/JNI backend dependencies, except that realtime no longer pulls in `audio_thread_priority`.

## Basic playback

```rust,no_run
use mata_playback::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() -> Result<(), mata_playback::Error> {
    let host = mata_playback::default_host();
    let device = host
        .default_output_device()
        .ok_or(mata_playback::ErrorKind::DeviceNotAvailable)?;
    let supported = device.default_output_config()?;
    let config = supported.config();

    let stream = device.build_output_stream(
        config,
        |data: &mut [f32], _| data.fill(0.0),
        |err| eprintln!("audio error: {err}"),
        None,
    )?;

    stream.play()?;
    Ok(())
}
```

## Origin and license

This crate is based on CPAL 0.18.2 and retains the Apache-2.0 license from the upstream project. See `LICENSE`.
