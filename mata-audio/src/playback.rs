//! Streaming audio support used by the widget toolkit.
//!
//! The implementation uses one process-global output stream, one decoder/mixer
//! worker, incremental `mata-audio` decoding, and a lock-free SPSC ring between
//! the worker and the selected playback backend's real-time callback.
//!
//! `mata-playback` is the default backend. CPAL can be selected by building
//! `mata-audio` with `default-features = false, features = ["cpal"]`.

use std::{
    cell::UnsafeCell,
    collections::HashMap,
    mem::MaybeUninit,
    fs::File,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

// Prefer the default mata-playback backend when both backend features are
// enabled. To select CPAL, disable default features and enable `cpal`.
#[cfg(feature = "mata-playback")]
use mata_playback as backend;
#[cfg(all(not(feature = "mata-playback"), feature = "cpal"))]
use cpal as backend;

use backend::{
    BufferSize, FromSample, I24, SampleFormat, SizedSample,
    SupportedBufferSize, SupportedStreamConfig, U24,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crate::{AudioDecoder, AudioError, AudioFrameBuffer, Decoder};
use crate::codecs::support::{TunedBufReader, DEFAULT_IO_CAPACITY};

const MIX_BLOCK_FRAMES: usize = 256;
const TARGET_MIX_AHEAD_FRAMES: usize = 1024;
const RING_BUFFER_MILLISECONDS: usize = 250;
const PREFERRED_DEVICE_BUFFER_FRAMES: u32 = 1024;
const COMMAND_QUEUE_CAPACITY: usize = 512;
const MAX_COMMANDS_PER_TICK: usize = 64;
const PHASE_ONE: u64 = 1u64 << 32;

static NEXT_SOURCE_ID: AtomicU64 = AtomicU64::new(1);
static ENGINE: OnceLock<EngineHandle> = OnceLock::new();
static MIXER_CPU_NS: AtomicU64 = AtomicU64::new(0);
static DECODER_CPU_NS: AtomicU64 = AtomicU64::new(0);
static RING_FILL_SAMPLES: AtomicUsize = AtomicUsize::new(0);
static CALLBACK_PERIOD_NS: AtomicU64 = AtomicU64::new(0);
static RT_ERROR_CODE: AtomicU32 = AtomicU32::new(0);
static LIMITER_MODE: AtomicU32 = AtomicU32::new(1);
static DECODER_POOL: OnceLock<DecodeWorkerPool> = OnceLock::new();
static WORKER_TUNING_HOOK: OnceLock<fn(usize)> = OnceLock::new();
static VOICE_RETAINED_BYTES: AtomicUsize = AtomicUsize::new(0);
static VOICE_HIGH_WATER_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEVICE_RECONFIGURATIONS: AtomicUsize = AtomicUsize::new(0);
static DENORMAL_SAMPLES_SANITIZED: AtomicU64 = AtomicU64::new(0);
static REALTIME_TUNING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Low-overhead process-global playback telemetry snapshots.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlaybackTelemetry {
    pub mixer_cpu_nanoseconds: u64,
    pub decoder_cpu_nanoseconds: u64,
    pub ring_fill_samples: usize,
    pub underruns: usize,
    pub callback_period_nanoseconds: u64,
    pub voice_retained_bytes: usize,
    pub voice_high_water_bytes: usize,
    pub device_reconfigurations: usize,
    pub denormal_samples_sanitized: u64,
}

/// Install an optional hook invoked once from each playback worker. Applications
/// may use it to experiment with OS-specific priority elevation or CPU affinity
/// without making those platform APIs dependencies of mata-audio. Decoder
/// workers pass their zero-based index; the central mixer/output worker passes
/// `usize::MAX`.
pub fn install_worker_tuning_hook(hook: fn(usize)) -> Result<(), fn(usize)> { WORKER_TUNING_HOOK.set(hook) }

/// Backwards-compatible alias for [`install_worker_tuning_hook`].
pub fn install_decoder_worker_tuning_hook(hook: fn(usize)) -> Result<(), fn(usize)> { install_worker_tuning_hook(hook) }

/// Enable or disable best-effort real-time worker tuning. When enabled, the
/// installed worker hook is invoked; absence/failure of an application-supplied
/// platform hook is treated as a graceful fallback to normal scheduling.
pub fn set_realtime_worker_tuning(enabled: bool) { REALTIME_TUNING_ENABLED.store(enabled, Ordering::Release); }

#[inline]
fn tune_worker(index: usize) {
    if REALTIME_TUNING_ENABLED.load(Ordering::Acquire) {
        if let Some(hook) = WORKER_TUNING_HOOK.get() { hook(index); }
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum LimiterQuality { Disabled, HardClip, Soft }
pub fn set_limiter_quality(mode:LimiterQuality){LIMITER_MODE.store(match mode{LimiterQuality::Disabled=>0,LimiterQuality::HardClip=>1,LimiterQuality::Soft=>2},Ordering::Release);}

/// Observable playback state for one streamed source.
#[derive(Debug, Clone)]
pub struct AudioSourceState {
    pub playing: bool,
    pub paused: bool,
    pub stopped: bool,
    pub looping: bool,
    pub volume: f32,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub last_error: Option<String>,
}

impl Default for AudioSourceState {
    fn default() -> Self {
        Self {
            playing: false,
            paused: false,
            stopped: true,
            looping: false,
            volume: 1.0,
            position_seconds: 0.0,
            duration_seconds: None,
            last_error: None,
        }
    }
}

struct SourceFastState {
    flags: AtomicU32,
    volume_bits: AtomicU32,
    position_bits: AtomicU64,
}
impl SourceFastState {
    fn new(playing:bool)->Self{Self{flags:AtomicU32::new(if playing{1}else{2}),volume_bits:AtomicU32::new(1.0f32.to_bits()),position_bits:AtomicU64::new(0.0f64.to_bits())}}
    fn publish(&self,playing:bool,paused:bool,stopped:bool,looping:bool,volume:f32,position:f64){let flags=(playing as u32)|((paused as u32)<<1)|((stopped as u32)<<2)|((looping as u32)<<3);self.volume_bits.store(volume.to_bits(),Ordering::Relaxed);self.position_bits.store(position.to_bits(),Ordering::Relaxed);self.flags.store(flags,Ordering::Release);}
    fn flags(&self)->u32{self.flags.load(Ordering::Acquire)}
}

/// Lightweight handle to a streamed `mata-audio` source mixed by one shared
/// output stream.
#[derive(Clone)]
pub struct AudioSource {
    id: u64,
    path: Arc<PathBuf>,
    state: Arc<Mutex<AudioSourceState>>,
    fast: Arc<SourceFastState>,
}

impl std::fmt::Debug for AudioSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioSource")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("state", &self.state())
            .finish()
    }
}

impl AudioSource {
    /// Creates a source and opens its decoder on the shared mixer worker.
    /// Playback starts immediately unless `autoplay` is false.
    pub fn open<P: AsRef<Path>>(path: P, autoplay: bool) -> Result<Self, String> {
        let path = Arc::new(path.as_ref().to_path_buf());
        if !path.is_file() {
            return Err(format!("audio file does not exist: {}", path.display()));
        }

        let id = NEXT_SOURCE_ID.fetch_add(1, Ordering::Relaxed);
        let state = Arc::new(Mutex::new(AudioSourceState {
            playing: autoplay,
            paused: !autoplay,
            stopped: false,
            ..AudioSourceState::default()
        }));

        let fast=Arc::new(SourceFastState::new(autoplay));
        engine().send(Command::Open { id, path: path.clone(), autoplay, state: state.clone(), fast:fast.clone() })?;

        Ok(Self { id, path, state, fast })
    }

    #[inline]
    pub const fn id(&self) -> u64 { self.id }

    #[inline]
    pub fn path(&self) -> &Path { self.path.as_path() }

    pub fn play(&self) { engine().try_send(Command::Play(self.id)); }
    pub fn pause(&self) { engine().try_send(Command::Pause(self.id)); }
    pub fn toggle_playback(&self) { engine().try_send(Command::Toggle(self.id)); }
    pub fn stop(&self) { engine().try_send(Command::Stop(self.id)); }

    /// Seeks to an absolute playback position through the decoder's native
    /// sample/timeline index. Decoder scratch and staging capacity are retained;
    /// indexed codecs decode only the bounded synchronization gap when needed.
    pub fn seek(&self, seconds: f64) {
        engine().try_send(Command::Seek(self.id, sanitize_seconds(seconds)));
    }

    pub fn set_volume(&self, volume: f32) {
        engine().try_send(Command::SetVolume(self.id, sanitize_gain(volume)));
    }

    pub fn set_looping(&self, looping: bool) { engine().try_send(Command::SetLooping(self.id, looping)); }

    /// Select background producer decoding (default) or inline mixer-thread decoding.
    pub fn set_background_decode(&self, enabled: bool) { engine().try_send(Command::SetBackgroundDecode(self.id, enabled)); }
    /// Explicit non-real-time compaction of retained staging and decoder buffers.
    pub fn trim_memory(&self) { engine().try_send(Command::TrimMemory(self.id)); }

    pub fn state(&self) -> AudioSourceState {
        let mut snapshot=self.state.lock().map(|s|s.clone()).unwrap_or_default(); let flags=self.fast.flags();
        snapshot.playing=flags&1!=0;snapshot.paused=flags&2!=0;snapshot.stopped=flags&4!=0;snapshot.looping=flags&8!=0;
        snapshot.volume=f32::from_bits(self.fast.volume_bits.load(Ordering::Relaxed));snapshot.position_seconds=f64::from_bits(self.fast.position_bits.load(Ordering::Relaxed));snapshot
    }

    #[inline]
    pub fn is_playing(&self) -> bool { self.fast.flags()&1!=0 }
    #[inline]
    pub fn is_paused(&self) -> bool { self.fast.flags()&2!=0 }
    #[inline]
    pub fn volume(&self) -> f32 { f32::from_bits(self.fast.volume_bits.load(Ordering::Relaxed)) }
    #[inline]
    pub fn position(&self) -> f64 { f64::from_bits(self.fast.position_bits.load(Ordering::Relaxed)) }
    #[inline]
    pub fn duration(&self) -> Option<f64> { self.state().duration_seconds }
    #[inline]
    pub fn last_error(&self) -> Option<String> { self.state().last_error }

    pub fn underruns(&self) -> usize { engine().underruns.load(Ordering::Relaxed) }
    pub fn telemetry(&self) -> PlaybackTelemetry { PlaybackTelemetry {
        mixer_cpu_nanoseconds:MIXER_CPU_NS.load(Ordering::Relaxed), decoder_cpu_nanoseconds:DECODER_CPU_NS.load(Ordering::Relaxed),
        ring_fill_samples:RING_FILL_SAMPLES.load(Ordering::Relaxed), underruns:self.underruns(), callback_period_nanoseconds:CALLBACK_PERIOD_NS.load(Ordering::Relaxed),
        voice_retained_bytes:VOICE_RETAINED_BYTES.load(Ordering::Relaxed), voice_high_water_bytes:VOICE_HIGH_WATER_BYTES.load(Ordering::Relaxed),
        device_reconfigurations:DEVICE_RECONFIGURATIONS.load(Ordering::Relaxed), denormal_samples_sanitized:DENORMAL_SAMPLES_SANITIZED.load(Ordering::Relaxed),
    }}
}

impl Drop for AudioSource {
    fn drop(&mut self) {
        // Only remove the mixer voice when the final public handle goes away.
        if Arc::strong_count(&self.state) == 2 {
            engine().try_send(Command::Remove(self.id));
        }
    }
}

struct EngineHandle {
    sender: SyncSender<Command>,
    underruns: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl EngineHandle {
    fn start() -> Self {
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let underruns = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(false));
        let last_error = Arc::new(Mutex::new(None));

        let thread_underruns = underruns.clone();
        let thread_active = active.clone();
        let thread_error = last_error.clone();

        thread::Builder::new()
            .name("matawin-widget-audio".to_string())
            .spawn(move || {
                if let Err(error) = audio_worker(
                    receiver,
                    thread_underruns,
                    thread_active,
                    thread_error.clone(),
                ) {
                    set_error(&thread_error, error);
                }
            })
            .expect("failed to spawn matawin widget audio worker");

        Self { sender, underruns, active, last_error }
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.sender.send(command).map_err(|_| "audio mixer thread is not running".to_string())
    }

    fn try_send(&self, command: Command) {
        match self.sender.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                set_error(&self.last_error, "matawin audio command queue is full; command dropped");
            }
            Err(TrySendError::Disconnected(_)) => {
                set_error(&self.last_error, "matawin audio mixer thread is no longer running");
            }
        }
    }
}

