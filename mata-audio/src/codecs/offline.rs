//! Offline/high-throughput decoding support.
//!
//! The normal playback engine remains latency-first. This module instead uses a
//! bounded background compressed-byte read-ahead queue so local-file I/O can
//! overlap entropy decoding. It can also decode multiple independent files in
//! parallel. No queue is unbounded and seeking invalidates stale read-ahead.

use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};

use crate::{AudioDecoder, AudioError, AudioFrameBuffer, Decoder, DecoderMemoryUsage};

const DEFAULT_READ_AHEAD_BYTES: usize = 1024 * 1024;
const DEFAULT_CHUNK_BYTES: usize = 64 * 1024;
const MAX_READ_AHEAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct OfflineDecodeOptions {
    /// Maximum compressed bytes retained in the read-ahead queue.
    pub read_ahead_bytes: usize,
    /// Size of each sequential I/O request.
    pub read_chunk_bytes: usize,
    /// Independent-file worker limit used by `decode_files_parallel`.
    pub worker_threads: usize,
    /// Only use channel-parallel postprocessing above this many frames.
    pub channel_parallel_threshold_frames: usize,
}
impl Default for OfflineDecodeOptions {
    fn default() -> Self {
        Self {
            read_ahead_bytes: DEFAULT_READ_AHEAD_BYTES,
            read_chunk_bytes: DEFAULT_CHUNK_BYTES,
            worker_threads: thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 16),
            channel_parallel_threshold_frames: 16 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct OfflineDecodedAudio {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
    pub decoder_memory: DecoderMemoryUsage,
}

struct Chunk { start: u64, data: Box<[u8]>, generation: u64 }
struct ReadAheadState {
    generation: u64,
    next_offset: u64,
    queued_bytes: usize,
    queue: VecDeque<Chunk>,
    eof: bool,
    stop: bool,
}

/// Bounded local-file read-ahead reader. The worker owns a separate file handle,
/// so prefetch I/O never mutates the decoder's logical seek position.
pub struct ReadAheadFile {
    shared: Arc<(Mutex<ReadAheadState>, Condvar)>,
    position: u64,
    length: u64,
    worker: Option<thread::JoinHandle<()>>,
}

impl ReadAheadFile {
    pub fn open(path: impl AsRef<Path>, read_ahead_bytes: usize, chunk_bytes: usize) -> Result<Self, AudioError> {
        let path = path.as_ref().to_path_buf();
        let length = std::fs::metadata(&path)?.len();
        let chunk_bytes = chunk_bytes.clamp(4096, MAX_READ_AHEAD_BYTES);
        let budget = read_ahead_bytes.clamp(chunk_bytes, MAX_READ_AHEAD_BYTES);
        let shared = Arc::new((Mutex::new(ReadAheadState {
            generation: 0, next_offset: 0, queued_bytes: 0,
            queue: VecDeque::with_capacity((budget / chunk_bytes).max(2)),
            eof: false, stop: false,
        }), Condvar::new()));
        let worker_shared = shared.clone();
        let worker_path = path.clone();
        let worker = thread::Builder::new().name("mata-audio-read-ahead".into()).spawn(move || {
            let Ok(mut file) = File::open(worker_path) else { return; };
            loop {
                let (generation, offset) = {
                    let (lock, ready) = &*worker_shared;
                    let mut state = match lock.lock() { Ok(v) => v, Err(_) => return };
                    while !state.stop && (state.eof || state.queued_bytes.saturating_add(chunk_bytes) > budget) {
                        state = match ready.wait(state) { Ok(v) => v, Err(_) => return };
                    }
                    if state.stop { return; }
                    (state.generation, state.next_offset)
                };
                if file.seek(SeekFrom::Start(offset)).is_err() { return; }
                let mut data = vec![0u8; chunk_bytes];
                let read = match file.read(&mut data) { Ok(n) => n, Err(_) => return };
                data.truncate(read);
                let (lock, ready) = &*worker_shared;
                let mut state = match lock.lock() { Ok(v) => v, Err(_) => return };
                if state.generation != generation { continue; }
                if read == 0 { state.eof = true; ready.notify_all(); continue; }
                state.next_offset = offset.saturating_add(read as u64);
                state.queued_bytes = state.queued_bytes.saturating_add(read);
                state.queue.push_back(Chunk { start: offset, data: data.into_boxed_slice(), generation });
                ready.notify_all();
            }
        }).map_err(AudioError::Io)?;
        Ok(Self { shared, position: 0, length, worker: Some(worker) })
    }

    fn seek_absolute(&mut self, target: u64) -> std::io::Result<u64> {
        if target > self.length { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek beyond end of read-ahead file")); }
        self.position = target;
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().map_err(|_| std::io::Error::other("read-ahead lock poisoned"))?;
        state.generation = state.generation.wrapping_add(1);
        state.next_offset = target;
        state.queued_bytes = 0;
        state.queue.clear();
        state.eof = target == self.length;
        ready.notify_all();
        Ok(target)
    }
}

impl Read for ReadAheadFile {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() || self.position >= self.length { return Ok(0); }
        let mut written = 0usize;
        while written < output.len() && self.position < self.length {
            let (lock, ready) = &*self.shared;
            let mut state = lock.lock().map_err(|_| std::io::Error::other("read-ahead lock poisoned"))?;
            loop {
                while state.queue.front().is_some_and(|chunk| chunk.generation != state.generation || chunk.start.saturating_add(chunk.data.len() as u64) <= self.position) {
                    if let Some(chunk) = state.queue.pop_front() { state.queued_bytes = state.queued_bytes.saturating_sub(chunk.data.len()); }
                }
                if let Some(front) = state.queue.front() {
                    let end = front.start.saturating_add(front.data.len() as u64);
                    if front.start <= self.position && self.position < end { break; }
                }
                if state.eof { return Ok(written); }
                ready.notify_one();
                state = ready.wait(state).map_err(|_| std::io::Error::other("read-ahead lock poisoned"))?;
            }
            let front = state.queue.front().expect("read-ahead queue checked above");
            let offset = (self.position - front.start) as usize;
            let count = (front.data.len() - offset).min(output.len() - written);
            output[written..written + count].copy_from_slice(&front.data[offset..offset + count]);
            self.position = self.position.saturating_add(count as u64);
            written += count;
            if offset + count == front.data.len() {
                let chunk = state.queue.pop_front().unwrap();
                state.queued_bytes = state.queued_bytes.saturating_sub(chunk.data.len());
                ready.notify_one();
            }
        }
        Ok(written)
    }
}

impl Seek for ReadAheadFile {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let target: i128 = match position {
            SeekFrom::Start(value) => value as i128,
            SeekFrom::Current(delta) => self.position as i128 + delta as i128,
            SeekFrom::End(delta) => self.length as i128 + delta as i128,
        };
        if target < 0 || target > self.length as i128 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid read-ahead seek"));
        }
        self.seek_absolute(target as u64)
    }
    fn stream_position(&mut self) -> std::io::Result<u64> { Ok(self.position) }
}

