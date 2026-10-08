//! Demonstrates the video cpal smooth MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use std::{
    collections::VecDeque,
    fs::File,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, SampleFormat, SizedSample,
};

use matavideo::{
    Decoder,
    VideoError,
    VideoFrame,
    decoders::{Mjpeg, mjpeg::ThreadedMjpeg},
};

use matawin::{
    Window,
    colors::BLACK,
    events::{Key, WindowEvent},
    sprites::{Sprite, SpriteRender, SpriteType},
};

const VIDEO_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/video.mjpeg.avi"
);

// 1.0 = normal speed
// 0.5 = half speed
// 2.0 = double speed
//
// Audio is resampled to match this speed. This changes pitch as well as speed.
const PLAYBACK_SPEED: f64 = 1.0;

// Number of decoded video frames kept ahead.
const FRAMES_AHEAD: usize = 8;

// Keep at least this much decoded media/audio queued ahead of playback.
// 8 frames at 60 FPS is only ~133 ms, which is easy for an audio device to
// drain during a decode/render spike. Half a second gives CPAL a much safer
// cushion while FRAMES_AHEAD remains the minimum requested video lookahead.
const MIN_MEDIA_BUFFER_SECONDS: f64 = 0.50;

// false = single-threaded MJPEG decoding
// true  = background/parallel JPEG decoding
const USE_THREADS: bool = true;

// Only used when USE_THREADS is true.
const DECODE_WORKERS: usize = 4;

// Maximum sleep slice while waiting for the audio/video clock. Keeping this
// small makes window input remain responsive without busy-spinning.
const CLOCK_SLEEP_SLICE: Duration = Duration::from_millis(2);

#[inline]
/// Demonstrates or supports `rgba_to_matawin`.
///
/// Visibility: **private to the example binary**.
fn rgba_to_matawin(pixel: u32) -> u32 {
    (pixel >> 8) & 0x00FF_FFFF
}

/// Demonstrates or supports `create_bitmap`.
///
/// Visibility: **private to the example binary**.
fn create_bitmap(
    pixels: Vec<u32>,
    width: usize,
    height: usize,
) -> Vec<Vec<u32>> {
    assert_eq!(pixels.len(), width * height);

    let mut bitmap = Vec::with_capacity(height);

    for row in pixels.chunks_exact(width) {
        let mut output_row = Vec::with_capacity(width);

        for &pixel in row {
            output_row.push(rgba_to_matawin(pixel));
        }

        bitmap.push(output_row);
    }

    bitmap
}

/// Demonstrates or supports `update_bitmap`.
///
/// Visibility: **private to the example binary**.
fn update_bitmap(
    bitmap: &mut Vec<Vec<u32>>,
    pixels: &[u32],
    width: usize,
    height: usize,
) {
    assert_eq!(pixels.len(), width * height);

    if bitmap.len() != height
        || bitmap.first().map_or(0, Vec::len) != width
    {
        bitmap.clear();
        bitmap.reserve(height);

        for _ in 0..height {
            bitmap.push(vec![0; width]);
        }
    }

    for y in 0..height {
        let start = y * width;
        let source = &pixels[start..start + width];
        let destination = &mut bitmap[y];

        for (dst, &src) in destination.iter_mut().zip(source) {
            *dst = rgba_to_matawin(src);
        }
    }
}

// ============================================================================
// Video source
// ============================================================================

enum VideoSource {
    Single(Mjpeg<File>),
    Threaded(ThreadedMjpeg),
}

impl VideoSource {
    /// Demonstrates or supports `open`.
    ///
    /// Visibility: **private to the example binary**.
    fn open(path: &str) -> Result<Self, VideoError> {
        let file = File::open(path)
            .unwrap_or_else(|error| panic!("Failed to open '{path}': {error}"));

        if USE_THREADS {
            Ok(Self::Threaded(ThreadedMjpeg::from_avi(
                file,
                FRAMES_AHEAD,
                DECODE_WORKERS,
            )?))
        } else {
            let mut video = Mjpeg::from_avi(file)?;
            video.set_frames_ahead(FRAMES_AHEAD);

            // Decode the next frame plus FRAMES_AHEAD future frames on this
            // thread before playback begins.
            video.prefill()?;

            Ok(Self::Single(video))
        }
    }