fn engine() -> &'static EngineHandle { ENGINE.get_or_init(EngineHandle::start) }

#[cold]
#[inline(never)]
fn set_error(slot: &Arc<Mutex<Option<String>>>, message: impl Into<String>) {
    let message = message.into();
    eprintln!("matawin audio: {message}");
    if let Ok(mut error) = slot.lock() { *error = Some(message); }
}

#[cold]
#[inline(never)]
fn set_source_error(state: &Arc<Mutex<AudioSourceState>>, message: impl Into<String>) {
    if let Ok(mut state) = state.lock() { state.last_error = Some(message.into()); }
}

enum Command {
    Open {
        id: u64,
        path: Arc<PathBuf>,
        autoplay: bool,
        state: Arc<Mutex<AudioSourceState>>,
        fast: Arc<SourceFastState>,
    },
    Play(u64),
    Pause(u64),
    Toggle(u64),
    Stop(u64),
    Seek(u64, f64),
    SetVolume(u64, f32),
    SetLooping(u64, bool),
    SetBackgroundDecode(u64, bool),
    TrimMemory(u64),
    Remove(u64),
}

fn audio_worker(
    receiver: Receiver<Command>,
    underruns: Arc<AtomicUsize>,
    active_flag: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
) -> Result<(), String> {
    tune_worker(usize::MAX);
    let host = backend::default_host();
    let device = host.default_output_device()
        .ok_or_else(|| "no default audio output device is available".to_string())?;
    let mut session = create_output_session(&device, None, underruns.clone(), active_flag.clone(), last_error.clone())?;

    // Create decoder workers before steady-state mixing so the first source
    // refill does not allocate/spawn threads on the central mixer path.
    let _ = decoder_pool();
    let mut mixer = Mixer::new(
        session.output_rate,
        session.output_channels,
        session.device_period_frames,
        session.ring.clone(),
        active_flag,
        last_error,
        underruns,
    );

    loop {
        if RT_ERROR_CODE.swap(0,Ordering::AcqRel)!=0 {
            set_error(&mixer.last_error,"audio backend reported a real-time callback error; attempting output reconfiguration");
            // Device recovery is cold-path work. Existing DecodeProducer objects
            // remain untouched, so source decoders and their indexes/scratch are
            // not reopened when a device disappears or its format changes.
            if let Some(new_device)=host.default_output_device() {
                match create_output_session(&new_device,mixer.preferred_source_rate(),mixer.underruns.clone(),mixer.active_flag.clone(),mixer.last_error.clone()) {
                    Ok(new_session)=>{
                        mixer.reconfigure_output(new_session.output_rate,new_session.output_channels,new_session.device_period_frames,new_session.ring.clone());
                        session=new_session;
                        DEVICE_RECONFIGURATIONS.fetch_add(1,Ordering::Relaxed);
                    }
                    Err(error)=>set_error(&mixer.last_error,error),
                }
            }
        }
        let mut commands = 0usize;
        while commands < MAX_COMMANDS_PER_TICK {
            match receiver.try_recv() {
                Ok(command) => { mixer.handle_command(command); commands += 1; }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        mixer.update_active_flag();
        // Once source rates are known, prefer a device rate near the active
        // stream average. Reconfiguration occurs only while the output ring is
        // empty and never recreates source decoders.
        let rate_hint=mixer.preferred_source_rate();
        if rate_hint.is_some()&&rate_hint!=session.source_rate_hint&&mixer.ring.available_samples()==0{
            if let Some(new_device)=host.default_output_device(){if let Ok(new_session)=create_output_session(&new_device,rate_hint,mixer.underruns.clone(),mixer.active_flag.clone(),mixer.last_error.clone()){mixer.reconfigure_output(new_session.output_rate,new_session.output_channels,new_session.device_period_frames,new_session.ring.clone());session=new_session;DEVICE_RECONFIGURATIONS.fetch_add(1,Ordering::Relaxed);}}
        }
        mixer.adapt_from_ring();
        if mixer.has_audio() {
            let (queued_samples, free_samples) = mixer.ring.occupancy();
            let queued_frames = queued_samples / mixer.output_channels.max(1);
            if queued_frames < mixer.target_mix_ahead_frames {
                let free_frames = free_samples / mixer.output_channels.max(1);
                let wanted = mixer.target_mix_ahead_frames - queued_frames;
                let frames = mixer.mix_block_frames.min(wanted).min(free_frames);
                if frames != 0 { mixer.mix_and_queue(frames); continue; }
            }
        }

        let queued_frames = mixer.ring.available_samples() / mixer.output_channels.max(1);
        let frames_until_refill = queued_frames.saturating_sub(mixer.target_mix_ahead_frames / 2);
        let refill_micros = ((frames_until_refill as u128 * 1_000_000) / mixer.output_rate.max(1) as u128)
            .clamp(500, 10_000) as u64;
        match receiver.recv_timeout(Duration::from_micros(refill_micros)) {
            Ok(command) => mixer.handle_command(command),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        // Keep the output stream/session live for the entire worker loop.
        std::hint::black_box(&session.stream);
    }
}

#[derive(Debug, Clone, Copy)]
struct OutputPolicy { preferred_period_frames:u32, ring_buffer_milliseconds:usize, minimum_mix_ahead_frames:usize }

fn platform_output_policy()->OutputPolicy{
    #[cfg(target_os="windows")]
    { return OutputPolicy{preferred_period_frames:480,ring_buffer_milliseconds:120,minimum_mix_ahead_frames:512}; }
    #[cfg(target_os="android")]
    {
        let burst=std::env::var("MATA_AUDIO_ANDROID_FRAMES_PER_BURST").ok().and_then(|v|v.parse::<u32>().ok()).unwrap_or(192).clamp(64,2048);
        return OutputPolicy{preferred_period_frames:burst,ring_buffer_milliseconds:96,minimum_mix_ahead_frames:(burst as usize*4).max(384)};
    }
    #[cfg(target_os="linux")]
    {
        // PulseAudio/PipeWire generally prefer a slightly deeper client queue
        // than direct ALSA. Environment detection is only a sizing hint.
        let pulse=std::env::var_os("PULSE_SERVER").is_some()||std::env::var_os("PIPEWIRE_RUNTIME_DIR").is_some();
        return if pulse{OutputPolicy{preferred_period_frames:512,ring_buffer_milliseconds:180,minimum_mix_ahead_frames:1024}}else{OutputPolicy{preferred_period_frames:256,ring_buffer_milliseconds:120,minimum_mix_ahead_frames:768}};
    }
    #[allow(unreachable_code)]
    OutputPolicy{preferred_period_frames:PREFERRED_DEVICE_BUFFER_FRAMES,ring_buffer_milliseconds:RING_BUFFER_MILLISECONDS,minimum_mix_ahead_frames:TARGET_MIX_AHEAD_FRAMES}
}

struct OutputSession{stream:backend::Stream,ring:Arc<SpscF32Ring>,output_rate:u32,output_channels:usize,device_period_frames:usize,source_rate_hint:Option<u32>}

fn create_output_session(
    device:&backend::Device,
    preferred_source_rate:Option<u32>,
    underruns:Arc<AtomicUsize>,
    active:Arc<AtomicBool>,
    last_error:Arc<Mutex<Option<String>>>,
)->Result<OutputSession,String>{
    let policy=platform_output_policy();
    let supported=choose_output_config(device,preferred_source_rate).map_err(|e|format!("failed to choose output configuration: {e}"))?;
    let sample_format=supported.sample_format();
    let buffer_size=supported.buffer_size().clone();
    let mut config=supported.config();
    if let SupportedBufferSize::Range{min,max}=buffer_size{config.buffer_size=BufferSize::Fixed(policy.preferred_period_frames.clamp(min,max));}
    let output_rate=config.sample_rate;let output_channels=config.channels as usize;
    if output_rate==0||output_channels==0{return Err("playback backend returned an invalid output format".into())}
    let ring_frames=(output_rate as usize*policy.ring_buffer_milliseconds/1000).max(policy.minimum_mix_ahead_frames*2);
    let ring=Arc::new(SpscF32Ring::new(ring_frames*output_channels));
    let period=match config.buffer_size{BufferSize::Fixed(frames)=>frames as usize,_=>MIX_BLOCK_FRAMES};
    let stream=build_device_stream(device,config,sample_format,ring.clone(),underruns,active,last_error).map_err(|e|format!("failed to build audio output stream: {e}"))?;
    stream.play().map_err(|e|format!("failed to start audio output stream: {e}"))?;
    Ok(OutputSession{stream,ring,output_rate,output_channels,device_period_frames:period,source_rate_hint:preferred_source_rate})
}

fn choose_output_config(device: &backend::Device, preferred_source_rate: Option<u32>) -> Result<SupportedStreamConfig, backend::Error> {
    let default = device.default_output_config()?;
    let mut best: Option<(i32, SupportedStreamConfig)> = None;

    for range in device.supported_output_configs()? {
        if !supported_pcm_format(range.sample_format()) { continue; }
        let Some(config) = range.try_with_standard_sample_rate() else { continue; };
        let mut score = 0i32;
        match config.channels() { 2 => score += 1000, 1 => score += 500, _ => {} }
        if config.sample_format() == SampleFormat::F32 { score += 300; }
        if let Some(rate)=preferred_source_rate {
            let delta=config.sample_rate().abs_diff(rate);
            score += 240 - ((delta/100).min(240) as i32);
        } else if config.sample_rate() == 48_000 { score += 100; }
        else if config.sample_rate() == 44_100 { score += 50; }
        if config.sample_format() == default.sample_format() { score += 20; }
        if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, config));
        }
    }

    Ok(best.map(|(_, config)| config).unwrap_or(default))
}

