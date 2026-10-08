//! Records a WAV file (roughly 3 seconds long) using the default input device and config.
//!
//! The input data is recorded to "$CARGO_MANIFEST_DIR/recorded.wav".

use std::{
    fs::File,
    io::BufWriter,
    sync::{Arc, Mutex},
};

use clap::Parser;
use mata_playback::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Error, ErrorKind, FromSample, HostId, Sample, SampleFormat, SupportedStreamConfig,
};

#[derive(Parser, Debug)]
#[command(version, about = "mata-playback record_wav example", long_about = None)]
struct Opt {
    /// The audio device to use.
    #[arg(short, long)]
    device: Option<String>,

    /// How long to record, in seconds
    #[arg(long, default_value_t = 3)]
    duration: u64,

    /// Use the PulseAudio host explicitly on Linux.
    #[arg(long, default_value_t = false)]
    pulseaudio: bool,
}

fn main() -> Result<(), anyhow::Error> {
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

    // Set up the input device and stream with the default input config.
    let device = if let Some(device) = opt.device {
        let id = &device.parse().expect("failed to parse input device id");
        host.device_by_id(id)
    } else {
        host.default_input_device()
    }
    .expect("failed to find input device");

    println!("Input device: {}", device.id()?);

    let config = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .expect("Failed to get default input/output config");
    println!("Default input/output config: {config:?}");

    // The WAV file we're recording to.
    const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/recorded.wav");
    let spec = wav_spec_from_config(&config);
    let writer = hound::WavWriter::create(PATH, spec)?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    // A flag to indicate that recording is in progress.
    println!("Begin recording...");

    // Run the input stream on a separate thread.
    let writer_2 = writer.clone();

    let err_fn = move |err: Error| match err.kind() {
        ErrorKind::DeviceChanged | ErrorKind::Xrun | ErrorKind::RealtimeDenied => {
            eprintln!("{err}")
        }
        _ => eprintln!("Stream error: {err}"),
    };

    let stream = match config.sample_format() {
        SampleFormat::I8 => device.build_input_stream(
            config.into(),
            move |data, _: &_| write_input_data::<i8, i8>(data, &writer_2),
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            config.into(),
            move |data, _: &_| write_input_data::<i16, i16>(data, &writer_2),
            err_fn,
            None,
        )?,
        SampleFormat::I32 => device.build_input_stream(
            config.into(),
            move |data, _: &_| write_input_data::<i32, i32>(data, &writer_2),
            err_fn,
            None,
        )?,
        SampleFormat::F32 => device.build_input_stream(
            config.into(),
            move |data, _: &_| write_input_data::<f32, f32>(data, &writer_2),
            err_fn,
            None,
        )?,
        sample_format => {
            return Err(anyhow::Error::msg(format!(
                "Unsupported sample format '{sample_format}'"
            )))
        }
    };

    stream.play()?;

    // Let recording go for roughly three seconds.
    std::thread::sleep(std::time::Duration::from_secs(opt.duration));
    drop(stream);
    writer.lock().unwrap().take().unwrap().finalize()?;
    println!("Recording {PATH} complete!");
    Ok(())
}

fn sample_format(format: SampleFormat) -> hound::SampleFormat {
    if format.is_dsd() {
        panic!("DSD formats cannot be written to WAV files");
    } else if format.is_float() {
        hound::SampleFormat::Float
    } else {
        hound::SampleFormat::Int
    }
}

fn wav_spec_from_config(config: &SupportedStreamConfig) -> hound::WavSpec {
    hound::WavSpec {
        channels: config.channels() as _,
        sample_rate: config.sample_rate() as _,
        bits_per_sample: (config.sample_format().sample_size() * 8) as _,
        sample_format: sample_format(config.sample_format()),
    }
}

type WavWriterHandle = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

fn write_input_data<T, U>(input: &[T], writer: &WavWriterHandle)
where
    T: Sample,
    U: Sample + hound::Sample + FromSample<T>,
{
    if let Ok(mut guard) = writer.try_lock() {
        if let Some(writer) = guard.as_mut() {
            for &sample in input.iter() {
                let sample: U = U::from_sample(sample);
                writer.write_sample(sample).ok();
            }
        }
    }
}
