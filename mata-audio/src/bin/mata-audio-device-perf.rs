//! Physical-output P635/P636 probe. Requires a working playback device.
//! Records only measured fields; command-to-audible latency and loop gap need
//! external loopback equipment to be recorded separately.
use mata_audio::AudioSource;
use std::{env,fs,thread,time::{Duration,Instant}};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args=env::args().skip(1).collect::<Vec<_>>();
    let path=args.first().cloned().unwrap_or_else(||"performance/corpus/wav_s16_stereo_44100.wav".into());
    let duration=args.get(1).and_then(|s|s.parse::<u64>().ok()).unwrap_or(180).max(30);
    let out=args.get(2).cloned().unwrap_or_else(||"performance/results/device-current.csv".into());
    let source=AudioSource::open(path,true)?;source.set_looping(true);
    thread::sleep(Duration::from_millis(750));
    let start=source.telemetry();let t=Instant::now();thread::sleep(Duration::from_secs(duration));
    let end=source.telemetry();let elapsed=t.elapsed().as_secs_f64();source.stop();
    let cpu=end.mixer_cpu_nanoseconds.saturating_sub(start.mixer_cpu_nanoseconds) as f64/(elapsed*1e9)*100.0;
    let underruns=end.underruns.saturating_sub(start.underruns) as f64*3600.0/elapsed;
    if let Some(parent)=std::path::Path::new(&out).parent(){fs::create_dir_all(parent)?;}
    fs::write(&out,format!("kind,id,metric,value,unit\nplayback,physical,mixer_cpu_percent,{cpu:.4},percent\nplayback,physical,underrun_events_per_hour,{underruns:.4},events/hour\n"))?;
    println!("Recorded mixer CPU and hardware underruns: {out}. External loopback measurements are still required before freezing.");
    Ok(())
}