    /// Demonstrates or supports `next_frame`.
    ///
    /// Visibility: **private to the example binary**.
    fn next_frame(&mut self) -> Result<VideoFrame, VideoError> {
        match self {
            Self::Single(video) => video.next_frame(),
            Self::Threaded(video) => video.next_frame(),
        }
    }

    /// Demonstrates or supports `width`.
    ///
    /// Visibility: **private to the example binary**.
    fn width(&self) -> u32 {
        match self {
            Self::Single(video) => video.width(),
            Self::Threaded(video) => video.width(),
        }
    }

    /// Demonstrates or supports `height`.
    ///
    /// Visibility: **private to the example binary**.
    fn height(&self) -> u32 {
        match self {
            Self::Single(video) => video.height(),
            Self::Threaded(video) => video.height(),
        }
    }

    /// Demonstrates or supports `fps`.
    ///
    /// Visibility: **private to the example binary**.
    fn fps(&self) -> u32 {
        match self {
            Self::Single(video) => video.fps(),
            Self::Threaded(video) => video.fps(),
        }
    }

    /// Demonstrates or supports `buffered_frames`.
    ///
    /// Visibility: **private to the example binary**.
    fn buffered_frames(&self) -> usize {
        match self {
            Self::Single(video) => video.buffered_frames(),
            Self::Threaded(video) => video.outstanding_frames(),
        }
    }

    /// Demonstrates or supports `maintain_lookahead`.
    ///
    /// Visibility: **private to the example binary**.
    fn maintain_lookahead(&mut self) -> Result<(), VideoError> {
        if let Self::Single(video) = self {
            video.fill_ahead(1)?;
        }

        Ok(())
    }
}

// ============================================================================
// CPAL audio
// ============================================================================

/// Interleaved source f32 samples waiting to be sent to CPAL.
///
/// Resampling is performed inside the CPAL callback. `phase` is measured in
/// source audio frames, and `step` is the number of source frames consumed for
/// every output-device frame.
struct AudioQueue {
    samples: VecDeque<f32>,
    source_channels: usize,
    phase: f64,
    step: f64,
}

impl AudioQueue {
    /// Demonstrates or supports `new`.
    ///
    /// Visibility: **private to the example binary**.
    fn new(
        source_channels: usize,
        source_rate: u32,
        output_rate: u32,
        playback_speed: f64,
    ) -> Self {
        Self {
            samples: VecDeque::new(),
            source_channels,
            phase: 0.0,
            step: source_rate as f64 * playback_speed / output_rate as f64,
        }
    }

    #[inline]
    /// Demonstrates or supports `source_frames_available`.
    ///
    /// Visibility: **private to the example binary**.
    fn source_frames_available(&self) -> usize {
        self.samples.len() / self.source_channels
    }

    /// Demonstrates or supports `push_samples`.
    ///
    /// Visibility: **private to the example binary**.
    fn push_samples(&mut self, samples: &[f32]) {
        self.samples.extend(samples.iter().copied());
    }

    #[inline]
    /// Demonstrates or supports `source_sample`.
    ///
    /// Visibility: **private to the example binary**.
    fn source_sample(&self, frame_offset: usize, channel: usize) -> f32 {
        let index = frame_offset * self.source_channels + channel;
        self.samples.get(index).copied().unwrap_or(0.0)
    }

    #[inline]
    /// Demonstrates or supports `interpolated_source_channel`.
    ///
    /// Visibility: **private to the example binary**.
    fn interpolated_source_channel(&self, channel: usize) -> f32 {
        let a = self.source_sample(0, channel);
        let b = self.source_sample(1, channel);
        a + (b - a) * self.phase as f32
    }

