use std::{
    env,
    error::Error,
    fs::File,
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            AtomicU32,
            AtomicUsize,
            Ordering,
        },
        mpsc,
    },
    thread,
    time::{Instant, Duration},
};

use cpal::{
    BufferSize,
    FromSample,
    I24,
    SampleFormat,
    SizedSample,
    SupportedBufferSize,
    SupportedStreamConfig,
    U24,
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

const RING_BUFFER_SECONDS: usize = 2;
const PREBUFFER_MILLISECONDS: usize = 300;
const OUTPUT_BUFFER_FRAMES: u32 = 1024;

#[derive(Debug, Clone, Copy)]
struct SourceInfo {
    sample_rate: u32,
    channels: u16,
}

struct PlaybackTarget {
    sample_rate: u32,
    channels: usize,
    ring: Arc<SpscF32Ring>,
    stop: Arc<AtomicBool>,
}

/// A safe, preallocated, lock-free SPSC ring buffer for normalized f32 PCM.
///
/// There is exactly:
///
/// - one producer: the decoder/resampler thread
/// - one consumer: the CPAL audio callback
///
/// Samples are stored as their raw f32 bit pattern in AtomicU32 cells. This
/// avoids a mutex and also avoids allocation/deallocation inside the audio
/// callback.
struct SpscF32Ring {
    slots: Box<[AtomicU32]>,
    capacity: usize,

    read_index: AtomicUsize,
    write_index: AtomicUsize,

    closed: AtomicBool,
}

impl SpscF32Ring {
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0);

        let slots = (0..capacity)
            .map(|_| AtomicU32::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            slots,
            capacity,
            read_index: AtomicUsize::new(0),
            write_index: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        }
    }

    #[inline]
    fn available_samples(&self) -> usize {
        let read = self.read_index.load(Ordering::Acquire);
        let write = self.write_index.load(Ordering::Acquire);

        write
            .wrapping_sub(read)
            .min(self.capacity)
    }

    #[inline]
    fn free_samples(&self) -> usize {
        self.capacity
            .saturating_sub(self.available_samples())
    }

    #[inline]
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    #[inline]
    fn is_drained(&self) -> bool {
        self.is_closed() && self.available_samples() == 0
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Push samples from the decoding thread.
    ///
    /// This is allowed to wait because it never runs on the real-time audio
    /// callback. Back-pressure prevents the decoder from buffering the whole
    /// file in memory.
    fn push_blocking(
        &self,
        input: &[f32],
        stop: &AtomicBool,
    ) -> bool {
        let mut offset = 0usize;

        while offset < input.len() {
            if stop.load(Ordering::Acquire) {
                return false;
            }

            let read = self.read_index.load(Ordering::Acquire);
            let write = self.write_index.load(Ordering::Relaxed);

            let used = write
                .wrapping_sub(read)
                .min(self.capacity);

            let free = self.capacity - used;

            if free == 0 {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            let count = free.min(input.len() - offset);

            for i in 0..count {
                let slot = write
                    .wrapping_add(i)
                    % self.capacity;

                self.slots[slot].store(
                    input[offset + i].to_bits(),
                    Ordering::Relaxed,
                );
            }

            self.write_index.store(
                write.wrapping_add(count),
                Ordering::Release,
            );

            offset += count;
        }

        true
    }

    /// Drain directly into CPAL's output slice.
    ///
    /// This performs no allocation, locking, decoding, or resampling.
    fn read_into<T>(&self, output: &mut [T]) -> usize
    where
        T: SizedSample + FromSample<f32>,
    {
        let read = self.read_index.load(Ordering::Relaxed);
        let write = self.write_index.load(Ordering::Acquire);

        let available = write
            .wrapping_sub(read)
            .min(self.capacity);

        let count = available.min(output.len());

        for i in 0..count {
            let slot = read
                .wrapping_add(i)
                % self.capacity;

            let sample = f32::from_bits(
                self.slots[slot].load(Ordering::Relaxed),
            );

            output[i] = T::from_sample_(sample);
        }

        if count != 0 {
            self.read_index.store(
                read.wrapping_add(count),
                Ordering::Release,
            );
        }

        count
    }
}

/// Stateful streaming sample-rate/channel converter.
///
/// The fractional source position and one trailing source frame are preserved
/// across calls, so interpolation remains continuous across decoder packet/frame
/// boundaries.
struct StreamingConverter {
    source_rate: u32,
    source_channels: usize,

    output_rate: u32,
    output_channels: usize,

    step: f64,

    // Interleaved source PCM waiting to be consumed.
    buffer: Vec<f32>,

    // Source-frame position relative to the first frame in `buffer`.
    position: f64,

    passthrough: bool,
}

impl StreamingConverter {
    fn new(
        source_rate: u32,
        source_channels: usize,
        output_rate: u32,
        output_channels: usize,
    ) -> Result<Self, String> {
        if source_rate == 0 || output_rate == 0 {
            return Err("Sample rate cannot be zero".to_string());
        }

        if source_channels == 0 || output_channels == 0 {
            return Err("Channel count cannot be zero".to_string());
        }

        Ok(Self {
            source_rate,
            source_channels,
            output_rate,
            output_channels,

            step: source_rate as f64
                / output_rate as f64,

            buffer: Vec::new(),
            position: 0.0,

            passthrough:
                source_rate == output_rate
                && source_channels == output_channels,
        })
    }

    fn push(
        &mut self,
        samples: &[f32],
    ) -> Result<Vec<f32>, String> {
        if samples.len() % self.source_channels != 0 {
            return Err(format!(
                "Decoder produced {} samples, which is not divisible by {} channels",
                samples.len(),
                self.source_channels,
            ));
        }

        if samples.is_empty() {
            return Ok(Vec::new());
        }

        if self.passthrough {
            return Ok(
                samples
                    .iter()
                    .copied()
                    .map(clean_sample)
                    .collect(),
            );
        }

        self.buffer.reserve(samples.len());

        self.buffer.extend(
            samples
                .iter()
                .copied()
                .map(clean_sample),
        );

        Ok(self.produce(false))
    }

    fn finish(&mut self) -> Vec<f32> {
        if self.passthrough || self.buffer.is_empty() {
            return Vec::new();
        }

        let frame_count =
            self.buffer.len() / self.source_channels;

        if frame_count == 0 {
            return Vec::new();
        }

        // Duplicate the final source frame so the last real frame can still
        // participate in interpolation.
        let start =
            (frame_count - 1) * self.source_channels;

        let final_frame =
            self.buffer[
                start
                    ..start + self.source_channels
            ]
            .to_vec();

        self.buffer.extend_from_slice(
            &final_frame,
        );

        self.produce(true)
    }

    fn produce(
        &mut self,
        finishing: bool,
    ) -> Vec<f32> {
        let frames =
            self.buffer.len() / self.source_channels;

        if frames < 2 {
            return Vec::new();
        }

        let estimated_output_frames =
            (
                (frames as f64
                    / self.step)
                    .ceil()
                    as usize
            )
            .max(1);

        let mut output =
            Vec::with_capacity(
                estimated_output_frames
                    * self.output_channels,
            );

        loop {
            let index0 =
                self.position.floor() as usize;

            let index1 =
                index0 + 1;

            if index1 >= frames {
                break;
            }

            let fraction =
                (self.position
                    - index0 as f64)
                    as f32;

            self.write_output_frame(
                &mut output,
                index0,
                index1,
                fraction,
            );

            self.position += self.step;

            if finishing
                && self.position
                    >= (frames - 1) as f64
            {
                break;
            }
        }

        // Everything strictly before floor(position) is no longer needed.
        // Keep the current interpolation base frame.
        let consumed_frames =
            self.position.floor() as usize;

        if consumed_frames != 0 {
            let consumed_samples =
                consumed_frames
                    .saturating_mul(
                        self.source_channels,
                    )
                    .min(self.buffer.len());

            self.buffer.drain(
                ..consumed_samples,
            );

            self.position -=
                consumed_frames as f64;
        }

        output
    }

    fn write_output_frame(
        &self,
        output: &mut Vec<f32>,
        index0: usize,
        index1: usize,
        fraction: f32,
    ) {
        if self.output_channels == 1 {
            // Downmix all source channels to mono.
            let mut a = 0.0f32;
            let mut b = 0.0f32;

            for channel in 0..self.source_channels {
                a += self.source_sample(
                    index0,
                    channel,
                );

                b += self.source_sample(
                    index1,
                    channel,
                );
            }

            a /= self.source_channels as f32;
            b /= self.source_channels as f32;

            output.push(
                clean_sample(
                    lerp(a, b, fraction),
                ),
            );

            return;
        }

        for output_channel in 0..self.output_channels {
            let value =
                if self.source_channels == 1 {
                    // Mono -> all output channels.
                    let a =
                        self.source_sample(
                            index0,
                            0,
                        );

                    let b =
                        self.source_sample(
                            index1,
                            0,
                        );

                    lerp(a, b, fraction)
                } else if output_channel
                    < self.source_channels
                {
                    // Preserve corresponding channels.
                    let a =
                        self.source_sample(
                            index0,
                            output_channel,
                        );

                    let b =
                        self.source_sample(
                            index1,
                            output_channel,
                        );

                    lerp(a, b, fraction)
                } else {
                    // Don't repeat L/R into arbitrary extra device channels.
                    0.0
                };

            output.push(
                clean_sample(value),
            );
        }
    }

    #[inline]
    fn source_sample(
        &self,
        frame: usize,
        channel: usize,
    ) -> f32 {
        self.buffer[
            frame * self.source_channels
                + channel
        ]
    }
}

fn main() -> Result<(), Box<dyn Error>> {

    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| {
            "audio/sample-1.m4a".to_string()
        });

    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or(
            "No default audio output device is available",
        )?;

    // The decoder is constructed INSIDE its worker thread. This is especially
    // useful for platform decoders such as the no-dependency Windows M4A path,
    // because all decoder state remains on the thread that created it.
    let (
        source_info_tx,
        source_info_rx,
    ) = mpsc::sync_channel::<Result<SourceInfo, String>>(1);

    let (
        target_tx,
        target_rx,
    ) = mpsc::sync_channel::<PlaybackTarget>(1);

    let worker_path = path.clone();

    let decoder_thread = thread::spawn(
        move || -> Result<(), String> {
            let file = File::open(
                &worker_path,
            )
            .map_err(|error| {
                format!(
                    "Failed to open {worker_path}: {error}"
                )
            })?;
            let start = Instant::now();
            let mut decoder =
                match AudioDecoder::new(file) {
                    Ok(decoder) => decoder,

                    Err(error) => {
                        let message =
                            error.to_string();

                        let _ =
                            source_info_tx.send(
                                Err(
                                    message.clone(),
                                ),
                            );

                        return Err(message);
                    }
                };
            let difference = start.elapsed();
            println!("Spawned decoder in: {:#?}", difference);

            let source = SourceInfo {
                sample_rate:
                    decoder.sample_rate(),

                channels:
                    decoder.channels(),
            };

            if source.sample_rate == 0
                || source.channels == 0
            {
                let message =
                    "Decoder reported an invalid sample rate or channel count"
                        .to_string();

                let _ =
                    source_info_tx.send(
                        Err(
                            message.clone(),
                        ),
                    );

                return Err(message);
            }

            source_info_tx
                .send(Ok(source))
                .map_err(|_| {
                    "Playback thread disappeared before decoder initialization completed"
                        .to_string()
                })?;

            let target =
                target_rx.recv()
                    .map_err(|_| {
                        "Playback target was not provided"
                            .to_string()
                    })?;

            let result =
                decode_into_ring(
                    &mut decoder,
                    source,
                    &target,
                );

            target.ring.close();

            result
        },
    );

    let source =
        match source_info_rx.recv()? {
            Ok(info) => info,

            Err(message) => {
                let _ = decoder_thread.join();

                return Err(
                    std::io::Error::other(
                        message,
                    )
                    .into(),
                );
            }
        };

    println!(
        "Decoding {path}: {} Hz, {} channel(s)",
        source.sample_rate,
        source.channels,
    );

    /*
     * Prefer the file's native sample rate whenever the output device supports
     * it. That avoids resampling completely in the common case.
     */
    let supported =
        choose_output_config(
            &device,
            source,
        )?;

    let sample_format =
        supported.sample_format();

    let mut config =
        supported.config();

    /*
     * Prefer a reasonably generous device buffer for stable playback.
     * If the host reports no configurable range, leave BufferSize::Default.
     */
    if let SupportedBufferSize::Range {
        min,
        max,
    } = supported.buffer_size()
    {
        let frames =
            OUTPUT_BUFFER_FRAMES
                .clamp(*min, *max);

        config.buffer_size =
            BufferSize::Fixed(
                frames,
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

    if output_rate
        == source.sample_rate
    {
        println!(
            "Sample-rate conversion: not required"
        );
    } else {
        println!(
            "Sample-rate conversion: {} Hz -> {} Hz",
            source.sample_rate,
            output_rate,
        );
    }

    let ring_capacity =
        output_rate as usize
            * output_channels
            * RING_BUFFER_SECONDS;

    let ring =
        Arc::new(
            SpscF32Ring::new(
                ring_capacity,
            ),
        );

    let done =
        Arc::new(
            AtomicBool::new(false),
        );

    let stop =
        Arc::new(
            AtomicBool::new(false),
        );

    let underflows =
        Arc::new(
            AtomicUsize::new(0),
        );

    target_tx.send(
        PlaybackTarget {
            sample_rate:
                output_rate,

            channels:
                output_channels,

            ring:
                ring.clone(),

            stop:
                stop.clone(),
        },
    )?;

    /*
     * Prebuffer before calling stream.play().
     *
     * This allows the decoding thread to get ahead of the audio device without
     * loading the entire file.
     */
    let prebuffer_frames =
        (
            output_rate as usize
            * PREBUFFER_MILLISECONDS
            / 1000
        )
        .max(1);

    let prebuffer_samples =
        prebuffer_frames
            * output_channels;

    while ring.available_samples()
        < prebuffer_samples.min(ring.capacity)
        && !ring.is_closed()
    {
        thread::sleep(
            Duration::from_millis(2),
        );
    }

    let stream =
        match sample_format {
            SampleFormat::I8 => {
                build_stream::<i8>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::I16 => {
                build_stream::<i16>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::I24 => {
                build_stream::<I24>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::I32 => {
                build_stream::<i32>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::I64 => {
                build_stream::<i64>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::U8 => {
                build_stream::<u8>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::U16 => {
                build_stream::<u16>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::U24 => {
                build_stream::<U24>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::U32 => {
                build_stream::<u32>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::U64 => {
                build_stream::<u64>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::F32 => {
                build_stream::<f32>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            SampleFormat::F64 => {
                build_stream::<f64>(
                    &device,
                    config,
                    ring.clone(),
                    done.clone(),
                    stop.clone(),
                    underflows.clone(),
                )?
            }

            format => {
                stop.store(
                    true,
                    Ordering::Release,
                );

                return Err(
                    format!(
                        "Unsupported CPAL output sample format: {format}"
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

    stop.store(
        true,
        Ordering::Release,
    );

    // `done` is set by the callback once the software ring buffer is drained.
    // Give the backend/device time to play the final buffer that was just
    // submitted before destroying the stream.
    thread::sleep(
        Duration::from_millis(100),
    );

    drop(stream);

    let worker_result =
        decoder_thread.join()
            .map_err(|_| {
                std::io::Error::other(
                    "Decoder thread panicked",
                )
            })?;

    if let Err(message) =
        worker_result
    {
        return Err(
            std::io::Error::other(
                message,
            )
            .into(),
        );
    }

    let underflow_count =
        underflows.load(
            Ordering::Acquire,
        );

    if underflow_count != 0 {
        eprintln!(
            "Warning: {underflow_count} audio-buffer underrun(s) occurred."
        );

        eprintln!(
            "If you still hear crackle, increase PREBUFFER_MILLISECONDS or RING_BUFFER_SECONDS."
        );
    }

    Ok(())
}

fn choose_output_config(
    device: &cpal::Device,
    source: SourceInfo,
) -> Result<
    SupportedStreamConfig,
    cpal::Error,
> {
    let default =
        device.default_output_config()?;

    let default_channels =
        default.channels();

    let default_format =
        default.sample_format();

    let mut best:
        Option<(
            i32,
            SupportedStreamConfig,
        )> = None;

    /*
     * First try the source's exact sample rate.
     *
     * Among exact-rate configurations:
     * - strongly prefer the default device channel count,
     * - then prefer an exact source-channel match,
     * - prefer f32,
     * - then prefer the device's default sample format.
     */
    for range
        in device.supported_output_configs()?
    {
        let format =
            range.sample_format();

        if !supported_pcm_format(format) {
            continue;
        }

        let channels =
            range.channels();

        let Some(candidate) =
            range.try_with_sample_rate(
                source.sample_rate,
            )
        else {
            continue;
        };

        let mut score =
            0i32;

        if channels
            == default_channels
        {
            score += 1000;
        }

        if channels
            == source.channels
        {
            score += 500;
        }

        if format
            == SampleFormat::F32
        {
            score += 100;
        }

        if format
            == default_format
        {
            score += 50;
        }

        let replace =
            best.as_ref()
                .map(
                    |(best_score, _)|
                    score > *best_score,
                )
                .unwrap_or(true);

        if replace {
            best =
                Some((
                    score,
                    candidate,
                ));
        }
    }

    Ok(
        best
            .map(|(_, config)| config)
            .unwrap_or(default),
    )
}

#[inline]
fn supported_pcm_format(
    format: SampleFormat,
) -> bool {
    matches!(
        format,
        SampleFormat::I8
            | SampleFormat::I16
            | SampleFormat::I24
            | SampleFormat::I32
            | SampleFormat::I64
            | SampleFormat::U8
            | SampleFormat::U16
            | SampleFormat::U24
            | SampleFormat::U32
            | SampleFormat::U64
            | SampleFormat::F32
            | SampleFormat::F64
    )
}

fn decode_into_ring(
    decoder: &mut AudioDecoder<File>,
    source: SourceInfo,
    target: &PlaybackTarget,
) -> Result<(), String> {
    let mut converter =
        StreamingConverter::new(
            source.sample_rate,
            source.channels as usize,
            target.sample_rate,
            target.channels,
        )?;

    loop {
        if target.stop.load(
            Ordering::Acquire,
        ) {
            return Ok(());
        }

        match decoder.next_frame() {
            Ok(frame) => {
                if frame.sample_rate
                    != source.sample_rate
                {
                    return Err(format!(
                        "The decoder changed sample rate mid-stream from {} Hz to {} Hz",
                        source.sample_rate,
                        frame.sample_rate,
                    ));
                }

                if frame.channels
                    != source.channels
                {
                    return Err(format!(
                        "The decoder changed channel count mid-stream from {} to {}",
                        source.channels,
                        frame.channels,
                    ));
                }

                let converted =
                    converter.push(
                        &frame.samples,
                    )?;

                if !converted.is_empty()
                    && !target.ring.push_blocking(
                        &converted,
                        &target.stop,
                    )
                {
                    return Ok(());
                }
            }

            Err(AudioError::NoNextFrame) => {
                break;
            }

            Err(error) => {
                return Err(
                    error.to_string(),
                );
            }
        }
    }

    let tail =
        converter.finish();

    if !tail.is_empty() {
        let _ =
            target.ring.push_blocking(
                &tail,
                &target.stop,
            );
    }

    Ok(())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring: Arc<SpscF32Ring>,
    done: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    underflows: Arc<AtomicUsize>,
) -> Result<
    cpal::Stream,
    cpal::Error,
>
where
    T: SizedSample
        + FromSample<f32>,
{
    let callback_ring =
        ring.clone();

    let callback_done =
        done.clone();

    let callback_stop =
        stop.clone();

    let callback_underflows =
        underflows.clone();

    device.build_output_stream(
        config,

        move |
            output: &mut [T],
            _: &cpal::OutputCallbackInfo,
        | {
            let count =
                callback_ring.read_into(
                    output,
                );

            /*
             * If the decoder has not produced enough data yet, fill the
             * remainder with silence. The callback never waits for the
             * decoding thread.
             */
            if count < output.len() {
                for sample
                    in &mut output[count..]
                {
                    *sample =
                        T::from_sample_(
                            0.0,
                        );
                }

                if !callback_ring.is_closed() {
                    callback_underflows.fetch_add(
                        1,
                        Ordering::Relaxed,
                    );
                }
            }

            if callback_ring.is_drained() {
                callback_done.store(
                    true,
                    Ordering::Release,
                );

                callback_stop.store(
                    true,
                    Ordering::Release,
                );
            }
        },

        move |error| {
            eprintln!(
                "CPAL stream error: {error}"
            );

            done.store(
                true,
                Ordering::Release,
            );

            stop.store(
                true,
                Ordering::Release,
            );
        },

        None,
    )
}

#[inline(always)]
fn clean_sample(
    sample: f32,
) -> f32 {
    if !sample.is_finite() {
        return 0.0;
    }

    /*
     * CPAL's integer conversions expect a normalized floating-point sample.
     * Keep the positive side slightly below exactly +1.0.
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
