use std::{
    env,
    error::Error,
    fs::File,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use cpal::{
    BufferSize,
    FromSample,
    SampleFormat,
    SizedSample,
    SupportedBufferSize,
    traits::{
        DeviceTrait,
        HostTrait,
        StreamTrait,
    },
};

use mata_audio::{
    AudioDecoder,
    AudioError,
    Decoder,
};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| {
            "audio/sample-1.mp3".to_string()
        });

    let file = File::open(&path)?;

    let mut decoder =
        AudioDecoder::new(file)?;

    let source_rate =
        decoder.sample_rate();

    let source_channels =
        decoder.channels() as usize;

    println!(
        "Decoding {path}: {source_rate} Hz, \
         {source_channels} channel(s)"
    );

    let mut pcm =
        Vec::<f32>::new();

    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                pcm.extend_from_slice(
                    &frame.samples,
                );
            }

            Err(AudioError::NoNextFrame) => {
                break;
            }

            Err(error) => {
                return Err(error.into());
            }
        }
    }

    if pcm.is_empty() {
        return Err(
            "Audio decoded to no PCM samples"
                .into(),
        );
    }

    /*
     * Inspect the decoder output.
     *
     * This is useful because if your decoders are producing huge
     * amplitudes or non-finite values, the problem is before CPAL.
     */
    let mut peak =
        0.0f32;

    let mut bad_samples =
        0usize;

    for &sample in &pcm {
        if sample.is_finite() {
            peak =
                peak.max(sample.abs());
        } else {
            bad_samples += 1;
        }
    }

    println!(
        "Decoded peak: {peak:.6}, \
         non-finite samples: {bad_samples}"
    );

    let host =
        cpal::default_host();

    let device =
        host.default_output_device()
            .ok_or(
                "No default audio output device",
            )?;

    let supported =
        device.default_output_config()?;

    let sample_format =
        supported.sample_format();

    let mut config =
        supported.config();

    /*
     * Ask for a reasonably large buffer.
     *
     * Don't blindly request 1024 if the device does not support it.
     */
    if let SupportedBufferSize::Range {
        min,
        max,
    } = *supported.buffer_size()
    {
        let buffer_size =
            1024u32.clamp(
                min,
                max,
            );

        config.buffer_size =
            BufferSize::Fixed(
                buffer_size,
            );
    }

    let output_rate =
        config.sample_rate;

    let output_channels =
        config.channels as usize;

    println!(
        "Output: {} Hz, {} channel(s), {:?}, {:?}",
        output_rate,
        output_channels,
        sample_format,
        config.buffer_size,
    );

    /*
     * Do ALL expensive transformation here, before the audio
     * callback starts.
     */
    let playback_pcm =
        prepare_pcm(
            &pcm,
            source_rate,
            source_channels,
            output_rate,
            output_channels,
        );

    let playback_pcm =
        Arc::new(playback_pcm);

    let done =
        Arc::new(
            AtomicBool::new(false),
        );

    let stream =
        match sample_format {
            SampleFormat::I8 => {
                build_stream::<i8>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::I16 => {
                build_stream::<i16>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::I32 => {
                build_stream::<i32>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::I64 => {
                build_stream::<i64>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::U8 => {
                build_stream::<u8>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::U16 => {
                build_stream::<u16>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::U32 => {
                build_stream::<u32>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::U64 => {
                build_stream::<u64>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::F32 => {
                build_stream::<f32>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            SampleFormat::F64 => {
                build_stream::<f64>(
                    &device,
                    config,
                    playback_pcm,
                    done.clone(),
                )?
            }

            format => {
                return Err(
                    format!(
                        "Unsupported CPAL sample format: {format}"
                    )
                    .into(),
                );
            }
        };

    stream.play()?;

    while !done.load(Ordering::Acquire) {
        thread::sleep(
            Duration::from_millis(10),
        );
    }

    /*
     * Let the backend finish playing the samples already queued
     * to the hardware.
     */
    thread::sleep(
        Duration::from_millis(250),
    );

    drop(stream);

    Ok(())
}

fn prepare_pcm(
    input: &[f32],
    source_rate: u32,
    source_channels: usize,
    output_rate: u32,
    output_channels: usize,
) -> Vec<f32> {
    assert!(source_channels > 0);
    assert!(output_channels > 0);

    let source_frames =
        input.len()
            / source_channels;

    if source_frames == 0 {
        return Vec::new();
    }

    let output_frames =
        (
            source_frames as u128
            * output_rate as u128
            / source_rate as u128
        ) as usize;

    let mut output =
        Vec::with_capacity(
            output_frames
                * output_channels,
        );

    let step =
        source_rate as f64
        / output_rate as f64;

    for output_frame
        in 0..output_frames
    {
        let source_position =
            output_frame as f64
            * step;

        let index0 =
            source_position.floor()
                as usize;

        let index0 =
            index0.min(
                source_frames - 1,
            );

        let index1 =
            (index0 + 1)
                .min(
                    source_frames - 1,
                );

        let fraction =
            (
                source_position
                - index0 as f64
            ) as f32;

        if output_channels == 1 {
            /*
             * Downmix all source channels to mono.
             */
            let mut a =
                0.0f32;

            let mut b =
                0.0f32;

            for channel
                in 0..source_channels
            {
                a += clean_sample(
                    input[
                        index0
                            * source_channels
                            + channel
                    ],
                );

                b += clean_sample(
                    input[
                        index1
                            * source_channels
                            + channel
                    ],
                );
            }

            a /=
                source_channels
                    as f32;

            b /=
                source_channels
                    as f32;

            output.push(
                clean_sample(
                    lerp(
                        a,
                        b,
                        fraction,
                    ),
                ),
            );
        } else {
            for output_channel
                in 0..output_channels
            {
                let source_channel =
                    if source_channels == 1
                    {
                        0
                    } else if output_channel
                        < source_channels
                    {
                        output_channel
                    } else {
                        /*
                         * Extra device channels receive silence rather
                         * than L/R/L/R/L/R duplication.
                         */
                        output.push(0.0);
                        continue;
                    };

                let a =
                    clean_sample(
                        input[
                            index0
                                * source_channels
                                + source_channel
                        ],
                    );

                let b =
                    clean_sample(
                        input[
                            index1
                                * source_channels
                                + source_channel
                        ],
                    );

                output.push(
                    clean_sample(
                        lerp(
                            a,
                            b,
                            fraction,
                        ),
                    ),
                );
            }
        }
    }

    output
}

#[inline(always)]
fn clean_sample(sample: f32) -> f32 {
    if !sample.is_finite() {
        return 0.0;
    }

    /*
     * FromSample's integer conversion range is:
     *
     *     -1.0 <= x < 1.0
     *
     * so don't allow exactly +1.0.
     */
    sample.clamp(
        -1.0,
        0.999_999_94,
    )
}

#[inline(always)]
fn lerp(
    a: f32,
    b: f32,
    t: f32,
) -> f32 {
    a + (b - a) * t
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    pcm: Arc<Vec<f32>>,
    done: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample
        + FromSample<f32>,
{
    let mut position =
        0usize;

    device.build_output_stream(
        config,

        move |
            output: &mut [T],
            _: &cpal::OutputCallbackInfo,
        | {
            let remaining =
                pcm.len()
                    .saturating_sub(
                        position,
                    );

            let count =
                remaining.min(
                    output.len(),
                );

            /*
             * PCM was already:
             *
             * - resampled
             * - channel mapped
             * - checked for NaN/Inf
             * - clamped to a safe range
             */
            for (
                dst,
                &src,
            ) in output[..count]
                .iter_mut()
                .zip(
                    &pcm[
                        position
                            ..position
                                + count
                    ],
                )
            {
                *dst =
                    T::from_sample_(
                        src,
                    );
            }

            /*
             * Fill the rest of the CPAL buffer with silence.
             */
            for dst
                in &mut output[count..]
            {
                *dst =
                    T::from_sample_(
                        0.0,
                    );
            }

            position +=
                count;

            if position
                >= pcm.len()
            {
                done.store(
                    true,
                    Ordering::Release,
                );
            }
        },

        |error| {
            eprintln!(
                "CPAL stream error: {error}"
            );
        },

        None,
    )
}