    /// Demonstrates or supports `output_channel_sample`.
    ///
    /// Visibility: **private to the example binary**.
    fn output_channel_sample(
        &self,
        output_channel: usize,
        output_channels: usize,
    ) -> f32 {
        if self.source_channels == 1 {
            return self.interpolated_source_channel(0);
        }

        // Downmix multi-channel source to mono.
        if output_channels == 1 {
            let mut sum = 0.0f32;
            for channel in 0..self.source_channels {
                sum += self.interpolated_source_channel(channel);
            }
            return sum / self.source_channels as f32;
        }

        // Common stereo/multi-channel mapping. Extra device channels repeat
        // source channels rather than becoming silent.
        let source_channel = output_channel % self.source_channels;
        self.interpolated_source_channel(source_channel)
    }

    /// Demonstrates or supports `advance_one_output_frame`.
    ///
    /// Visibility: **private to the example binary**.
    fn advance_one_output_frame(&mut self) {
        self.phase += self.step;

        let source_frames_to_drop = self.phase.floor() as usize;
        self.phase -= source_frames_to_drop as f64;

        let samples_to_drop = source_frames_to_drop
            .saturating_mul(self.source_channels)
            .min(self.samples.len());

        for _ in 0..samples_to_drop {
            self.samples.pop_front();
        }
    }
}

struct AudioOutput {
    stream: cpal::Stream,
    queue: Arc<Mutex<AudioQueue>>,

    // The first source-audio sample corresponds to this media timestamp.
    media_origin_seconds: f64,

    source_channels: u16,
    source_rate: u32,
    output_channels: u16,
    output_rate: u32,
    playback_speed: f64,

    // CPAL predicts the hardware playback instant for every callback. The
    // first prediction gives us an audio-device clock anchor, allowing video
    // to follow the actual audio playback clock rather than CPU decode speed.
    first_playback_instant: Arc<Mutex<Option<cpal::StreamInstant>>>,

    total_source_frames_queued: u64,
    started: bool,
}

impl AudioOutput {
    /// Demonstrates or supports `new`.
    ///
    /// Visibility: **private to the example binary**.
    fn new(
        source_channels: u16,
        source_rate: u32,
        playback_speed: f64,
        media_origin_seconds: f64,
    ) -> Result<Self, String> {
        if source_channels == 0 {
            return Err("audio source has zero channels".to_string());
        }

        if source_rate == 0 {
            return Err("audio source has a zero sample rate".to_string());
        }

        if !playback_speed.is_finite() || playback_speed <= 0.0 {
            return Err("PLAYBACK_SPEED must be finite and greater than zero".to_string());
        }

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default CPAL output device is available".to_string())?;

        let supported = device
            .default_output_config()
            .map_err(|error| format!("failed to query default audio output config: {error}"))?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let output_channels = config.channels;
        let output_rate = config.sample_rate;

        let queue = Arc::new(Mutex::new(AudioQueue::new(
            source_channels as usize,
            source_rate,
            output_rate,
            playback_speed,
        )));

        let first_playback_instant = Arc::new(Mutex::new(None));

        let stream = match sample_format {
            SampleFormat::I8 => build_output_stream::<i8>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::U8 => build_output_stream::<u8>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::I24 => build_output_stream::<cpal::I24>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::U24 => build_output_stream::<cpal::U24>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::I32 => build_output_stream::<i32>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::U32 => build_output_stream::<u32>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::I64 => build_output_stream::<i64>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::U64 => build_output_stream::<u64>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            SampleFormat::F64 => build_output_stream::<f64>(
                &device,
                config,
                Arc::clone(&queue),
                Arc::clone(&first_playback_instant),
            ),
            other => {
                return Err(format!(
                    "unsupported CPAL output sample format: {other}"
                ));
            }
        }
        .map_err(|error| format!("failed to build CPAL output stream: {error}"))?;

        Ok(Self {
            stream,
            queue,
            media_origin_seconds,
            source_channels,
            source_rate,
            output_channels,
            output_rate,
            playback_speed,
            first_playback_instant,
            total_source_frames_queued: 0,
            started: false,
        })
    }

