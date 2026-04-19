use std::fs::File;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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

//
// 🎧 DecoderState: handles streaming and decoding
//
struct DecoderState {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    buffer: Vec<f32>,
    pos: usize,
    path: String,
    finished: bool,
    looped: bool,
}

impl DecoderState {
    fn new(path: String, looped: bool) -> Self {
        let (format, decoder) = open_decoder(&path);
        Self {
            format,
            decoder,
            buffer: Vec::new(),
            pos: 0,
            path,
            finished: false,
            looped,
        }
    }

    fn restart(&mut self) {
        let (format, decoder) = open_decoder(&self.path);
        self.format = format;
        self.decoder = decoder;
        self.buffer.clear();
        self.pos = 0;
        self.finished = false;
    }

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
                    if self.looped {
                        self.restart();
                        continue;
                    } else {
                        self.finished = true;
                        return 0.0;
                    }
                }
                Err(_) => {
                    self.finished = true;
                    return 0.0;
                }
            };

            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(Error::DecodeError(_)) => continue,
                Err(_) => {
                    self.finished = true;
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
                        for c in 0..ch {
                            self.buffer.push(buf.chan(c)[f]);
                        }
                    }
                }
                AudioBufferRef::F64(buf) => {
                    let buf = buf.as_ref();
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        for c in 0..ch {
                            self.buffer.push(buf.chan(c)[f] as f32);
                        }
                    }
                }
                _ => continue,
            }
        }
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

fn open_decoder(path: &str) -> (Box<dyn FormatReader>, Box<dyn Decoder>) {
    let file = File::open(path).expect("open failed");
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3"); // assume mp3

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .expect("unsupported format");

    let format = probed.format;
    let track = format.default_track().unwrap();
    let codec_params = &track.codec_params;

    let decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .unwrap();

    (format, decoder)
}

//
// 🔊 CPAL stream builder
//
fn build_stream(state: Arc<Mutex<DecoderState>>) -> cpal::Stream {
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device");

    let config = device.default_output_config().unwrap();
    let sample_format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let channels = config.channels as usize;

    match sample_format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| write_data(out, channels, &state),
                err_fn,
                None,
            )
            .unwrap(),
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                &config,
                move |out: &mut [i16], _| write_data_i16(out, channels, &state),
                err_fn,
                None,
            )
            .unwrap(),
        cpal::SampleFormat::U16 => device
            .build_output_stream(
                &config,
                move |out: &mut [u16], _| write_data_u16(out, channels, &state),
                err_fn,
                None,
            )
            .unwrap(),
        _ => panic!("unsupported format"),
    }
}

fn write_data(output: &mut [f32], channels: usize, state: &Arc<Mutex<DecoderState>>) {
    let mut s = state.lock().unwrap();
    for frame in output.chunks_mut(channels) {
        for sample in frame {
            *sample = s.next_sample();
        }
    }
}

fn write_data_i16(output: &mut [i16], channels: usize, state: &Arc<Mutex<DecoderState>>) {
    let mut s = state.lock().unwrap();
    for frame in output.chunks_mut(channels) {
        for sample in frame {
            *sample = (s.next_sample().clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        }
    }
}

fn write_data_u16(output: &mut [u16], channels: usize, state: &Arc<Mutex<DecoderState>>) {
    let mut s = state.lock().unwrap();
    for frame in output.chunks_mut(channels) {
        for sample in frame {
            let v = s.next_sample().clamp(-1.0, 1.0);
            *sample = ((v * 0.5 + 0.5) * u16::MAX as f32) as u16;
        }
    }
}

fn err_fn(e: cpal::StreamError) {
    eprintln!("Stream error: {:?}", e);
}

//
// 🎵 One-shot Audio
//
pub struct Audio;

impl Audio {
    pub fn new() -> Self {
        Self
    }

    pub fn play<S: Into<String>>(&self, path: S) {
        let path = path.into();
        thread::spawn(move || {
            let state = Arc::new(Mutex::new(DecoderState::new(path, false)));
            let stream = build_stream(state);
            stream.play().unwrap();

            // crude sleep until done (replace with proper duration)
            thread::sleep(Duration::from_secs(5));
        });
    }
}

//
// 🎵 Background Playlist
//
pub struct Bgs {
    state: Arc<Mutex<DecoderState>>,
    stream: cpal::Stream,
    playlist: Arc<Mutex<Vec<String>>>,
    index: Arc<Mutex<usize>>,
    paused: Arc<Mutex<bool>>,
}

impl Bgs {
    pub fn new<S: Into<String>>(initial: S) -> Self {
        let path = initial.into();
        let state = Arc::new(Mutex::new(DecoderState::new(path.clone(), false)));
        let stream = build_stream(state.clone());
        stream.play().unwrap();

        let bgs = Self {
            state,
            stream,
            playlist: Arc::new(Mutex::new(vec![path])),
            index: Arc::new(Mutex::new(0)),
            paused: Arc::new(Mutex::new(false)),
        };

        bgs.monitor();
        bgs
    }

    pub fn playlist<S: Into<String>>(list: Vec<S>) -> Self {
        assert!(!list.is_empty(), "Playlist cannot be empty");
        let list: Vec<String> = list.into_iter().map(|s| s.into()).collect();
        let first = list[0].clone();
        let state = Arc::new(Mutex::new(DecoderState::new(first.clone(), false)));
        let stream = build_stream(state.clone());
        stream.play().unwrap();

        let bgs = Self {
            state,
            stream,
            playlist: Arc::new(Mutex::new(list)),
            index: Arc::new(Mutex::new(0)),
            paused: Arc::new(Mutex::new(false)),
        };

        bgs.monitor();
        bgs
    }

    pub fn pause(&self) {
        let mut p = self.paused.lock().unwrap();
        *p = true;
    }

    pub fn resume(&self) {
        let mut p = self.paused.lock().unwrap();
        *p = false;
    }

    fn monitor(&self) {
        let state = self.state.clone();
        let playlist = self.playlist.clone();
        let index = self.index.clone();
        let paused = self.paused.clone();

        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(100));

            if *paused.lock().unwrap() {
                continue;
            }

            let finished = state.lock().unwrap().is_finished();
            if !finished {
                continue;
            }

            let mut idx = index.lock().unwrap();
            let list = playlist.lock().unwrap();

            *idx += 1;
            if *idx >= list.len() {
                // playlist finished
                break;
            }

            let next = list[*idx].clone();
            drop(list);
            drop(idx);

            let mut state_lock = state.lock().unwrap();
            *state_lock = DecoderState::new(next, false);
        });
    }
}