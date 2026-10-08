//! Feeds back the input stream directly into the output stream.
//!
//! Assumes that the input and output devices can use the same stream configuration and share a
//! sample format.
//!
//! Uses a delay of `LATENCY_MS` milliseconds in case the default input and output streams are not
//! precisely synchronised.

use clap::Parser;
use mata_playback::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Error, ErrorKind, HostId, InputCallbackInfo, OutputCallbackInfo, SampleFormat,
    SizedSample, StreamConfig,
};
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapRb,
};

#[derive(Parser, Debug)]
#[command(version, about = "mata-playback feedback example", long_about = None)]
struct Opt {
    /// The input audio device to use
    #[arg(short, long, value_name = "IN")]
    input_device: Option<String>,

    /// The output audio device to use
    #[arg(short, long, value_name = "OUT")]
    output_device: Option<String>,

    /// Specify the delay between input and output
    #[arg(short, long, value_name = "DELAY_MS", default_value_t = 150.0)]
    latency: f32,

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

    // Find devices.
    let input_device = if let Some(device) = opt.input_device {
        let id = &device.parse().expect("failed to parse input device id");
        host.device_by_id(id)
    } else {
        host.default_input_device()
    }
    .expect("failed to find input device");

    let output_device = if let Some(device) = opt.output_device {
        let id = &device.parse().expect("failed to parse output device id");
        host.device_by_id(id)
    } else {
        host.default_output_device()
    }
    .expect("failed to find output device");

    println!("Using input device: \"{}\"", input_device.id()?);
    println!("Using output device: \"{}\"", output_device.id()?);

    // We'll try and use the same configuration between streams to keep it simple.
    let input_config = input_device.default_input_config()?;
    let output_config = output_device.default_output_config()?;
    assert_eq!(
        input_config.sample_format(),
        output_config.sample_format(),
        "input and output devices must share a sample format for this example"
    );

    match input_config.sample_format() {
        SampleFormat::I8 => run::<i8>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::I16 => run::<i16>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::I32 => run::<i32>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::I64 => run::<i64>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::U8 => run::<u8>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::U16 => run::<u16>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::U32 => run::<u32>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::U64 => run::<u64>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::F32 => run::<f32>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        SampleFormat::F64 => run::<f64>(
            &input_device,
            &output_device,
            input_config.into(),
            opt.latency,
        ),
        sample_format => panic!("Unsupported sample format '{sample_format}'"),
    }
}

fn run<T>(
    input_device: &Device,
    output_device: &Device,
    config: StreamConfig,
    latency_ms: f32,
) -> anyhow::Result<()>
where
    T: SizedSample + std::fmt::Debug + Send + 'static,
{
    // Create a delay in case the input and output devices aren't synced.
    let latency_frames = (latency_ms / 1_000.0) * config.sample_rate as f32;
    let latency_samples = latency_frames as usize * config.channels as usize;

    // The buffer to share samples
    let ring = HeapRb::<T>::new(latency_samples * 2);
    let (mut producer, mut consumer) = ring.split();

    // Pre-fill with silence equal to the length of the delay.
    for _ in 0..latency_samples {
        // The ring buffer has twice as much space as necessary to add latency here,
        // so this should never fail
        producer.try_push(T::EQUILIBRIUM).unwrap();
    }

    let input_data_fn = move |data: &[T], _: &InputCallbackInfo| {
        if producer.push_slice(data) < data.len() {
            eprintln!("output stream fell behind: try increasing latency");
        }
    };

    let output_data_fn = move |data: &mut [T], _: &OutputCallbackInfo| {
        let read = consumer.pop_slice(data);
        if read < data.len() {
            data[read..].fill(T::EQUILIBRIUM);
            eprintln!("input stream fell behind: try increasing latency");
        }
    };

    // Build streams.
    println!(
        "Attempting to build both streams with {} samples and `{config:?}`.",
        T::FORMAT
    );
    let input_stream = input_device.build_input_stream(config, input_data_fn, err_fn, None)?;
    let output_stream = output_device.build_output_stream(config, output_data_fn, err_fn, None)?;
    println!("Successfully built streams.");

    // Play the streams.
    println!("Starting the input and output streams with `{latency_ms}` milliseconds of latency.");
    input_stream.play()?;
    output_stream.play()?;

    // Run for 10 seconds before closing.
    println!("Playing for 10 seconds... ");
    std::thread::sleep(std::time::Duration::from_secs(10));
    drop(input_stream);
    drop(output_stream);
    println!("Done!");
    Ok(())
}

fn err_fn(err: Error) {
    match err.kind() {
        ErrorKind::DeviceChanged | ErrorKind::Xrun | ErrorKind::RealtimeDenied => {
            eprintln!("{err}")
        }
        _ => eprintln!("Stream error: {err}"),
    }
}
