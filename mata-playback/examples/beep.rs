//! Plays a simple 440 Hz sine wave (beep) tone.
//!
//! This example demonstrates:
//! - Selecting ALSA/PulseAudio hosts on Linux
//! - Selecting devices by ID or using the default output device
//! - Querying the default output configuration
//! - Building and running an output stream with typed samples
//! - Generating audio data in the stream callback
//!
//! Run with: `cargo run --example beep`
//! With PulseAudio explicitly: `cargo run --example beep -- --pulseaudio`
//! With specific device: `cargo run --example beep -- --device "wasapi:device_id"`

use clap::Parser;
use mata_playback::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Error, ErrorKind, FromSample, HostId, OutputCallbackInfo, Sample, SampleFormat,
    SizedSample, StreamConfig, I24,
};

#[derive(Parser, Debug)]
#[command(version, about = "mata-playback beep example", long_about = None)]
struct Opt {
    /// The audio device to use
    #[arg(short, long)]
    device: Option<String>,

    /// Use the PulseAudio host explicitly on Linux.
    #[arg(long, default_value_t = false)]
    pulseaudio: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let host = if opt.pulseaudio {
        #[cfg(all(target_os = "linux", feature = "pulseaudio"))]
        {
            mata_playback::host_from_id(HostId::PulseAudio)?
        }
        #[cfg(not(all(target_os = "linux", feature = "pulseaudio")))]
        {
            anyhow::bail!("PulseAudio requires Linux and the `pulseaudio` feature");
        }
    } else {
        mata_playback::default_host()
    };

    let device = if let Some(device) = opt.device {
        let id = &device.parse().expect("failed to parse device id");
        host.device_by_id(id)
    } else {
        host.default_output_device()
    }
    .expect("failed to find output device");
    println!("Output device: {}", device.id()?);

    let config = device.default_output_config().unwrap();
    println!("Default output config: {config:?}");

    match config.sample_format() {
        SampleFormat::I8 => run::<i8>(&device, config.into()),
        SampleFormat::I16 => run::<i16>(&device, config.into()),
        SampleFormat::I24 => run::<I24>(&device, config.into()),
        SampleFormat::I32 => run::<i32>(&device, config.into()),
        // SampleFormat::I48 => run::<I48>(&device, config.into()),
        SampleFormat::I64 => run::<i64>(&device, config.into()),
        SampleFormat::U8 => run::<u8>(&device, config.into()),
        SampleFormat::U16 => run::<u16>(&device, config.into()),
        // SampleFormat::U24 => run::<U24>(&device, config.into()),
        SampleFormat::U32 => run::<u32>(&device, config.into()),
        // SampleFormat::U48 => run::<U48>(&device, config.into()),
        SampleFormat::U64 => run::<u64>(&device, config.into()),
        SampleFormat::F32 => run::<f32>(&device, config.into()),
        SampleFormat::F64 => run::<f64>(&device, config.into()),
        sample_format => panic!("Unsupported sample format '{sample_format}'"),
    }
}

pub fn run<T>(device: &Device, config: StreamConfig) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate as f32;
    let channels = config.channels as usize;

    // Produce a sinusoid of maximum amplitude.
    let mut sample_clock = 0f32;
    let mut next_value = move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        (sample_clock * 440.0 * 2.0 * std::f32::consts::PI / sample_rate).sin()
    };

    let err_fn = |err: Error| match err.kind() {
        ErrorKind::DeviceChanged | ErrorKind::Xrun | ErrorKind::RealtimeDenied => {
            eprintln!("{err}")
        }
        _ => eprintln!("Stream error: {err}"),
    };

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &OutputCallbackInfo| write_data(data, channels, &mut next_value),
        err_fn,
        None,
    )?;
    stream.play()?;

    std::thread::sleep(std::time::Duration::from_millis(1000));

    Ok(())
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> f32)
where
    T: Sample + FromSample<f32>,
{
    for frame in output.chunks_mut(channels) {
        let value: T = T::from_sample(next_sample());
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