#[inline]
fn supported_pcm_format(format: SampleFormat) -> bool {
    matches!(format,
        SampleFormat::I8 | SampleFormat::I16 | SampleFormat::I24 |
        SampleFormat::I32 | SampleFormat::I64 | SampleFormat::U8 |
        SampleFormat::U16 | SampleFormat::U24 | SampleFormat::U32 |
        SampleFormat::U64 | SampleFormat::F32 | SampleFormat::F64)
}

struct DecodeInner {
    decoder: AudioDecoder<TunedBufReader<File>>,
    output: AudioFrameBuffer,
    channels: usize,
    rate: u32,
}

struct DecodeProducer {
    inner: Mutex<DecodeInner>,
    pcm: Arc<SpscF32Ring>,
    scheduled: AtomicBool,
    eof: AtomicBool,
    error_ready: AtomicBool,
    error: Mutex<Option<String>>,
    preferred_samples: usize,
    requested_samples: AtomicUsize,
}
impl DecodeProducer {
    fn new(decoder:AudioDecoder<TunedBufReader<File>>,rate:u32,channels:usize)->Arc<Self>{
        let preferred=decoder.preferred_frame_samples().unwrap_or(4096*channels).max(channels);
        let ring_samples=((rate as usize*channels)/2).max(preferred*4).next_power_of_two();
        Arc::new(Self{inner:Mutex::new(DecodeInner{decoder,output:AudioFrameBuffer::with_capacity(preferred),channels,rate}),pcm:Arc::new(SpscF32Ring::new(ring_samples)),scheduled:AtomicBool::new(false),eof:AtomicBool::new(false),error_ready:AtomicBool::new(false),error:Mutex::new(None),preferred_samples:preferred,requested_samples:AtomicUsize::new(preferred)})
    }
    fn request_samples(&self, samples: usize) {
        let requested = samples.max(self.preferred_samples);
        let mut current = self.requested_samples.load(Ordering::Relaxed);
        while requested > current {
            match self.requested_samples.compare_exchange_weak(current, requested, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }
    fn schedule(self:&Arc<Self>){if self.eof.load(Ordering::Acquire){return}if self.scheduled.compare_exchange(false,true,Ordering::AcqRel,Ordering::Relaxed).is_ok(){if decoder_pool().try_submit(self.clone()).is_err(){self.scheduled.store(false,Ordering::Release);}}}
    fn fill(&self){
        let started=Instant::now();
        let target = self.requested_samples.swap(self.preferred_samples, Ordering::AcqRel).max(self.preferred_samples);
        let mut produced = 0usize;
        let mut inner=match self.inner.lock(){Ok(v)=>v,Err(_)=>{self.scheduled.store(false,Ordering::Release);return}};
        for _ in 0..8 {
            if produced >= target || self.pcm.free_samples()<self.preferred_samples{break}
            let expected_rate=inner.rate; let expected_channels=inner.channels;
            let result={let DecodeInner{decoder,output,..}=&mut *inner; decoder.decode_into(output).map(|frame|(frame.sample_rate,frame.channels))};
            match result {
                Ok((frame_rate,frame_channels))=>{if frame_rate!=expected_rate||frame_channels as usize!=expected_channels{if let Ok(mut e)=self.error.lock(){*e=Some("decoder format changed in producer".into());self.error_ready.store(true,Ordering::Release);}self.eof.store(true,Ordering::Release);break}for sample in inner.output.samples_mut(){*sample=clean_source_sample(*sample);}let written=self.pcm.write(inner.output.samples()); produced=produced.saturating_add(written); if written<inner.output.len(){break}},
                Err(AudioError::NoNextFrame)=>{self.eof.store(true,Ordering::Release);break},
                Err(error)=>{if let Ok(mut e)=self.error.lock(){*e=Some(error.to_string());self.error_ready.store(true,Ordering::Release);}self.eof.store(true,Ordering::Release);break},
            }
        }
        DECODER_CPU_NS.fetch_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64,Ordering::Relaxed);
        self.scheduled.store(false,Ordering::Release);
    }
    fn fill_inline(&self){self.fill()}
    fn seek(&self,sample:u64)->Result<u64,String>{
        let mut inner=self.inner.lock().map_err(|_|"decoder producer lock poisoned".to_string())?;
        self.pcm.clear(); inner.output.clear(); self.eof.store(false,Ordering::Release); self.error_ready.store(false,Ordering::Release); if let Ok(mut e)=self.error.lock(){*e=None;}
        let mut position=inner.decoder.seek_to_sample(sample).map_err(|e|e.to_string())?.ok_or_else(||"decoder does not support native seeking".to_string())?;
        // Indexed codecs resume at or before the target. Decode only the small
        // indexed gap and queue the unconsumed tail so playback starts exactly
        // at the requested sample without reopening or losing frame remainder.
        while position<sample {
            let channels=inner.channels; let rate=inner.rate;
            let result={let DecodeInner{decoder,output,..}=&mut *inner;decoder.decode_into(output).map(|f|(f.sample_rate,f.channels,f.samples.len()))};
            match result {
                Ok((frame_rate,frame_channels,sample_len))=>{if frame_rate!=rate||frame_channels as usize!=channels{return Err("decoder format changed while seeking".into())}let frames=(sample_len/channels) as u64;if frames==0{break}if position+frames>sample{let skip=((sample-position) as usize)*channels;for value in inner.output.samples_mut(){*value=clean_source_sample(*value);}let _=self.pcm.write(&inner.output.samples()[skip..]);position=sample;break}position+=frames;},
                Err(AudioError::NoNextFrame)=>{self.eof.store(true,Ordering::Release);break},
                Err(error)=>return Err(error.to_string()),
            }
        }
        Ok(position)
    }
    fn take_error(&self)->Option<String>{if !self.error_ready.swap(false,Ordering::AcqRel){return None}self.error.lock().ok()?.take()}
}

struct DecodeJobQueue {
    slots: Box<[UnsafeCell<MaybeUninit<Arc<DecodeProducer>>>]>,
    capacity: usize,
    mask: usize,
    read_index: CachePaddedIndex,
    write_index: CachePaddedIndex,
}
unsafe impl Sync for DecodeJobQueue {}
impl DecodeJobQueue {
    fn new(capacity:usize)->Self{let capacity=capacity.max(2).next_power_of_two();Self{slots:(0..capacity).map(|_|UnsafeCell::new(MaybeUninit::uninit())).collect::<Vec<_>>().into_boxed_slice(),capacity,mask:capacity-1,read_index:CachePaddedIndex::new(0),write_index:CachePaddedIndex::new(0)}}
    fn try_push(&self,job:Arc<DecodeProducer>)->Result<(),Arc<DecodeProducer>>{let read=self.read_index.load(Ordering::Acquire);let write=self.write_index.load(Ordering::Relaxed);if write.wrapping_sub(read)>=self.capacity{return Err(job)}let index=write&self.mask;unsafe{(*self.slots.get_unchecked(index).get()).write(job);}self.write_index.store(write.wrapping_add(1),Ordering::Release);Ok(())}
    fn pop(&self)->Option<Arc<DecodeProducer>>{let read=self.read_index.load(Ordering::Relaxed);let write=self.write_index.load(Ordering::Acquire);if read==write{return None}let index=read&self.mask;let job=unsafe{(*self.slots.get_unchecked(index).get()).assume_init_read()};self.read_index.store(read.wrapping_add(1),Ordering::Release);Some(job)}
}
struct DecodeWorker { queue: Arc<DecodeJobQueue>, thread: thread::Thread }
struct DecodeWorkerPool { workers: Vec<DecodeWorker>, next: AtomicUsize }
impl DecodeWorkerPool {
    fn new()->Self{
        let count=thread::available_parallelism().map(|n|n.get()).unwrap_or(2).clamp(1,4);let mut workers=Vec::with_capacity(count);
        for index in 0..count {let queue=Arc::new(DecodeJobQueue::new(16));let worker_queue=queue.clone();let handle=thread::Builder::new().name(format!("mata-audio-decode-{index}")).spawn(move||{tune_worker(index); loop{if let Some(job)=worker_queue.pop(){job.fill();continue}thread::park_timeout(Duration::from_millis(4));}}).expect("failed to spawn decoder worker");let worker_thread=handle.thread().clone();drop(handle);workers.push(DecodeWorker{queue,thread:worker_thread});}
        Self{workers,next:AtomicUsize::new(0)}
    }
    fn try_submit(&self,job:Arc<DecodeProducer>)->Result<(),Arc<DecodeProducer>>{if self.workers.is_empty(){return Err(job)}let start=self.next.fetch_add(1,Ordering::Relaxed)%self.workers.len();let mut pending=job;for offset in 0..self.workers.len(){let worker=&self.workers[(start+offset)%self.workers.len()];match worker.queue.try_push(pending){Ok(())=>{worker.thread.unpark();return Ok(())},Err(job)=>pending=job}}Err(pending)}
}
fn decoder_pool()->&'static DecodeWorkerPool{DECODER_POOL.get_or_init(DecodeWorkerPool::new)}

#[derive(Clone, Copy)]
struct VoiceKey { index: usize, generation: u32 }
struct VoiceSlot { generation: u32, id: u64, voice: Option<Voice> }
struct VoiceStore {
    slots: Vec<VoiceSlot>,
    free: Vec<usize>,
    ids: HashMap<u64, VoiceKey>,
    active_count: usize,
}
impl VoiceStore {
    fn new() -> Self { Self { slots: Vec::new(), free: Vec::new(), ids: HashMap::new(), active_count: 0 } }
    fn insert(&mut self,id:u64,voice:Voice){
        self.remove(id);
        let active=voice.is_active();
        let index=if let Some(index)=self.free.pop(){let slot=&mut self.slots[index];slot.id=id;slot.voice=Some(voice);index}else{let index=self.slots.len();self.slots.push(VoiceSlot{generation:0,id,voice:Some(voice)});index};
        let generation=self.slots[index].generation; self.ids.insert(id,VoiceKey{index,generation}); if active{self.active_count+=1;}
    }
    fn get_mut(&mut self,id:u64)->Option<&mut Voice>{let key=*self.ids.get(&id)?;let slot=self.slots.get_mut(key.index)?;if slot.generation!=key.generation{return None}slot.voice.as_mut()}
    fn mutate<R>(&mut self,id:u64,f:impl FnOnce(&mut Voice)->R)->Option<R>{
        let key=*self.ids.get(&id)?;let slot=self.slots.get_mut(key.index)?;if slot.generation!=key.generation{return None}
        let voice=slot.voice.as_mut()?;let before=voice.is_active();let result=f(voice);let after=voice.is_active();
        match (before,after){(false,true)=>self.active_count=self.active_count.saturating_add(1),(true,false)=>self.active_count=self.active_count.saturating_sub(1),_=>{}}
        Some(result)
    }
    fn remove(&mut self,id:u64)->Option<Voice>{let key=self.ids.remove(&id)?;let slot=self.slots.get_mut(key.index)?;if slot.generation!=key.generation{return None}let voice=slot.voice.take()?;if voice.is_active(){self.active_count=self.active_count.saturating_sub(1);}slot.generation=slot.generation.wrapping_add(1);self.free.push(key.index);Some(voice)}
    fn active_count(&self)->usize{self.active_count}
}

struct Mixer {
    output_rate: u32,
    output_channels: usize,
    device_period_frames: usize,
    ring: Arc<SpscF32Ring>,
    active_flag: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    voices: VoiceStore,
    mix_buffer: Vec<f32>,
    failures: Vec<(u64,String)>,
    mix_block_frames: usize,
    target_mix_ahead_frames: usize,
    conservative_peak: f32,
    mixer_cpu_ns: u64,
    underruns: Arc<AtomicUsize>,
    last_underruns: usize,
    limiter_mode: u32,
    limiter_kernel: fn(&mut [f32]),
}

impl Mixer {
    fn new(output_rate:u32,output_channels:usize,device_period_frames:usize,ring:Arc<SpscF32Ring>,active_flag:Arc<AtomicBool>,last_error:Arc<Mutex<Option<String>>>,underruns:Arc<AtomicUsize>) -> Self {
        Self{output_rate,output_channels,device_period_frames:device_period_frames.max(64),ring,active_flag,last_error,voices:VoiceStore::new(),mix_buffer:Vec::with_capacity(MIX_BLOCK_FRAMES*output_channels),failures:Vec::with_capacity(8),mix_block_frames:MIX_BLOCK_FRAMES,target_mix_ahead_frames:TARGET_MIX_AHEAD_FRAMES,conservative_peak:0.0,mixer_cpu_ns:0,underruns,last_underruns:0,limiter_mode:u32::MAX,limiter_kernel:limiter_disabled}
    }
    fn preferred_source_rate(&self)->Option<u32>{
        let mut sum=0u64;let mut count=0u64;for slot in &self.voices.slots{if let Some(v)=slot.voice.as_ref(){if v.is_active(){sum=sum.saturating_add(v.source_rate as u64);count+=1;}}}if count==0{None}else{Some((sum/count).clamp(8_000,384_000) as u32)}
    }
    fn reconfigure_output(&mut self,rate:u32,channels:usize,period:usize,ring:Arc<SpscF32Ring>){
        self.output_rate=rate;self.output_channels=channels.max(1);self.device_period_frames=period.max(64);self.ring=ring;self.mix_buffer.clear();
        let policy=platform_output_policy();self.target_mix_ahead_frames=policy.minimum_mix_ahead_frames.max(self.device_period_frames*2);
        for slot in &mut self.voices.slots{if let Some(v)=slot.voice.as_mut(){v.cached_output_rate=0;v.rebuild_route_matrix(self.output_channels);}}
    }
    fn handle_command(&mut self,command:Command){
        match command {
            Command::Open{id,path,autoplay,state,fast}=>match Voice::open(path,autoplay,state.clone(),fast){Ok(v)=>self.voices.insert(id,v),Err(error)=>{if let Ok(mut s)=state.lock(){s.playing=false;s.paused=false;s.stopped=true;s.last_error=Some(error.clone());}set_error(&self.last_error,error);}},
            Command::Play(id)=>{self.voices.mutate(id,|v|v.set_playing(true));},
            Command::Pause(id)=>{self.voices.mutate(id,|v|v.set_playing(false));},
            Command::Toggle(id)=>{self.voices.mutate(id,|v|{let playing=v.paused;v.set_playing(playing)});},
            Command::Stop(id)=>{self.voices.mutate(id,|v|v.stop());},
            Command::Seek(id,seconds)=>if let Some(result)=self.voices.mutate(id,|v|{let result=v.seek(seconds);if let Err(error)=&result{set_source_error(&v.state,error.clone());}result}){if let Err(error)=result{set_error(&self.last_error,error);}},
            Command::SetVolume(id,volume)=>{self.voices.mutate(id,|v|v.set_volume(volume));},
            Command::SetLooping(id,looping)=>{self.voices.mutate(id,|v|v.set_looping(looping));},
            Command::SetBackgroundDecode(id,enabled)=>{self.voices.mutate(id,|v|v.background_decode=enabled);},
            Command::TrimMemory(id)=>{self.voices.mutate(id,|v|v.trim_memory_non_realtime());},
            Command::Remove(id)=>{self.voices.remove(id);}
        }
        self.update_active_flag();
    }
    fn has_audio(&self)->bool{self.voices.active_count()!=0}
    fn update_active_flag(&self){self.active_flag.store(self.has_audio(),Ordering::Release)}
    fn adapt_from_ring(&mut self){
        let active=self.voices.active_count();
        let period_base=(self.device_period_frames/4).clamp(128,1024);
        self.mix_block_frames=match active{0|1=>period_base,2..=8=>(period_base*3/2).min(self.device_period_frames),9..=32=>(period_base*2).min(self.device_period_frames*2),_=>(period_base*3).min(self.device_period_frames*2)}.max(128);
        let observed=self.underruns.load(Ordering::Relaxed);
        if observed>self.last_underruns { self.target_mix_ahead_frames=(self.target_mix_ahead_frames+self.mix_block_frames).min(8192); }
        else { let base=(self.mix_block_frames*4).clamp(512,4096); self.target_mix_ahead_frames=self.target_mix_ahead_frames.saturating_sub(16).max(base); }
        self.last_underruns=observed;
    }
    fn mix_and_queue(&mut self,frames:usize){
        let started=Instant::now(); let samples=frames*self.output_channels;
        if self.mix_buffer.len()<samples{self.mix_buffer.resize(samples,0.0);} self.mix_buffer[..samples].fill(0.0);
        self.failures.clear(); self.conservative_peak=0.0;
        let mut became_inactive=0usize; let mut retained_voice_bytes=0usize;
        for slot in &mut self.voices.slots { let Some(voice)=slot.voice.as_mut() else{continue}; retained_voice_bytes=retained_voice_bytes.saturating_add(voice.retained_bytes()); if !voice.is_active(){continue}
            if let Err(error)=voice.render_add(&mut self.mix_buffer[..samples],frames,self.output_rate,self.output_channels){set_source_error(&voice.state,error.clone());self.failures.push((slot.id,error));}
            else if !voice.is_active(){became_inactive=became_inactive.saturating_add(1);}
        }
        VOICE_RETAINED_BYTES.store(retained_voice_bytes,Ordering::Relaxed); VOICE_HIGH_WATER_BYTES.fetch_max(retained_voice_bytes, Ordering::Relaxed);
        self.voices.active_count=self.voices.active_count.saturating_sub(became_inactive);
        for (id,error) in self.failures.drain(..){self.voices.remove(id);set_error(&self.last_error,format!("audio source {id} failed: {error}"));}
        for &sample in &self.mix_buffer[..samples]{self.conservative_peak=self.conservative_peak.max(sample.abs());}
        if self.conservative_peak>1.0{let mode=LIMITER_MODE.load(Ordering::Acquire);if mode!=self.limiter_mode{self.limiter_mode=mode;self.limiter_kernel=match mode{0=>limiter_disabled,1=>limiter_hard,_=>limiter_soft};}(self.limiter_kernel)(&mut self.mix_buffer[..samples]);}
        let written=self.ring.write(&self.mix_buffer[..samples]);debug_assert_eq!(written,samples);
        let elapsed=started.elapsed().as_nanos().min(u64::MAX as u128) as u64; self.mixer_cpu_ns=self.mixer_cpu_ns.saturating_add(elapsed); MIXER_CPU_NS.fetch_add(elapsed,Ordering::Relaxed); RING_FILL_SAMPLES.store(self.ring.available_samples(),Ordering::Relaxed);
        self.update_active_flag();
    }
}

type MixFrameKernel=fn(&mut Voice,&mut[f32],usize,usize,f32);
type RenderKernel=fn(&mut Voice,&mut[f32],usize,u32,usize)->Result<usize,String>;

struct Voice {
    producer: Arc<DecodeProducer>,
    source_rate: u32,
    source_rate_reciprocal: f64,
    source_channels: usize,
    gain: f32,
    gain_vector: [f32; 8],
    interpolation_scratch: [f32; 8],
    route_matrix: Vec<f32>,
    phase_indices: Vec<usize>,
    phase_fractions: Vec<f32>,
    paused: bool,
    stopped: bool,
    looping: bool,
    buffer: Vec<f32>,
    pull_scratch: Vec<f32>,
    background_decode: bool,
    loop_prefix: Vec<f32>,
    loop_prefix_frames: usize,
    head_samples: usize,
    phase_q32: u64,
    step_q32: u64,
    cached_output_rate: u32,
    cached_output_channels: usize,
    render_kernel: RenderKernel,
    mix_frame_kernel: MixFrameKernel,
    absolute_source_frame: u64,
    next_state_publish_frame: u64,
    eof: bool,
    state: Arc<Mutex<AudioSourceState>>,
    fast: Arc<SourceFastState>,
}

impl Voice {
    fn open(
        path: Arc<PathBuf>,
        autoplay: bool,
        state: Arc<Mutex<AudioSourceState>>,
        fast: Arc<SourceFastState>,
    ) -> Result<Self, String> {
        let decoder = open_decoder(path.as_path())?;
        let source_rate = decoder.sample_rate();
        let source_channels = decoder.channels() as usize;
        if source_rate == 0 { return Err("decoder reported a zero sample rate".into()); }
        if source_channels == 0 { return Err("decoder reported zero channels".into()); }
        let duration = decoder_duration_seconds(&decoder);
        let producer=DecodeProducer::new(decoder,source_rate,source_channels);

        if let Ok(mut s) = state.lock() {
            s.duration_seconds = duration;
            s.volume = 1.0;
            s.position_seconds = 0.0;
            s.playing = autoplay;
            s.paused = !autoplay;
            s.stopped = false;
            s.last_error = None;
        }

        Ok(Self {
            producer,
            source_rate,
            source_rate_reciprocal: 1.0 / source_rate as f64,
            source_channels,
            gain: 1.0,
            gain_vector: [1.0; 8],
            interpolation_scratch: [0.0; 8],
            route_matrix: Vec::new(),
            phase_indices: Vec::with_capacity(MIX_BLOCK_FRAMES),
            phase_fractions: Vec::with_capacity(MIX_BLOCK_FRAMES),
            paused: !autoplay,
            stopped: false,
            looping: false,
            buffer: Vec::with_capacity(4096 * source_channels),
            pull_scratch: vec![0.0; 8192 * source_channels],
            background_decode: true,
            loop_prefix: Vec::with_capacity(2048*source_channels),
            loop_prefix_frames: 0,
            head_samples: 0,
            phase_q32: 0,
            step_q32: PHASE_ONE,
            cached_output_rate: 0,
            cached_output_channels: 0,
            render_kernel: Voice::render_linear_kernel,
            mix_frame_kernel: Voice::mix_generic_frame,
            absolute_source_frame: 0,
            next_state_publish_frame: 0,
            eof: false,
            state,
            fast,
        })
    }