    /// Demonstrates or supports `play`.
    ///
    /// Visibility: **private to the example binary**.
    fn play(&mut self) -> Result<(), String> {
        if self.started {
            return Ok(());
        }

        self.stream
            .play()
            .map_err(|error| format!("failed to start CPAL output stream: {error}"))?;

        self.started = true;
        Ok(())
    }

    /// Demonstrates or supports `push_video_frame`.
    ///
    /// Visibility: **private to the example binary**.
    fn push_video_frame(&mut self, frame: &VideoFrame) -> Result<(), String> {
        if frame.samples.is_empty() {
            return Ok(());
        }

        if frame.channels != self.source_channels {
            return Err(format!(
                "audio channel count changed from {} to {}",
                self.source_channels,
                frame.channels,
            ));
        }

        if frame.sample_rate != self.source_rate {
            return Err(format!(
                "audio sample rate changed from {} Hz to {} Hz",
                self.source_rate,
                frame.sample_rate,
            ));
        }

        let channels = self.source_channels as usize;
        if frame.samples.len() % channels != 0 {
            return Err(format!(
                "audio sample count {} is not divisible by channel count {}",
                frame.samples.len(),
                channels,
            ));
        }

        self.total_source_frames_queued = self
            .total_source_frames_queued
            .saturating_add((frame.samples.len() / channels) as u64);

        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        queue.push_samples(&frame.samples);
        Ok(())
    }

    /// Current media timestamp according to CPAL's hardware stream clock.
    ///
    /// Because the audio is resampled by PLAYBACK_SPEED, real device time is
    /// multiplied by PLAYBACK_SPEED to get the corresponding media time.
    fn media_seconds(&self) -> f64 {
        if !self.started {
            return self.media_origin_seconds;
        }

        let anchor = *self
            .first_playback_instant
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let Some(anchor) = anchor else {
            return self.media_origin_seconds;
        };

        let now = self.stream.now();
        let elapsed = now.saturating_duration_since(anchor).as_secs_f64();

        self.media_origin_seconds + elapsed * self.playback_speed
    }

    /// Demonstrates or supports `media_end_seconds`.
    ///
    /// Visibility: **private to the example binary**.
    fn media_end_seconds(&self) -> f64 {
        self.media_origin_seconds
            + self.total_source_frames_queued as f64 / self.source_rate as f64
    }

    /// Demonstrates or supports `queued_source_frames`.
    ///
    /// Visibility: **private to the example binary**.
    fn queued_source_frames(&self) -> usize {
        let queue = self
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.source_frames_available()
    }
}

/// Demonstrates or supports `build_output_stream`.
///
/// Visibility: **private to the example binary**.
fn build_output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    queue: Arc<Mutex<AudioQueue>>,
    first_playback_instant: Arc<Mutex<Option<cpal::StreamInstant>>>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let output_channels = config.channels as usize;

    device.build_output_stream(
        config,
        move |output: &mut [T], info: &cpal::OutputCallbackInfo| {
            {
                let mut anchor = first_playback_instant
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());

                if anchor.is_none() {
                    *anchor = Some(info.timestamp().playback);
                }
            }

            let mut queue = queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            for output_frame in output.chunks_mut(output_channels) {
                if queue.source_frames_available() >= 2 {
                    for (channel, sample) in output_frame.iter_mut().enumerate() {
                        let value = queue
                            .output_channel_sample(channel, output_channels)
                            .clamp(-1.0, 1.0);

                        *sample = <T as FromSample<f32>>::from_sample_(value);
                    }

                    queue.advance_one_output_frame();
                } else {
                    for sample in output_frame {
                        *sample = <T as FromSample<f32>>::from_sample_(0.0);
                    }
                }
            }
        },
        move |error| {
            eprintln!("CPAL output stream error: {error}");
        },
        None,
    )
}