impl Drop for ReadAheadFile {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        if let Ok(mut state) = lock.lock() { state.stop = true; ready.notify_all(); }
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

/// Decode one local file using bounded read-ahead. The returned memory report is
/// sampled from the decoder after the full steady-state decode.
pub fn decode_file(path: impl AsRef<Path>, options: OfflineDecodeOptions) -> Result<OfflineDecodedAudio, AudioError> {
    let reader = ReadAheadFile::open(path, options.read_ahead_bytes, options.read_chunk_bytes)?;
    let mut decoder = AudioDecoder::new(reader)?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let mut frame = AudioFrameBuffer::with_capacity(decoder.preferred_frame_samples().unwrap_or(4096));
    let mut samples = Vec::new();
    loop {
        match decoder.decode_into(&mut frame) {
            Ok(decoded) => samples.extend_from_slice(decoded.samples),
            Err(AudioError::NoNextFrame) => break,
            Err(error) => return Err(error),
        }
    }
    sanitize_channels_parallel(
        &mut samples,
        channels as usize,
        options.channel_parallel_threshold_frames,
        options.worker_threads,
    );
    Ok(OfflineDecodedAudio { samples, channels, sample_rate, decoder_memory: decoder.memory_usage() })
}

/// Decode independent files concurrently. Codec-internal entropy dependencies
/// remain sequential; parallelism is used only where streams are independent.
pub fn decode_files_parallel(paths: &[PathBuf], options: OfflineDecodeOptions) -> Vec<Result<OfflineDecodedAudio, AudioError>> {
    if paths.is_empty() { return Vec::new(); }
    let workers = options.worker_threads.max(1).min(paths.len());
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let results: Arc<Vec<Mutex<Option<Result<OfflineDecodedAudio, AudioError>>>>> = Arc::new((0..paths.len()).map(|_| Mutex::new(None)).collect());
    thread::scope(|scope| {
        for _ in 0..workers {
            let next = next.clone(); let results = results.clone();
            scope.spawn(move || loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if index >= paths.len() { break; }
                let result = decode_file(&paths[index], options);
                if let Ok(mut slot) = results[index].lock() { *slot = Some(result); }
            });
        }
    });
    Arc::try_unwrap(results).ok().unwrap().into_iter().map(|slot| {
        slot.into_inner().ok().and_then(|v| v).unwrap_or_else(|| Err(AudioError::custom("offline worker failed to publish a result")))
    }).collect()
}