    fn is_active(&self) -> bool { !self.paused && !self.stopped }

    fn set_playing(&mut self, playing: bool) {
        if self.stopped && playing {
            let _ = self.seek(0.0);
            self.stopped = false;
        }
        self.paused = !playing;
        self.publish_state_now();
    }

    fn set_volume(&mut self, volume: f32) {
        self.gain = sanitize_gain(volume);
        self.gain_vector.fill(self.gain);
        self.publish_state_now();
    }

    fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
        self.publish_state_now();
    }

    fn stop(&mut self) {
        self.paused = true;
        self.stopped = true;
        let _ = self.seek(0.0);
        self.stopped = true;
        self.publish_state_now();
    }

    fn seek(&mut self, seconds: f64) -> Result<(), String> {
        let seconds=sanitize_seconds(seconds);
        let target=(seconds*self.source_rate as f64).round() as u64;
        let actual=self.producer.seek(target)?;
        // Keep every retained allocation: only logical lengths/heads are reset.
        self.buffer.clear(); self.head_samples=0;
        self.phase_q32=0; self.absolute_source_frame=actual; self.eof=false; self.stopped=false;
        self.publish_state_now(); Ok(())
    }

    fn restart_loop(&mut self)->Result<(),String>{
        if self.loop_prefix_frames!=0 {
            let actual=self.producer.seek(self.loop_prefix_frames as u64)?;
            if actual==self.loop_prefix_frames as u64 {self.buffer.clear();self.buffer.extend_from_slice(&self.loop_prefix);self.head_samples=0;self.phase_q32=0;self.absolute_source_frame=0;self.eof=false;self.stopped=false;return Ok(())}
        }
        self.seek(0.0)
    }