/// Queue a VideoFrame's audio. If this is the first frame containing audio,
/// create the CPAL stream using the frame's channel count/sample rate.
///
/// Returns true when a new AudioOutput was created.
fn queue_frame_audio(
    audio: &mut Option<AudioOutput>,
    frame: &VideoFrame,
    source_fps: u32,
) -> Result<bool, String> {
    if frame.samples.is_empty() {
        return Ok(false);
    }

    if frame.channels == 0 {
        return Err("VideoFrame has audio samples but channels == 0".to_string());
    }

    if frame.sample_rate == 0 {
        return Err("VideoFrame has audio samples but sample_rate == 0".to_string());
    }

    let mut created = false;

    if audio.is_none() {
        let image_index = frame
            .image_frames
            .first()
            .map(|image| image.index)
            .unwrap_or(0);

        let media_origin_seconds = image_index as f64 / source_fps as f64;

        *audio = Some(AudioOutput::new(
            frame.channels,
            frame.sample_rate,
            PLAYBACK_SPEED,
            media_origin_seconds,
        )?);

        created = true;
    }

    audio
        .as_mut()
        .expect("audio was initialized above")
        .push_video_frame(frame)?;

    Ok(created)
}

// ============================================================================
// Timing
// ============================================================================

/// Demonstrates or supports `frame_media_seconds`.
///
/// Visibility: **private to the example binary**.
fn frame_media_seconds(frame_index: u64, source_fps: u32) -> f64 {
    frame_index as f64 / source_fps as f64
}

/// Demonstrates or supports `current_media_seconds`.
///
/// Visibility: **private to the example binary**.
fn current_media_seconds(
    audio: Option<&AudioOutput>,
    wall_start: Instant,
) -> f64 {
    if let Some(audio) = audio {
        if audio.started {
            return audio.media_seconds();
        }
    }

    wall_start.elapsed().as_secs_f64() * PLAYBACK_SPEED
}

/// Demonstrates or supports `wait_for_media_time`.
///
/// Visibility: **private to the example binary**.
fn wait_for_media_time(
    target_media_seconds: f64,
    audio: Option<&AudioOutput>,
    wall_start: Instant,
) {
    loop {
        let current = current_media_seconds(audio, wall_start);

        if current >= target_media_seconds {
            break;
        }

        let remaining_media = target_media_seconds - current;
        let remaining_real = remaining_media / PLAYBACK_SPEED;

        let sleep_for = Duration::from_secs_f64(remaining_real)
            .min(CLOCK_SLEEP_SLICE);

        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
    }
}

/// Demonstrates or supports `target_buffer_frames`.
///
/// Visibility: **private to the example binary**.
fn target_buffer_frames(source_fps: u32) -> usize {
    let by_time = (source_fps as f64 * MIN_MEDIA_BUFFER_SECONDS)
        .ceil() as usize;

    FRAMES_AHEAD.max(by_time).max(2)
}