/// Sanitizes independent channels in parallel only for sufficiently large
/// offline blocks. Each worker owns a disjoint channel lane in the interleaved
/// buffer, so no two workers write the same sample. Tiny blocks remain scalar
/// to avoid thread-launch overhead.
fn sanitize_channels_parallel(samples: &mut [f32], channels: usize, threshold_frames: usize, worker_limit: usize) {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if channels == 1 || frames < threshold_frames.max(1) || worker_limit <= 1 {
        for sample in samples { *sample = sanitize_offline_sample(*sample); }
        return;
    }
    let workers = worker_limit.min(channels).max(1);
    let next_channel = std::sync::atomic::AtomicUsize::new(0);
    // SAFETY: every channel index is handed to exactly one worker. For a fixed
    // channel `c`, that worker writes indices c + n*channels only, which are
    // disjoint from every other channel lane. `thread::scope` prevents the raw
    // pointer from escaping the lifetime of `samples`.
    let base = samples.as_mut_ptr() as usize;
    thread::scope(|scope| {
        for _ in 0..workers {
            let next_channel = &next_channel;
            scope.spawn(move || {
                let ptr = base as *mut f32;
                loop {
                    let channel = next_channel.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if channel >= channels { break; }
                    for frame in 0..frames {
                        let index = frame * channels + channel;
                        // SAFETY: see the disjoint-channel argument above.
                        unsafe {
                            let slot = ptr.add(index);
                            *slot = sanitize_offline_sample(*slot);
                        }
                    }
                }
            });
        }
    });
}

#[cfg(feature = "mmap")]
pub fn decode_mmap_file(path: impl AsRef<Path>) -> Result<OfflineDecodedAudio, AudioError> {
    let file = File::open(path)?;
    // SAFETY: the mapping is read-only and remains alive until decoding ends.
    let mapping = unsafe { memmap2::MmapOptions::new().map(&file).map_err(AudioError::Io)? };
    let cursor = std::io::Cursor::new(mapping.as_ref());
    let mut decoder = AudioDecoder::new(cursor)?;
    let channels = decoder.channels(); let sample_rate = decoder.sample_rate();
    let mut frame = AudioFrameBuffer::with_capacity(decoder.preferred_frame_samples().unwrap_or(4096));
    let mut samples = Vec::new();
    loop { match decoder.decode_into(&mut frame) { Ok(decoded) => samples.extend_from_slice(decoded.samples), Err(AudioError::NoNextFrame) => break, Err(error) => return Err(error) } }
    for sample in &mut samples { *sample = sanitize_offline_sample(*sample); }
    Ok(OfflineDecodedAudio { samples, channels, sample_rate, decoder_memory: decoder.memory_usage() })
}

#[inline]
fn sanitize_offline_sample(sample: f32) -> f32 {
    let bits = sample.to_bits();
    let exponent = bits & 0x7f80_0000;
    let mantissa = bits & 0x007f_ffff;
    if exponent == 0x7f80_0000 || (exponent == 0 && mantissa != 0) { 0.0 } else { sample }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};

    #[test]
    fn read_ahead_seek_invalidates_stale_chunks() {
        let mut path = std::env::temp_dir();
        path.push(format!("mata-audio-read-ahead-{}.bin", std::process::id()));
        std::fs::write(&path, (0u8..=255).cycle().take(200_000).collect::<Vec<_>>()).unwrap();
        let mut reader = ReadAheadFile::open(&path, 32 * 1024, 4096).unwrap();
        let mut a = [0u8; 16]; reader.read_exact(&mut a).unwrap();
        reader.seek(SeekFrom::Start(100_000)).unwrap();
        let mut b = [0u8; 16]; reader.read_exact(&mut b).unwrap();
        assert_eq!(b[0], (100_000usize % 256) as u8);
        let _ = std::fs::remove_file(path);
    }
}