    fn retained_bytes(&self) -> usize {
        self.route_matrix.capacity().saturating_mul(core::mem::size_of::<f32>())
            .saturating_add(self.phase_indices.capacity().saturating_mul(core::mem::size_of::<usize>()))
            .saturating_add(self.phase_fractions.capacity().saturating_mul(core::mem::size_of::<f32>()))
            .saturating_add(self.buffer.capacity().saturating_mul(core::mem::size_of::<f32>()))
            .saturating_add(self.pull_scratch.capacity().saturating_mul(core::mem::size_of::<f32>()))
            .saturating_add(self.loop_prefix.capacity().saturating_mul(core::mem::size_of::<f32>()))
    }

    fn trim_memory_non_realtime(&mut self) {
        self.route_matrix.shrink_to_fit(); self.phase_indices.shrink_to_fit(); self.phase_fractions.shrink_to_fit();
        self.buffer.shrink_to_fit(); self.pull_scratch.shrink_to_fit(); self.loop_prefix.shrink_to_fit();
        if let Ok(mut inner)=self.producer.inner.lock(){inner.output.shrink_to_fit_non_realtime();inner.decoder.trim_memory();}
    }

    fn render_add(&mut self,mix:&mut[f32],frames:usize,output_rate:u32,output_channels:usize)->Result<usize,String>{
        if self.paused||self.stopped{return Ok(0)}
        if self.cached_output_rate!=output_rate||self.cached_output_channels!=output_channels{
            self.cached_output_rate=output_rate;self.cached_output_channels=output_channels;
            self.step_q32=(((self.source_rate as u128)<<32)/output_rate.max(1) as u128).min(u64::MAX as u128) as u64;
            self.rebuild_route_matrix(output_channels);
            self.render_kernel=if self.step_q32==PHASE_ONE&&self.source_channels==output_channels{Voice::render_equal_kernel}else{Voice::render_linear_kernel};
        }
        (self.render_kernel)(self,mix,frames,output_rate,output_channels)
    }

