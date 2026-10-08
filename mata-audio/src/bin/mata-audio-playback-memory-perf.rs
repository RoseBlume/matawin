//! Playback retained/high-water memory probe for P506/P521/P522.
//! Requires a working playback device; it is intentionally separate from the
//! decoder-only memory harness so headless CI can still run P502-P520.

use mata_audio::AudioSource;
use std::{path::PathBuf, thread, time::{Duration, Instant}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().unwrap_or_else(|| "performance/corpus/wav_s16_stereo_44100.wav".into()));
    let seconds = args.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60);
    let voices = args.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 128);
    let output = PathBuf::from(args.next().unwrap_or_else(|| "performance/results/playback-memory.csv".into()));
    if let Some(parent) = output.parent() { std::fs::create_dir_all(parent)?; }

    let mut sources = Vec::with_capacity(voices);
    for _ in 0..voices {
        let source = AudioSource::open(&path, true)?;
        source.set_looping(true);
        source.set_volume(1.0 / voices as f32);
        sources.push(source);
    }

    // Let open commands and initial decoder fills settle before defining the
    // steady-state baseline. This does not hide later growth: high-water bytes
    // are process-global and monotonic for the lifetime of the engine.
    thread::sleep(Duration::from_millis(750));
    let started = Instant::now();
    let mut baseline = None;
    let mut max_retained = 0usize;
    let mut max_high = 0usize;
    let mut rows = String::from("elapsed_seconds,voices,retained_bytes,high_water_bytes,ring_fill_samples,underruns\n");
    while started.elapsed() < Duration::from_secs(seconds.max(1)) {
        thread::sleep(Duration::from_secs(1));
        let telemetry = sources[0].telemetry();
        baseline.get_or_insert(telemetry.voice_retained_bytes);
        max_retained = max_retained.max(telemetry.voice_retained_bytes);
        max_high = max_high.max(telemetry.voice_high_water_bytes);
        rows.push_str(&format!("{:.3},{voices},{},{},{},{}\n",
            started.elapsed().as_secs_f64(), telemetry.voice_retained_bytes,
            telemetry.voice_high_water_bytes, telemetry.ring_fill_samples,
            telemetry.underruns));
    }
    std::fs::write(&output, rows)?;
    let baseline = baseline.unwrap_or(max_retained);
    let growth = max_retained.saturating_sub(baseline);
    println!("voices={voices} baseline_retained_bytes={baseline} max_retained_bytes={max_retained} high_water_bytes={max_high} growth_bytes={growth}");
    println!("wrote {}", output.display());
    Ok(())
}