/// Keep future VideoFrames decoded and, importantly, queue their audio into
/// CPAL immediately. The old player only filled this queue once at startup;
/// while those frames were being displayed, the audio queue drained without
/// replacement and repeatedly underrran.
fn refill_pending_frames(
    video: &mut VideoSource,
    pending_frames: &mut VecDeque<VideoFrame>,
    audio: &mut Option<AudioOutput>,
    source_fps: u32,
    target_frames: usize,
    terminal_error: &mut Option<String>,
) -> Result<(), String> {
    while pending_frames.len() < target_frames
        && terminal_error.is_none()
    {
        let frame = match video.next_frame() {
            Ok(frame) => frame,
            Err(error) => {
                *terminal_error = Some(error.to_string());
                break;
            }
        };

        // Queue audio as soon as the future frame is decoded, not when its
        // image is finally presented. This keeps the CPAL ring comfortably
        // ahead of the hardware clock.
        queue_frame_audio(
            audio,
            &frame,
            source_fps,
        )?;

        pending_frames.push_back(frame);

        if let Err(error) = video.maintain_lookahead() {
            *terminal_error = Some(error.to_string());
            break;
        }
    }

    Ok(())
}

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() {
    if !PLAYBACK_SPEED.is_finite() || PLAYBACK_SPEED <= 0.0 {
        panic!("PLAYBACK_SPEED must be finite and greater than zero");
    }

    let mut video = VideoSource::open(VIDEO_PATH)
        .unwrap_or_else(|error| panic!("Failed to open video: {error}"));

    let source_fps = video.fps();
    if source_fps == 0 {
        panic!("AVI did not contain a valid video frame rate");
    }

    // ------------------------------------------------------------------------
    // Decode the first frame
    // ------------------------------------------------------------------------

    let first_frame = video
        .next_frame()
        .unwrap_or_else(|error| panic!("Failed to decode first video frame: {error}"));

    let width = video.width() as usize;
    let height = video.height() as usize;

    if width == 0 || height == 0 {
        panic!("Invalid video dimensions: {width}x{height}");
    }

    let first_image = first_frame
        .image_frames
        .first()
        .expect("MJPEG frame did not contain an ImageFrame");

    let bitmap = create_bitmap(
        first_image.pixels.clone(),
        width,
        height,
    );

    // ------------------------------------------------------------------------
    // Window
    // ------------------------------------------------------------------------

    let mut window = Window::new(
        "MJPEG Player",
        (
            width.try_into().unwrap(),
            height.try_into().unwrap(),
        ),
    );

    window.set_maximizable(false);
    window.set_resizable(false);
    window.set_background_color(BLACK);

    // Do not use window.set_target_fps() for media timing. The audio stream (or
    // wall clock when there is no audio) controls presentation timing below.

    let video_sprite = window.sprites.len();
    window.sprites.push(Sprite {
        sprite_type: SpriteType::Custom("Video"),
        health: 1,
        position: (0, 0),
        size: (width, height),
        render: SpriteRender::Bitmap { pixels: bitmap },
        vectors: Vec::new(),
        is_solid: false,
    });

    println!("Video: {width}x{height}");
    println!("Source FPS: {source_fps}");
    println!("Playback speed: {PLAYBACK_SPEED}x");
    println!("Frames ahead: {FRAMES_AHEAD}");
    println!("Threaded video decoding: {USE_THREADS}");

    if USE_THREADS {
        println!("JPEG workers: {DECODE_WORKERS}");
    }

    window.present();

    // ------------------------------------------------------------------------
    // Audio + continuous preroll
    // ------------------------------------------------------------------------

    let mut audio: Option<AudioOutput> = None;

    queue_frame_audio(
        &mut audio,
        &first_frame,
        source_fps,
    )
    .unwrap_or_else(|error| panic!("Failed to queue first audio frame: {error}"));

    let target_frames = target_buffer_frames(source_fps);
    let mut pending_frames = VecDeque::<VideoFrame>::with_capacity(target_frames);
    let mut terminal_error: Option<String> = None;

    refill_pending_frames(
        &mut video,
        &mut pending_frames,
        &mut audio,
        source_fps,
        target_frames,
        &mut terminal_error,
    )
    .unwrap_or_else(|error| panic!("Failed to preroll video/audio: {error}"));

    println!("Playback buffer target: {target_frames} frames");

    if let Some(audio) = audio.as_mut() {
        println!(
            "Audio: {} channels @ {} Hz -> {} channels @ {} Hz",
            audio.source_channels,
            audio.source_rate,
            audio.output_channels,
            audio.output_rate,
        );
        println!(
            "Initial queued source audio frames: {}",
            audio.queued_source_frames()
        );

        // Normal AVI files begin audio at (or extremely near) media time zero.
        // Starting only after the larger preroll prevents an immediate CPAL
        // underrun.
        audio
            .play()
            .unwrap_or_else(|error| panic!("Failed to start audio: {error}"));
    } else {
        println!(
            "No audio samples were supplied by the decoder; using timer-based video pacing."
        );
    }

    // The fallback wall clock starts only after decode/audio preroll.
    let wall_start = Instant::now();

    // ------------------------------------------------------------------------
    // Playback
    // ------------------------------------------------------------------------

    'playback: loop {
        for event in window.poll_events() {
            match event {
                WindowEvent::Close | WindowEvent::KeyPressed(Key::Escape) => {
                    break 'playback;
                }
                _ => {}
            }
        }

        let frame = match pending_frames.pop_front() {
            Some(frame) => frame,
            None => {
                if terminal_error.is_some() {
                    break;
                }

                // Normally unreachable because refill_pending_frames keeps the
                // queue full, but recover gracefully if the target is changed
                // to zero or the queue is otherwise drained.
                match video.next_frame() {
                    Ok(frame) => frame,
                    Err(error) => {
                        terminal_error = Some(error.to_string());
                        break;
                    }
                }
            }
        };

        // Replenish immediately after consuming one queued frame. This happens
        // BEFORE we wait for the current image's presentation time, giving the
        // decoder that entire interval to keep audio safely ahead.
        refill_pending_frames(
            &mut video,
            &mut pending_frames,
            &mut audio,
            source_fps,
            target_frames,
            &mut terminal_error,
        )
        .unwrap_or_else(|error| panic!("Failed to refill video/audio buffer: {error}"));

        let frame_width = video.width() as usize;
        let frame_height = video.height() as usize;

        if frame_width != width || frame_height != height {
            eprintln!(
                "MJPEG frame changed dimensions from {width}x{height} to {frame_width}x{frame_height}"
            );
            break;
        }

        let image = match frame.image_frames.first() {
            Some(image) => image,
            None => {
                eprintln!("Decoded MJPEG VideoFrame contained no ImageFrame");
                break;
            }
        };

        let target_media_seconds = frame_media_seconds(image.index, source_fps);

        wait_for_media_time(
            target_media_seconds,
            audio.as_ref(),
            wall_start,
        );

        match &mut window.sprites[video_sprite].render {
            SpriteRender::Bitmap { pixels: bitmap } => {
                update_bitmap(
                    bitmap,
                    &image.pixels,
                    width,
                    height,
                );
            }
            _ => unreachable!("Video sprite is no longer a bitmap"),
        }

        window.present();

        // Optional diagnostics. If queued_source_frames repeatedly approaches
        // zero, increase MIN_MEDIA_BUFFER_SECONDS or investigate decode speed.
        // println!(
        //     "frame={} audio_queue={} video_queue={}",
        //     image.index,
        //     audio.as_ref().map_or(0, AudioOutput::queued_source_frames),
        //     pending_frames.len(),
        // );
    }

    if let Some(error) = terminal_error {
        println!("Video finished: {error}");
    }

    // Let the final queued audio reach the hardware before dropping the CPAL
    // stream. Without this, the last fraction of a second can be cut off.
    if let Some(audio) = audio.as_ref() {
        let audio_end = audio.media_end_seconds();

        while audio.media_seconds() < audio_end {
            let remaining_media = audio_end - audio.media_seconds();
            let remaining_real = remaining_media / PLAYBACK_SPEED;
            let sleep_for = Duration::from_secs_f64(remaining_real)
                .min(CLOCK_SLEEP_SLICE);

            if !sleep_for.is_zero() {
                thread::sleep(sleep_for);
            }
        }
    }
}