    fn render_equal_kernel(&mut self,mix:&mut[f32],frames:usize,_output_rate:u32,output_channels:usize)->Result<usize,String>{
        if !self.ensure_frame(frames)? { if self.eof { if self.looping{self.restart_loop()?}else{self.paused=true;self.stopped=true;} } return Ok(0); }
        let count=frames.min(self.available_frames()).min(mix.len()/output_channels);
        let samples=count*output_channels; let start=self.head_samples;
        crate::simd::mix_add(&mut mix[..samples],&self.buffer[start..start+samples],self.gain);
        self.phase_q32=self.phase_q32.saturating_add((count as u64)<<32);self.discard_consumed();self.sync_state();Ok(count)
    }

    fn render_linear_kernel(&mut self,mix:&mut[f32],frames:usize,_output_rate:u32,output_channels:usize)->Result<usize,String>{
        self.phase_indices.clear();self.phase_fractions.clear();
        let mut phase=self.phase_q32;
        for _ in 0..frames {self.phase_indices.push((phase>>32) as usize);self.phase_fractions.push((phase as u32 as f32)*(1.0/4294967296.0));phase=phase.wrapping_add(self.step_q32);}
        let needed=self.phase_indices.last().copied().unwrap_or(0)+1;
        if !self.ensure_frame(needed)? {if self.eof {if self.looping{self.restart_loop()?}else{self.paused=true;self.stopped=true;}}}
        let available=self.available_frames(); let mut rendered=0usize;
        for i in 0..self.phase_indices.len() {let index0=self.phase_indices[i];let fraction=self.phase_fractions[i];if index0+1>=available{break}let base=rendered*output_channels;(self.mix_frame_kernel)(self,&mut mix[base..base+output_channels],index0,index0+1,fraction);rendered+=1;}
        self.phase_q32=self.phase_q32.wrapping_add(self.step_q32.saturating_mul(rendered as u64));self.discard_consumed();self.sync_state();Ok(rendered)
    }

    fn rebuild_route_matrix(&mut self,output_channels:usize){
        self.route_matrix.clear(); self.route_matrix.resize(self.source_channels*output_channels,0.0);
        if output_channels==1 { let w=1.0/self.source_channels.max(1) as f32; for c in 0..self.source_channels{self.route_matrix[c]=w;} }
        else if self.source_channels==1 { for o in 0..output_channels{self.route_matrix[o]=1.0;} }
        else { for c in 0..self.source_channels.min(output_channels){self.route_matrix[c*output_channels+c]=1.0;} }
        self.mix_frame_kernel=match(self.source_channels,output_channels){(1,1)=>Voice::mix_mono_mono,(1,2)=>Voice::mix_mono_stereo,(2,1)=>Voice::mix_stereo_mono,(2,2)=>Voice::mix_stereo_stereo,_=>Voice::mix_generic_frame};
    }

    fn mix_one_output_frame(&mut self,output:&mut[f32],index0:usize,index1:usize,fraction:f32){(self.mix_frame_kernel)(self,output,index0,index1,fraction)}
    fn mix_mono_mono(&mut self,output:&mut[f32],i0:usize,i1:usize,t:f32){output[0]+=lerp(self.sample(i0,0),self.sample(i1,0),t)*self.gain;}
    fn mix_mono_stereo(&mut self,output:&mut[f32],i0:usize,i1:usize,t:f32){let v=lerp(self.sample(i0,0),self.sample(i1,0),t)*self.gain;output[0]+=v;output[1]+=v;}
    fn mix_stereo_mono(&mut self,output:&mut[f32],i0:usize,i1:usize,t:f32){let a=(self.sample(i0,0)+self.sample(i0,1))*0.5;let b=(self.sample(i1,0)+self.sample(i1,1))*0.5;output[0]+=lerp(a,b,t)*self.gain;}
    fn mix_stereo_stereo(&mut self,output:&mut[f32],i0:usize,i1:usize,t:f32){output[0]+=lerp(self.sample(i0,0),self.sample(i1,0),t)*self.gain;output[1]+=lerp(self.sample(i0,1),self.sample(i1,1),t)*self.gain;}
    fn mix_generic_frame(&mut self,output:&mut[f32],index0:usize,index1:usize,fraction:f32){
        if self.source_channels<=self.interpolation_scratch.len(){
            let channels=self.source_channels;let base0=self.head_samples+index0*channels;let base1=self.head_samples+index1*channels;
            let(buffer,scratch)=(&self.buffer,&mut self.interpolation_scratch);crate::simd::lerp_slice(&buffer[base0..base0+channels],&buffer[base1..base1+channels],fraction,&mut scratch[..channels]);
            for o in 0..output.len(){let mut value=0.0;for c in 0..channels{let w=self.route_matrix.get(c*output.len()+o).copied().unwrap_or(0.0);if w!=0.0{value+=scratch[c]*w;}}output[o]+=value*self.gain;}
        }else{for o in 0..output.len(){let mut value=0.0;for c in 0..self.source_channels{let w=self.route_matrix.get(c*output.len()+o).copied().unwrap_or(0.0);if w!=0.0{value+=lerp(self.sample(index0,c),self.sample(index1,c),fraction)*w;}}output[o]+=value*self.gain;}}
    }

    fn ensure_frame(&mut self,frame:usize)->Result<bool,String>{
        let mut idle_spins=0usize;
        let deficit_frames=(frame+1).saturating_sub(self.available_frames());
        self.producer.request_samples(deficit_frames.saturating_mul(self.source_channels));
        while self.available_frames()<=frame {
            let available=self.producer.pcm.available_samples();
            if available!=0 {
                let take=available.min(self.pull_scratch.len());
                let read=self.producer.pcm.read_f32(&mut self.pull_scratch[..take]);
                if self.absolute_source_frame==0 && self.loop_prefix.len()<2048*self.source_channels {let remaining=2048*self.source_channels-self.loop_prefix.len();let copy=read.min(remaining);self.loop_prefix.extend_from_slice(&self.pull_scratch[..copy]);self.loop_prefix_frames=self.loop_prefix.len()/self.source_channels;}
                self.buffer.reserve(read); self.buffer.extend_from_slice(&self.pull_scratch[..read]); idle_spins=0;
                // Prefetch before the decoded queue is exhausted so the central
                // mixer normally consumes PCM without waiting for a decoder.
                if self.background_decode && self.producer.pcm.available_samples() < self.producer.preferred_samples * 2 { self.producer.schedule(); }
                continue;
            }
            if let Some(error)=self.producer.take_error(){return Err(error)}
            if self.producer.eof.load(Ordering::Acquire){self.eof=true;return Ok(false)}
            if self.background_decode { self.producer.schedule(); } else { self.producer.fill_inline(); }
            if self.background_decode { idle_spins+=1; if idle_spins>=8{return Ok(false)} thread::yield_now(); }
        }
        Ok(true)
    }

    #[inline]
    fn available_frames(&self) -> usize {
        self.buffer.len().saturating_sub(self.head_samples) / self.source_channels
    }

    #[inline]
    fn sample(&self, frame: usize, channel: usize) -> f32 {
        self.buffer[self.head_samples + frame * self.source_channels + channel]
    }

    fn discard_consumed(&mut self) {
        let frames=((self.phase_q32>>32) as usize).min(self.available_frames());
        if frames==0{return}
        self.head_samples+=frames*self.source_channels; self.phase_q32-= (frames as u64)<<32; self.absolute_source_frame=self.absolute_source_frame.saturating_add(frames as u64);
        if self.head_samples==self.buffer.len(){self.buffer.clear();self.head_samples=0;}
        else if self.head_samples>=16*1024 && self.head_samples>=self.buffer.len()/2 {
            let remaining=self.buffer.len()-self.head_samples; self.buffer.copy_within(self.head_samples..,0); self.buffer.truncate(remaining); self.head_samples=0;
        }
    }

