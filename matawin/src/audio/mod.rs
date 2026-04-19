use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use std::thread;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::{Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};



pub struct Audio;

impl Audio {
    /// Constructor (empty for now)
    pub fn new() -> Self {
        Self
    }

    /// Play a sound effect asynchronously
    pub fn play<S: Into<String>>(&self, path: S) {
        let path = path.into();

        thread::spawn(move || {
            // Open file
            let file = std::fs::File::open(&path).expect("failed to open file");
            let mss = MediaSourceStream::new(Box::new(file), Default::default());

            // Probe format
            let hint = Hint::new();
            let probed = symphonia::default::get_probe()
                .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
                .expect("unsupported format");

            let format = probed.format;
            let track = format.default_track().expect("no track");
            let codec_params = &track.codec_params;

            let channels = codec_params.channels.map_or(2, |c| c.count());

            let decoder = symphonia::default::get_codecs()
                .make(codec_params, &DecoderOptions::default())
                .expect("unsupported codec");

            let state = Arc::new(Mutex::new(DecoderState {
                format,
                decoder,
                buffer: Vec::new(),
                pos: 0,
                channels,
                ended: false,
            }));

            // CPAL setup
            let host = cpal::default_host();
            let device = host.default_output_device().expect("no output device");
            let config = device.default_output_config().unwrap();
            let sample_format = config.sample_format();
            let config: cpal::StreamConfig = config.into();

            let s = match sample_format {
                cpal::SampleFormat::F32 => build_stream(&device, &config, state.clone(), Arc::new(AtomicBool::new(false)), |x| x),
                cpal::SampleFormat::I16 => build_stream(&device, &config, state.clone(), Arc::new(AtomicBool::new(false)), f32_to_i16),
                cpal::SampleFormat::U16 => build_stream(&device, &config, state.clone(), Arc::new(AtomicBool::new(false)), f32_to_u16),
                _ => panic!("unsupported device format"),
            }
            .unwrap();

            s.play().unwrap();

            // Keep thread alive until sound ends
            loop {
                let st = state.lock().unwrap();
                if st.ended {
                    break;
                }
                drop(st);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
    }
}

// --- Reuse your DecoderState, build_stream, and converters from before ---
pub struct Bgs {
    state: Arc<Mutex<DecoderState>>,
    stream: cpal::Stream,
    paused: Arc<AtomicBool>,
    playlist: Vec<String>,
    loop_playlist: bool,
}

impl Bgs {
    /// Load a single track (default, no playlist loop)
    pub fn play<S: Into<String>>(path: S) -> Self {
        Self::with_playlist(vec![path.into()], false)
    }

    /// Load multiple tracks as a playlist, optionally looping
    pub fn with_playlist<S: Into<String>>(playlist: Vec<S>, loop_playlist: bool) -> Self {
        // Start with the first track
        let playlist: Vec<String> = playlist.into_iter().map(|s| s.into()).collect();
        let first_track = playlist.first().expect("Playlist cannot be empty").clone();

        let (state, stream, paused) = Self::init_stream(&first_track);

        Self {
            state,
            stream,
            paused,
            playlist,
            loop_playlist,
        }
    }

    /// Pause playback
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume playback
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        !self.paused.load(Ordering::SeqCst)
    }

    /// Internal helper: initialize decoder & stream for a path
    fn init_stream(path: &str) -> (Arc<Mutex<DecoderState>>, cpal::Stream, Arc<AtomicBool>) {
        let file = std::fs::File::open(path).expect("failed to open file");
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let hint = Hint::new();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .expect("unsupported format");

        let format = probed.format;
        let track = format.default_track().expect("no track");
        let codec_params = &track.codec_params;

        let channels = codec_params.channels.map_or(2, |c| c.count());

        let decoder = symphonia::default::get_codecs()
            .make(codec_params, &DecoderOptions::default())
            .expect("unsupported codec");

        let state = Arc::new(Mutex::new(DecoderState {
            format,
            decoder,
            buffer: Vec::new(),
            pos: 0,
            channels,
            ended: false,
        }));

        let paused = Arc::new(AtomicBool::new(false));

        // CPAL setup
        let host = cpal::default_host();
        let device = host.default_output_device().expect("no output device");
        let config = device.default_output_config().unwrap();
        let sample_format = config.sample_format();
        let config: cpal::StreamConfig = config.into();

        let s = match sample_format {
            cpal::SampleFormat::F32 => build_stream(&device, &config, state.clone(), paused.clone(), |x| x),
            cpal::SampleFormat::I16 => build_stream(&device, &config, state.clone(), paused.clone(), f32_to_i16),
            cpal::SampleFormat::U16 => build_stream(&device, &config, state.clone(), paused.clone(), f32_to_u16),
            _ => panic!("unsupported device format"),
        }
        .unwrap();

        s.play().unwrap();

        (state, s, paused)
    }
}


// --- Decoder State ---
struct DecoderState {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    buffer: Vec<f32>,
    pos: usize,
    channels: usize,
    ended: bool, // NEW
}

impl DecoderState {
    fn next_sample(&mut self) -> f32 {
        loop {
            if self.pos < self.buffer.len() {
                let s = self.buffer[self.pos];
                self.pos += 1;
                return s;
            }

            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(_)) => {
                    self.ended = true;
                    return 0.0;
                } // EOF
                Err(e) => {
                    eprintln!("packet error: {:?}", e);
                    return 0.0;
                }
            };

            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(Error::DecodeError(_)) => continue,
                Err(e) => {
                    eprintln!("decode error: {:?}", e);
                    return 0.0;
                }
            };

            self.buffer.clear();
            self.pos = 0;

            match decoded {
                AudioBufferRef::F32(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push(buf.chan(c)[f]); }
                    }
                }
                AudioBufferRef::F64(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push(buf.chan(c)[f] as f32); }
                    }
                }
                AudioBufferRef::S8(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push(buf.chan(c)[f] as f32 / i8::MAX as f32); }
                    }
                },

                AudioBufferRef::S16(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push(buf.chan(c)[f] as f32 / i16::MAX as f32); }
                    }
                },
                AudioBufferRef::S24(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch {
                            let sample = buf.chan(c)[f].inner(); // ✅ convert properly
                            let normalized = sample as f32 / i32::MAX as f32;
                            self.buffer.push(normalized);
                        }
                    }
                }
                AudioBufferRef::S32(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push(buf.chan(c)[f] as f32 / i32::MAX as f32); }
                    }
                },
                AudioBufferRef::U8(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push((buf.chan(c)[f] as f32 / u8::MAX as f32) * 2.0 - 1.0); }
                    }
                },
                AudioBufferRef::U16(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push((buf.chan(c)[f] as f32 / u16::MAX as f32) * 2.0 - 1.0); }
                    }
                },
                AudioBufferRef::U24(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch {
                            let sample = buf.chan(c)[f].inner(); // ✅ convert properly
                            let normalized = (sample as f32 / u32::MAX as f32) * 2.0 - 1.0;
                            self.buffer.push(normalized);
                        }
                    }
                }
                AudioBufferRef::U32(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch { self.buffer.push((buf.chan(c)[f] as f32 / u32::MAX as f32) * 2.0 - 1.0); }
                    }
                },
            }
        }
    }
}


fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    state: Arc<Mutex<DecoderState>>,
    paused: Arc<AtomicBool>,
    convert: fn(f32) -> T,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + cpal::SizedSample + Send + 'static,
{
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            let mut state = state.lock().unwrap();
            for frame in output.chunks_mut(channels) {
                for ch in 0..channels {
                    let sample = if paused.load(Ordering::SeqCst) { 0.0 } else { state.next_sample() };
                    frame[ch] = convert(sample);
                }
            }
        },
        |e| eprintln!("Stream error: {:?}", e),
        None,
    )
}

// --- Converters ---
fn f32_to_i16(x: f32) -> i16 { (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16 }
fn f32_to_u16(x: f32) -> u16 { ((x.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16 }