    fn publish_state_now(&mut self) {
        let position=(self.absolute_source_frame as f64+(self.phase_q32 as f64/PHASE_ONE as f64))*self.source_rate_reciprocal;
        self.fast.publish(!self.paused&&!self.stopped,self.paused&&!self.stopped,self.stopped,self.looping,self.gain,position);
        let cadence=(self.source_rate/30).max(1) as u64;
        self.next_state_publish_frame=self.absolute_source_frame.saturating_add(cadence);
    }

    fn sync_state(&mut self) {
        if self.absolute_source_frame >= self.next_state_publish_frame { self.publish_state_now(); }
    }
}

fn open_decoder(path: &Path) -> Result<AudioDecoder<TunedBufReader<File>>, String> {
    let file = File::open(path).map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    let reader = TunedBufReader::with_capacity(DEFAULT_IO_CAPACITY, file);
    AudioDecoder::new(reader).map_err(|e| format!("failed to create decoder for {}: {e}", path.display()))
}

fn decoder_duration_seconds(decoder: &AudioDecoder<TunedBufReader<File>>) -> Option<f64> {
    match decoder {
        AudioDecoder::Wav(d) => Some(d.duration_seconds()),
        AudioDecoder::Aiff(d) => Some(d.duration_seconds()),
        AudioDecoder::Flac(d) => d.duration_seconds(),
        _ => None,
    }
}

fn validate_audio_frame(
    samples: &[f32],
    frame_rate: u32,
    frame_channels: u16,
    expected_rate: u32,
    expected_channels: usize,
) -> Result<(), String> {
    if frame_rate != expected_rate {
        return Err(format!("decoder changed sample rate from {expected_rate} Hz to {frame_rate} Hz"));
    }
    if frame_channels as usize != expected_channels {
        return Err(format!("decoder changed channel count from {expected_channels} to {frame_channels}"));
    }
    if samples.len() % expected_channels != 0 {
        return Err(format!("decoder returned {} samples for {expected_channels} channels", samples.len()));
    }
    Ok(())
}

fn build_device_stream(
    device: &backend::Device,
    config: backend::StreamConfig,
    format: SampleFormat,
    ring: Arc<SpscF32Ring>,
    underruns: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
) -> Result<backend::Stream, backend::Error> {
    match format {
        SampleFormat::I8 => build_stream::<i8>(device, config, ring, underruns, active, last_error),
        SampleFormat::I16 => build_stream_i16(device, config, ring, underruns, active, last_error),
        SampleFormat::I24 => build_stream::<I24>(device, config, ring, underruns, active, last_error),
        SampleFormat::I32 => build_stream::<i32>(device, config, ring, underruns, active, last_error),
        SampleFormat::I64 => build_stream::<i64>(device, config, ring, underruns, active, last_error),
        SampleFormat::U8 => build_stream::<u8>(device, config, ring, underruns, active, last_error),
        SampleFormat::U16 => build_stream::<u16>(device, config, ring, underruns, active, last_error),
        SampleFormat::U24 => build_stream::<U24>(device, config, ring, underruns, active, last_error),
        SampleFormat::U32 => build_stream::<u32>(device, config, ring, underruns, active, last_error),
        SampleFormat::U64 => build_stream::<u64>(device, config, ring, underruns, active, last_error),
        SampleFormat::F32 => build_stream_f32(device, config, ring, underruns, active, last_error),
        SampleFormat::F64 => build_stream::<f64>(device, config, ring, underruns, active, last_error),
        _ => Err(backend::Error::with_message(
            backend::ErrorKind::InvalidInput,
            format!("unsupported PCM output format {format}"),
        )),
    }
}

fn build_stream_f32(device:&backend::Device,config:backend::StreamConfig,ring:Arc<SpscF32Ring>,underruns:Arc<AtomicUsize>,active:Arc<AtomicBool>,_last_error:Arc<Mutex<Option<String>>>)->Result<backend::Stream,backend::Error>{
    let mut previous=Instant::now();
    device.build_output_stream(config,move|output:&mut[f32],_:&backend::OutputCallbackInfo|{let now=Instant::now();CALLBACK_PERIOD_NS.store(now.duration_since(previous).as_nanos().min(u64::MAX as u128) as u64,Ordering::Relaxed);previous=now;let count=ring.read_f32(output);RING_FILL_SAMPLES.store(ring.available_samples(),Ordering::Relaxed);if count<output.len(){output[count..].fill(0.0);if active.load(Ordering::Acquire){underruns.fetch_add(1,Ordering::Relaxed);}}},move|_error|RT_ERROR_CODE.store(1,Ordering::Release),None)
}
fn build_stream_i16(device:&backend::Device,config:backend::StreamConfig,ring:Arc<SpscF32Ring>,underruns:Arc<AtomicUsize>,active:Arc<AtomicBool>,_last_error:Arc<Mutex<Option<String>>>)->Result<backend::Stream,backend::Error>{
    let mut previous=Instant::now();
    device.build_output_stream(config,move|output:&mut[i16],_:&backend::OutputCallbackInfo|{let now=Instant::now();CALLBACK_PERIOD_NS.store(now.duration_since(previous).as_nanos().min(u64::MAX as u128) as u64,Ordering::Relaxed);previous=now;let count=ring.read_i16(output);RING_FILL_SAMPLES.store(ring.available_samples(),Ordering::Relaxed);if count<output.len(){output[count..].fill(0);if active.load(Ordering::Acquire){underruns.fetch_add(1,Ordering::Relaxed);}}},move|_error|RT_ERROR_CODE.store(1,Ordering::Release),None)
}

fn build_stream<T>(
    device: &backend::Device,
    config: backend::StreamConfig,
    ring: Arc<SpscF32Ring>,
    underruns: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    _last_error: Arc<Mutex<Option<String>>>,
) -> Result<backend::Stream, backend::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let mut previous=Instant::now();
    device.build_output_stream(
        config,
        move |output: &mut [T], _: &backend::OutputCallbackInfo| {
            let now=Instant::now(); CALLBACK_PERIOD_NS.store(now.duration_since(previous).as_nanos().min(u64::MAX as u128) as u64,Ordering::Relaxed); previous=now;
            let count = ring.read_into(output);
            RING_FILL_SAMPLES.store(ring.available_samples(), Ordering::Relaxed);
            if count < output.len() {
                for sample in &mut output[count..] { *sample = T::from_sample_(0.0); }
                if active.load(Ordering::Acquire) {
                    underruns.fetch_add(1, Ordering::Relaxed);
                }
            }
        },
        move |_error| { RT_ERROR_CODE.store(1,Ordering::Release); },
        None,
    )
}

#[repr(align(64))]
struct CachePaddedIndex(AtomicUsize);
impl CachePaddedIndex { fn new(v:usize)->Self{Self(AtomicUsize::new(v))} #[inline] fn load(&self,o:Ordering)->usize{self.0.load(o)} #[inline] fn store(&self,v:usize,o:Ordering){self.0.store(v,o)} }

struct SpscF32Ring {
    slots: Box<[UnsafeCell<f32>]>, capacity: usize, mask: usize, read_index: CachePaddedIndex, write_index: CachePaddedIndex,
}

unsafe impl Sync for SpscF32Ring {}

impl SpscF32Ring {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1).next_power_of_two();
        Self {
            slots: (0..capacity).map(|_| UnsafeCell::new(0.0)).collect::<Vec<_>>().into_boxed_slice(),
            capacity,
            mask: capacity - 1,
            read_index: CachePaddedIndex::new(0),
            write_index: CachePaddedIndex::new(0),
        }
    }

    fn clear(&self) { let write=self.write_index.load(Ordering::Acquire); self.read_index.store(write,Ordering::Release); }

    #[inline]
    fn occupancy(&self) -> (usize, usize) {
        let read = self.read_index.load(Ordering::Acquire);
        let write = self.write_index.load(Ordering::Acquire);
        let available = write.wrapping_sub(read).min(self.capacity);
        (available, self.capacity - available)
    }

    #[inline]
    fn available_samples(&self) -> usize { self.occupancy().0 }

    #[inline]
    fn free_samples(&self) -> usize { self.occupancy().1 }

    #[inline]
    fn readable_spans(&self,maximum:usize)->((usize,usize),(usize,usize)){let read=self.read_index.load(Ordering::Relaxed);let write=self.write_index.load(Ordering::Acquire);let count=write.wrapping_sub(read).min(self.capacity).min(maximum);let start=read&self.mask;let first=count.min(self.capacity-start);((start,first),(0,count-first))}

    fn write(&self, input: &[f32]) -> usize {
        let read = self.read_index.load(Ordering::Acquire);
        let write = self.write_index.load(Ordering::Relaxed);
        let used = write.wrapping_sub(read).min(self.capacity);
        let count = (self.capacity - used).min(input.len());
        if count == 0 { return 0; }
        let start = write & self.mask;
        let first = count.min(self.capacity - start);
        unsafe {
            for i in 0..first { *self.slots.get_unchecked(start + i).get() = *input.get_unchecked(i); }
            for i in first..count { *self.slots.get_unchecked(i - first).get() = *input.get_unchecked(i); }
        }
        self.write_index.store(write.wrapping_add(count), Ordering::Release);
        count
    }

    fn read_f32(&self, output:&mut[f32])->usize {
        let read=self.read_index.load(Ordering::Relaxed);let write=self.write_index.load(Ordering::Acquire);let count=write.wrapping_sub(read).min(self.capacity).min(output.len());if count==0{return 0}
        let start=read&self.mask;let first=count.min(self.capacity-start);unsafe{core::ptr::copy_nonoverlapping(self.slots.as_ptr().add(start).cast::<f32>(),output.as_mut_ptr(),first);if first<count{core::ptr::copy_nonoverlapping(self.slots.as_ptr().cast::<f32>(),output.as_mut_ptr().add(first),count-first);}}
        self.read_index.store(read.wrapping_add(count),Ordering::Release);count
    }

    fn read_i16(&self, output:&mut[i16])->usize {
        let read=self.read_index.load(Ordering::Relaxed);let write=self.write_index.load(Ordering::Acquire);let count=write.wrapping_sub(read).min(self.capacity).min(output.len());if count==0{return 0}
        let start=read&self.mask;let first=count.min(self.capacity-start);
        unsafe{let a=core::slice::from_raw_parts(self.slots.as_ptr().add(start).cast::<f32>(),first);crate::simd::f32_to_i16(a,&mut output[..first]);if first<count{let b=core::slice::from_raw_parts(self.slots.as_ptr().cast::<f32>(),count-first);crate::simd::f32_to_i16(b,&mut output[first..count]);}}
        self.read_index.store(read.wrapping_add(count),Ordering::Release);count
    }

    fn read_into<T>(&self, output: &mut [T]) -> usize
    where T: SizedSample + FromSample<f32>
    {
        let read = self.read_index.load(Ordering::Relaxed);
        let write = self.write_index.load(Ordering::Acquire);
        let available = write.wrapping_sub(read).min(self.capacity);
        let count = available.min(output.len());
        if count == 0 { return 0; }
        let start = read & self.mask;
        let first = count.min(self.capacity - start);
        unsafe {
            for i in 0..first {
                *output.get_unchecked_mut(i) = T::from_sample_(*self.slots.get_unchecked(start + i).get());
            }
            for i in first..count {
                *output.get_unchecked_mut(i) = T::from_sample_(*self.slots.get_unchecked(i - first).get());
            }
        }
        self.read_index.store(read.wrapping_add(count), Ordering::Release);
        count
    }
}

#[inline(always)]
fn sanitize_gain(gain: f32) -> f32 { if gain.is_finite() { gain.max(0.0) } else { 0.0 } }
#[inline(always)]
fn sanitize_seconds(seconds: f64) -> f64 { if seconds.is_finite() { seconds.max(0.0) } else { 0.0 } }
#[inline(always)]
fn clean_source_sample(sample: f32) -> f32 {
    let bits=sample.to_bits(); let exponent=bits&0x7f80_0000; let mantissa=bits&0x007f_ffff;
    if exponent!=0x7f80_0000 && (exponent!=0 || mantissa==0) { sample } else { sanitize_non_normal_sample(exponent,mantissa) }
}
#[cold]
#[inline(never)]
fn sanitize_non_normal_sample(exponent:u32,mantissa:u32)->f32{if exponent==0&&mantissa!=0{DENORMAL_SAMPLES_SANITIZED.fetch_add(1,Ordering::Relaxed);}0.0}
#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
#[inline]fn limiter_disabled(_samples:&mut[f32]){}
#[inline]fn limiter_hard(samples:&mut[f32]){crate::simd::limit_samples(samples)}
fn limiter_soft(samples:&mut[f32]){for sample in samples{*sample=limit_sample(*sample);}}

#[inline(always)]
fn limit_sample(sample: f32) -> f32 {
    if !sample.is_finite() { return 0.0; }
    #[cfg(feature="fast-math")]
    {
        let a=sample.abs();if a>=1.2{return sample.signum().clamp(-1.0,0.999_999_94);}let y=0.15*a;return (sample*(1.0-y+y*y)).clamp(-1.0,0.999_999_94);
    }
    #[cfg(not(feature="fast-math"))]
    { (sample / (1.0 + sample.abs() * 0.15)).clamp(-1.0, 0.999_999_94) }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    #[test]
    fn spsc_wrap_is_two_spans_and_preserves_order(){let ring=SpscF32Ring::new(8);assert_eq!(ring.write(&[1.,2.,3.,4.,5.,6.]),6);let mut first=[0.0;5];assert_eq!(ring.read_f32(&mut first),5);assert_eq!(ring.write(&[7.,8.,9.,10.,11.]),5);let spans=ring.readable_spans(8);assert!(spans.0.1+spans.1.1<=8);let mut out=[0.0;6];assert_eq!(ring.read_f32(&mut out),6);assert_eq!(out,[6.,7.,8.,9.,10.,11.]);}
    #[test]
    fn fixed_phase_exact_equal_rate(){let mut phase=0u64;for _ in 0..1_000_000{phase=phase.wrapping_add(PHASE_ONE);}assert_eq!(phase>>32,1_000_000);}
    #[test]
    fn hard_limiter_is_bounded(){let mut s=[-2.0,-1.0,0.0,1.0,2.0,f32::NAN];crate::simd::limit_samples(&mut s);assert!(s.iter().all(|x|x.is_finite()&&(-1.0..=1.0).contains(x)));}
}

#[cfg(test)]
mod hardening_spsc_tests {
    use super::SpscF32Ring;
    use std::{sync::Arc,thread,time::{Duration,Instant}};

    #[test]
    fn repeated_wraparound_preserves_fifo_and_capacity() {
        let ring=SpscF32Ring::new(7); // rounds to 8
        let mut next_write=0u32;let mut next_read=0u32;
        for step in 0..25000 {
            let amount=(step*17%13) as usize;
            let payload:Vec<f32>=(next_write..next_write+amount as u32).map(|v|v as f32).collect();
            let written=ring.write(&payload);
            next_write+=written as u32;
            let mut out=[0.0;11];let read=ring.read_f32(&mut out[..step*11%12]);
            for v in &out[..read] {assert_eq!(*v,next_read as f32);next_read+=1;}
            assert!(ring.available_samples()<=8);
        }
        let mut remainder=[0.0;16];let n=ring.read_f32(&mut remainder);
        for v in &remainder[..n] {assert_eq!(*v,next_read as f32);next_read+=1;}
        assert_eq!(next_write,next_read);
    }

    #[test]
    fn concurrent_spsc_producer_consumer_preserves_every_sample() {
        const TOTAL:usize=200_000;
        let ring=Arc::new(SpscF32Ring::new(256));
        let producer={let ring=ring.clone();thread::spawn(move || {
            let mut written=0usize;
            let start=Instant::now();
            while written<TOTAL {
                assert!(start.elapsed()<Duration::from_secs(30),"producer stalled");
                let slice:Vec<f32>=(written..(written+37).min(TOTAL)).map(|v|v as f32).collect();
                let n=ring.write(&slice);written+=n;
                if n==0 {thread::yield_now();}
            }
        })};
        let mut read=0usize;let mut out=[0.0;41];let start=Instant::now();
        while read<TOTAL {
            assert!(start.elapsed()<Duration::from_secs(30),"consumer stalled");
            let count=ring.read_f32(&mut out);
            for &value in &out[..count] {assert_eq!(value,read as f32);read+=1;}
            if count==0 {thread::yield_now();}
        }
        producer.join().unwrap();
        assert_eq!(ring.available_samples(),0);
    }

    #[test]
    fn forced_underruns_many_queues_and_wraps_stay_bounded() {
        let rings=(0..128).map(|_|SpscF32Ring::new(16)).collect::<Vec<_>>();
        for cycle in 0..500 {
            for (index,ring) in rings.iter().enumerate() {
                let data=[(index+cycle) as f32;23];
                assert!(ring.write(&data)<=16);
                let mut read=[0.0;47];
                let actual=ring.read_f32(&mut read);
                assert!(actual<=16);
                assert!(read[..actual].iter().all(|&v|v==(index+cycle) as f32));
                assert_eq!(ring.read_f32(&mut read),0);
            }
        }
    }
}

#[cfg(test)]
mod hardening_command_tests {
    use super::{Command, COMMAND_QUEUE_CAPACITY, MAX_COMMANDS_PER_TICK};
    use std::sync::mpsc::{self,TrySendError,TryRecvError};
    #[test]
    fn saturated_bounded_command_channel_does_not_block_or_reorder() {
        let (sender,receiver)=mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        for id in 0..COMMAND_QUEUE_CAPACITY {assert!(sender.try_send(Command::Play(id as u64)).is_ok());}
        assert!(matches!(sender.try_send(Command::Pause(0)),Err(TrySendError::Full(_))));
        let mut last=0usize;
        // Mimic the bounded work performed in each mix tick.
        while last<COMMAND_QUEUE_CAPACITY {
            for _ in 0..MAX_COMMANDS_PER_TICK {
                match receiver.try_recv() {
                    Ok(Command::Play(id))=>{assert_eq!(id,last as u64);last+=1;},
                    Ok(_)=>panic!("unexpected command type"),
                    Err(TryRecvError::Empty)=>break,
                    Err(e)=>panic!("{e}"),
                }
            }
        }
        assert!(matches!(receiver.try_recv(),Err(TryRecvError::Empty)));
        drop(receiver);
        assert!(matches!(sender.try_send(Command::Stop(0)),Err(TrySendError::Disconnected(_))));
    }
}
