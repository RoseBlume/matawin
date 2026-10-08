//! Reproducible, dependency-free performance harness for P0-P30 and P61-P120.
//! Run from the crate root with:
//! `cargo run --release --no-default-features --features benchmarking,auto-detect,simd --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/current.csv`

use mata_audio::codecs::benchmark_support;
use mata_audio::codecs::pcm::{decode_interleaved_into, PcmEncoding, PcmFormat};
use mata_audio::{AudioDecoder, AudioError, AudioFrameBuffer, Decoder};
use mata_audio::simd;
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::{self, File};
use std::hint::black_box;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct CountingAllocator;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        if new_size > old.size() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, old, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}
#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[derive(Clone)]
struct CorpusEntry {
    id: String,
    path: PathBuf,
    reference: PathBuf,
    channels: u16,
    sample_rate: u32,
    tolerance: f32,
}

#[derive(Default)]
struct DecodeMetrics {
    open_ns: u128,
    decode_ns: u128,
    frames: u64,
    decode_calls: u64,
    samples: u64,
    allocs: u64,
    allocated_bytes: u64,
    hash: u64,
    p50_frame_ns: u128,
    p95_frame_ns: u128,
    max_abs_error: f32,
    reference_samples: usize,
    peak_rss_kib: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let manifest = PathBuf::from(args.next().unwrap_or_else(|| "performance/corpus/manifest.tsv".into()));
    let output = PathBuf::from(args.next().unwrap_or_else(|| "performance/results/current.csv".into()));
    let entries = read_manifest(&manifest)?;
    fs::create_dir_all(output.parent().unwrap_or(Path::new(".")))?;

    let mut out = File::create(&output)?;
    writeln!(out, "kind,id,metric,value,unit,compiler,features,isa")?;
    let compiler = compiler_version();
    let features = active_features();
    let isa = format!("{:?}", simd::runtime_isa());

    for entry in &entries {
        let metrics = benchmark_decode(entry)?;
        emit(&mut out, "decode", &entry.id, "open_latency", metrics.open_ns as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "full_decode", metrics.decode_ns as f64, "ns", &compiler, &features, &isa)?;
        let seconds = metrics.frames as f64 / entry.sample_rate.max(1) as f64;
        let throughput = if metrics.decode_ns == 0 { 0.0 } else { seconds / (metrics.decode_ns as f64 / 1e9) };
        emit(&mut out, "decode", &entry.id, "realtime_factor", throughput, "x", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "decode_into_p50", metrics.p50_frame_ns as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "decode_into_p95", metrics.p95_frame_ns as f64, "ns", &compiler, &features, &isa)?;
        let (next_p50, next_p95) = benchmark_next_frame(entry)?;
        emit(&mut out, "decode", &entry.id, "next_frame_p50", next_p50 as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "next_frame_p95", next_p95 as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "allocations_per_frame", metrics.allocs as f64 / metrics.decode_calls.max(1) as f64, "alloc/frame", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "allocated_bytes_per_audio_second", metrics.allocated_bytes as f64 / seconds.max(1e-9), "bytes/s", &compiler, &features, &isa)?;
        emit(&mut out, "decode", &entry.id, "peak_rss", metrics.peak_rss_kib as f64, "KiB", &compiler, &features, &isa)?;
        emit_text(&mut out, "correctness", &entry.id, "pcm_fnv1a64", &format!("0x{:016x}", metrics.hash), "hash", &compiler, &features, &isa)?;
        emit(&mut out, "correctness", &entry.id, "max_abs_error", metrics.max_abs_error as f64, "f32", &compiler, &features, &isa)?;
        if metrics.max_abs_error > entry.tolerance {
            return Err(format!(
                "{} exceeded numerical tolerance: {} > {}",
                entry.id, metrics.max_abs_error, entry.tolerance
            ).into());
        }
        let (seek_ns, loop_ns) = reopen_latency(entry)?;
        emit(&mut out, "timeline", &entry.id, "reopen_seek_half", seek_ns as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "timeline", &entry.id, "loop_reopen_first_frame", loop_ns as f64, "ns", &compiler, &features, &isa)?;
        let t = Instant::now();
        let mut memory_decoder = AudioDecoder::open_small_file_in_memory(&entry.path, 64 * 1024 * 1024)?;
        let memory_open_ns = t.elapsed().as_nanos();
        let mut memory_buffer = AudioFrameBuffer::default();
        let t = Instant::now();
        loop {
            match memory_decoder.decode_into(&mut memory_buffer) {
                Ok(frame) => { black_box(frame.samples.len()); }
                Err(AudioError::NoNextFrame) => break,
                Err(error) => return Err(error.into()),
            }
        }
        emit(&mut out, "io", &entry.id, "mmap_style_memory_resident_open", memory_open_ns as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "io", &entry.id, "mmap_style_memory_resident_decode", t.elapsed().as_nanos() as f64, "ns", &compiler, &features, &isa)?;
    }

    benchmark_pcm_regressions(&mut out, &compiler, &features, &isa)?;
    benchmark_shared_primitives(&mut out, &compiler, &features, &isa)?;
    benchmark_synthetic_playback(&mut out, &compiler, &features, &isa)?;
    benchmark_many_decoders(&entries, &mut out, &compiler, &features, &isa)?;
    benchmark_long_steady_state(&entries, &mut out, &compiler, &features, &isa)?;
    #[cfg(feature = "perf-instrumentation")]
    for (name, calls, total_ns, max_ns) in benchmark_support::stage_snapshot() {
        emit(&mut out, "stage", name, "calls", calls as f64, "count", &compiler, &features, &isa)?;
        emit(&mut out, "stage", name, "total", total_ns as f64, "ns", &compiler, &features, &isa)?;
        emit(&mut out, "stage", name, "max", max_ns as f64, "ns", &compiler, &features, &isa)?;
    }
    write_summary(&output)?;
    println!("wrote {}", output.display());
    Ok(())
}

fn read_manifest(path: &Path) -> Result<Vec<CorpusEntry>, Box<dyn std::error::Error>> {
    let root = path.parent().unwrap_or(Path::new("."));
    let reference_root = root.parent().unwrap_or(root).join("reference");
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index == 0 || line.trim().is_empty() { continue; }
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 9 { return Err(format!("bad manifest row {}", index + 1).into()); }
        out.push(CorpusEntry {
            id: c[0].into(), path: root.join(c[1]), channels: c[3].parse()?, sample_rate: c[4].parse()?,
            reference: reference_root.join(c[5]), tolerance: c[8].parse()?,
        });
    }
    Ok(out)
}

fn open(path: &Path) -> Result<AudioDecoder<BufReader<File>>, AudioError> {
    AudioDecoder::new(BufReader::with_capacity(64 * 1024, File::open(path)?))
}

fn benchmark_decode(entry: &CorpusEntry) -> Result<DecodeMetrics, Box<dyn std::error::Error>> {
    let t = Instant::now();
    let mut decoder = open(&entry.path)?;
    let open_ns = t.elapsed().as_nanos();
    if decoder.channels() != entry.channels || decoder.sample_rate() != entry.sample_rate {
        return Err(format!("{} format mismatch", entry.id).into());
    }
    // Load the reference and reserve harness-owned storage before resetting the
    // counting allocator. Allocation metrics below therefore describe the
    // decoder/decode path rather than growth of the benchmark's own vectors.
    let reference = read_f32le(&entry.reference)?;
    let preferred_samples = decoder.preferred_frame_samples().unwrap_or(4096).max(1);
    let expected_calls = reference.len().div_ceil(preferred_samples).saturating_add(8);
    let mut buffer = AudioFrameBuffer::with_capacity(preferred_samples);
    let mut frame_times = Vec::with_capacity(expected_calls);
    let mut decoded_pcm = Vec::with_capacity(reference.len());
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    let start = Instant::now();
    loop {
        let fstart = Instant::now();
        match decoder.decode_into(&mut buffer) {
            Ok(frame) => {
                frame_times.push(fstart.elapsed().as_nanos());
                decoded_pcm.extend_from_slice(frame.samples);
            }
            Err(AudioError::NoNextFrame) => break,
            Err(e) => return Err(e.into()),
        }
    }
    let decode_ns = start.elapsed().as_nanos();
    let allocs = ALLOCS.load(Ordering::Relaxed); let bytes = BYTES.load(Ordering::Relaxed);
    frame_times.sort_unstable();
    let p50 = percentile(&frame_times, 50); let p95 = percentile(&frame_times, 95);
    let max_abs_error = compare_pcm(&decoded_pcm, &reference);
    Ok(DecodeMetrics {
        open_ns, decode_ns,
        frames: decoded_pcm.len() as u64 / entry.channels.max(1) as u64,
        decode_calls: frame_times.len() as u64,
        samples: decoded_pcm.len() as u64,
        allocs, allocated_bytes: bytes,
        hash: fnv_pcm(&decoded_pcm), p50_frame_ns: p50, p95_frame_ns: p95,
        max_abs_error, reference_samples: reference.len(), peak_rss_kib: peak_rss_kib(),
    })
}

fn benchmark_next_frame(entry: &CorpusEntry) -> Result<(u128,u128), Box<dyn std::error::Error>> {
    let mut decoder = open(&entry.path)?;
    let mut times = Vec::new();
    loop {
        let start = Instant::now();
        match decoder.next_frame() {
            Ok(frame) => { black_box(frame.samples.len()); times.push(start.elapsed().as_nanos()); }
            Err(AudioError::NoNextFrame) => break,
            Err(error) => return Err(error.into()),
        }
    }
    times.sort_unstable();
    Ok((percentile(&times, 50), percentile(&times, 95)))
}

fn reopen_latency(entry: &CorpusEntry) -> Result<(u128,u128), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let mut d = open(&entry.path)?; let mut b = AudioFrameBuffer::default();
    let target = entry.sample_rate as u64 / 4; let mut decoded = 0u64;
    while decoded < target {
        match d.decode_into(&mut b) { Ok(f) => decoded += f.frame_count() as u64, Err(AudioError::NoNextFrame) => break, Err(e) => return Err(e.into()) }
    }
    let seek = start.elapsed().as_nanos();
    let start = Instant::now(); let mut d = open(&entry.path)?; let mut b = AudioFrameBuffer::default(); let _ = d.decode_into(&mut b); let loop_ns = start.elapsed().as_nanos();
    Ok((seek, loop_ns))
}


fn benchmark_pcm_regressions(out: &mut File, compiler: &str, features: &str, isa: &str) -> Result<(), Box<dyn std::error::Error>> {
    let samples = 16 * 1024usize;
    let mut frame = AudioFrameBuffer::with_capacity(samples);
    let cases: &[(&str, PcmEncoding, u16)] = &[
        ("wav_u8", PcmEncoding::Unsigned8, 8),
        ("wav_s16", PcmEncoding::Signed16Le, 16),
        ("wav_s24", PcmEncoding::Signed24Le, 24),
        ("wav_s32", PcmEncoding::Signed32Le, 32),
        ("wav_f32", PcmEncoding::Float32Le, 32),
        ("wav_f64", PcmEncoding::Float64Le, 64),
        ("aiff_s8", PcmEncoding::Signed8, 8),
        ("aiff_s16be", PcmEncoding::Signed16Be, 16),
        ("aiff_s24be", PcmEncoding::Signed24Be, 24),
        ("aiff_s32be", PcmEncoding::Signed32Be, 32),
        ("aifc_s16le", PcmEncoding::Signed16Le, 16),
        ("aifc_s24le", PcmEncoding::Signed24Le, 24),
        ("aifc_s32le", PcmEncoding::Signed32Le, 32),
    ];

    for &(name, encoding, bits) in cases {
        let data = synthetic_pcm_bytes(encoding, samples);
        let format = PcmFormat { encoding, channels: 1, sample_rate: 48_000, valid_bits_per_sample: bits };
        // Warm the dispatch/cache path before measuring the steady-state loop.
        decode_interleaved_into(&data, format, &mut frame)?;
        let start = Instant::now();
        let mut checksum = 0u64;
        for _ in 0..256 {
            decode_interleaved_into(black_box(&data), format, &mut frame)?;
            checksum ^= fnv_pcm(frame.samples());
        }
        emit(out, "pcm", name, "convert_256x", start.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
        emit_text(out, "correctness", name, "checksum", &format!("0x{checksum:016x}"), "hash", compiler, features, isa)?;
    }
    Ok(())
}

fn synthetic_pcm_bytes(encoding: PcmEncoding, samples: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.saturating_mul(encoding.bytes_per_sample()));
    for i in 0..samples {
        let centered = ((i as i32 * 7919) & 0xffff) - 32768;
        match encoding {
            PcmEncoding::Unsigned8 => out.push(((centered >> 8) + 128) as u8),
            PcmEncoding::Signed8 => out.push((centered >> 8) as i8 as u8),
            PcmEncoding::Signed16Le => out.extend_from_slice(&(centered as i16).to_le_bytes()),
            PcmEncoding::Signed16Be => out.extend_from_slice(&(centered as i16).to_be_bytes()),
            PcmEncoding::Signed24Le => {
                let v = centered << 8; let b = v.to_le_bytes(); out.extend_from_slice(&b[..3]);
            }
            PcmEncoding::Signed24Be => {
                let v = centered << 8; let b = v.to_be_bytes(); out.extend_from_slice(&b[1..]);
            }
            PcmEncoding::Signed32Le => out.extend_from_slice(&(centered << 16).to_le_bytes()),
            PcmEncoding::Signed32Be => out.extend_from_slice(&(centered << 16).to_be_bytes()),
            PcmEncoding::Float32Le => out.extend_from_slice(&((centered as f32 / 32768.0).clamp(-1.0, 1.0)).to_le_bytes()),
            PcmEncoding::Float32Be => out.extend_from_slice(&((centered as f32 / 32768.0).clamp(-1.0, 1.0)).to_be_bytes()),
            PcmEncoding::Float64Le => out.extend_from_slice(&((centered as f64 / 32768.0).clamp(-1.0, 1.0)).to_le_bytes()),
            PcmEncoding::Float64Be => out.extend_from_slice(&((centered as f64 / 32768.0).clamp(-1.0, 1.0)).to_be_bytes()),
        }
    }
    out
}

fn benchmark_shared_primitives(out: &mut File, compiler: &str, features: &str, isa: &str) -> std::io::Result<()> {
    let bytes: Vec<u8> = (0..(2 * 1024 * 1024)).map(|i| (i as u8).wrapping_mul(31)).collect();
    let tests: [(&str, Box<dyn Fn() -> u64>); 5] = [
        ("sequential_read", Box::new(|| benchmark_support::sequential_read(bytes.len(), 64))),
        ("prefix_consume", Box::new(|| benchmark_support::prefix_consume(bytes.len(), 16 * 1024, 64))),
        ("compact_refill", Box::new(|| benchmark_support::compact_refill(bytes.len(), 64))),
        ("bit_reservoir64", Box::new(|| benchmark_support::bit_reservoir64(bytes.len(), 64))),
        ("bit_reservoir128", Box::new(|| benchmark_support::bit_reservoir128(bytes.len(), 64))),
    ];
    for (name, f) in tests {
        let t = Instant::now();
        let checksum = black_box(f());
        emit(out, "micro", name, "elapsed", t.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
        emit_text(out, "correctness", name, "checksum", &format!("0x{checksum:016x}"), "hash", compiler, features, isa)?;
    }
    let mut samples = vec![0.25f32; 4096];
    let t=Instant::now(); for _ in 0..10000 { simd::benchmark_scalar_gain(black_box(&mut samples), black_box(0.99999)); } let scalar_ns=t.elapsed().as_nanos(); emit(out,"simd","dispatch","gain_scalar_10k",scalar_ns as f64,"ns",compiler,features,isa)?;
    let t=Instant::now(); for _ in 0..10000 { simd::apply_gain(black_box(&mut samples), black_box(0.99999)); } let dispatch_ns=t.elapsed().as_nanos(); emit(out,"simd","dispatch","gain_dispatch_10k",dispatch_ns as f64,"ns",compiler,features,isa)?;
    emit(out,"simd","dispatch","gain_dispatch_overhead",dispatch_ns.saturating_sub(scalar_ns) as f64,"ns",compiler,features,isa)?;
    let t=Instant::now(); for _ in 0..1_000_000 { black_box(simd::runtime_isa()); } emit(out,"simd","dispatch","runtime_isa_lookup_1m",t.elapsed().as_nanos() as f64,"ns",compiler,features,isa)?;
    Ok(())
}

fn benchmark_synthetic_playback(out: &mut File, compiler: &str, features: &str, isa: &str) -> std::io::Result<()> {
    for &voices in &[1usize, 2, 8, 32, 128] {
        let t = Instant::now();
        let (sum, hash) = synthetic_mix(voices, 2, 2, 48_000, 48_000, 4096);
        black_box(sum);
        emit(out, "playback", "synthetic", &format!("mix_{voices}_voices"), t.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
        emit_text(out, "correctness", "synthetic", &format!("mix_{voices}_voices_hash"), &format!("0x{hash:016x}"), "hash", compiler, features, isa)?;
    }
    for &(name, si, so, ri, ro) in &[
        ("mono_mono", 1, 1, 48_000, 48_000),
        ("mono_stereo", 1, 2, 48_000, 48_000),
        ("stereo_stereo", 2, 2, 48_000, 48_000),
        ("stereo_mono", 2, 1, 48_000, 48_000),
        ("resample_equal_rate", 2, 2, 48_000, 48_000),
        ("resample_44100_to_48000", 2, 2, 44_100, 48_000),
    ] {
        let t = Instant::now();
        let (sum, hash) = synthetic_mix(8, si, so, ri, ro, 8192);
        black_box(sum);
        emit(out, "playback", "synthetic", name, t.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
        emit_text(out, "correctness", "synthetic", &format!("{name}_hash"), &format!("0x{hash:016x}"), "hash", compiler, features, isa)?;
    }
    let mut occupancy = 0isize;
    let mut underruns = 0u64;
    let t = Instant::now();
    for i in 0..1_000_000 {
        if i % 7 == 0 { occupancy += 256; }
        occupancy -= 64;
        if occupancy < 0 { underruns += 1; occupancy = 0; }
    }
    black_box(underruns);
    emit(out, "playback", "synthetic", "underrun_stress", t.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
    emit_text(out, "correctness", "synthetic", "underrun_count", &format!("0x{underruns:016x}"), "hash", compiler, features, isa)?;

    let (tx, rx) = std::sync::mpsc::sync_channel::<u64>(1);
    let t = Instant::now();
    let mut command_checksum = 0u64;
    for i in 0..10_000 {
        tx.send(i).unwrap();
        command_checksum = command_checksum.wrapping_add(black_box(rx.recv().unwrap()));
    }
    emit(out, "playback", "synthetic", "command_roundtrip_10k", t.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
    emit_text(out, "correctness", "synthetic", "command_roundtrip_checksum", &format!("0x{command_checksum:016x}"), "hash", compiler, features, isa)?;

    let t = Instant::now();
    let (sum, hash) = synthetic_mix(8, 2, 2, 44_100, 48_000, 48_000 * 60);
    black_box(sum);
    emit(out, "playback", "synthetic", "steady_state_60_audio_seconds", t.elapsed().as_nanos() as f64, "ns", compiler, features, isa)?;
    emit_text(out, "correctness", "synthetic", "steady_state_60_audio_seconds_hash", &format!("0x{hash:016x}"), "hash", compiler, features, isa)?;
    Ok(())
}

fn benchmark_many_decoders(entries: &[CorpusEntry], out:&mut File, compiler:&str, features:&str, isa:&str) -> Result<(),Box<dyn std::error::Error>> {
    if entries.is_empty(){return Ok(())} let entry=&entries[0]; let t=Instant::now(); let mut decoders=Vec::new(); for _ in 0..32 { decoders.push((open(&entry.path)?,AudioFrameBuffer::default())); }
    for (d,b) in &mut decoders { let _=d.decode_into(b); } emit(out,"decode",&entry.id,"32_decoder_startup_first_frame",t.elapsed().as_nanos() as f64,"ns",compiler,features,isa)?; Ok(())
}

fn benchmark_long_steady_state(entries:&[CorpusEntry],out:&mut File,compiler:&str,features:&str,isa:&str)->Result<(),Box<dyn std::error::Error>>{
    if entries.is_empty(){return Ok(())} let entry=&entries[0]; let t=Instant::now(); let mut hash=0u64; for _ in 0..20 { let mut d=open(&entry.path)?; let mut b=AudioFrameBuffer::default(); loop { match d.decode_into(&mut b){Ok(f)=>hash^=fnv_pcm(f.samples),Err(AudioError::NoNextFrame)=>break,Err(e)=>return Err(e.into())} } } black_box(hash); emit(out,"decode",&entry.id,"steady_state_20_loops",t.elapsed().as_nanos() as f64,"ns",compiler,features,isa)?; Ok(())
}

fn synthetic_mix(voices:usize,in_ch:usize,out_ch:usize,in_rate:u32,out_rate:u32,frames:usize)->(f32,u64){
    let source_frames=((frames as u64*in_rate as u64/out_rate as u64)+2) as usize; let mut src=vec![0.0f32;source_frames*in_ch]; for (i,s) in src.iter_mut().enumerate(){*s=((i*17%257) as f32/128.0)-1.0} let mut out=vec![0.0f32;frames*out_ch]; let step=in_rate as f64/out_rate as f64;
    for v in 0..voices { let gain=1.0/voices as f32; for f in 0..frames { let p=f as f64*step; let a=p as usize; let b=(a+1).min(source_frames-1); let q=(p-a as f64) as f32; for oc in 0..out_ch { let sample=if in_ch==1 {src[a]*(1.0-q)+src[b]*q} else if out_ch==1 {(0..in_ch).map(|c|src[a*in_ch+c]*(1.0-q)+src[b*in_ch+c]*q).sum::<f32>()/in_ch as f32} else if oc<in_ch {src[a*in_ch+oc]*(1.0-q)+src[b*in_ch+oc]*q}else{0.0}; out[f*out_ch+oc]+=sample*gain*(1.0+v as f32*0.0); } } }
    let sum = out.iter().copied().sum();
    let hash = fnv_pcm(&out);
    (sum, hash)
}

fn read_f32le(path:&Path)->Result<Vec<f32>,std::io::Error>{let mut bytes=Vec::new();File::open(path)?.read_to_end(&mut bytes)?;Ok(bytes.chunks_exact(4).map(|b|f32::from_le_bytes([b[0],b[1],b[2],b[3]])).collect())}
fn compare_pcm(a:&[f32],b:&[f32])->f32{let mut m=if a.len()==b.len(){0.0}else{f32::INFINITY};for (&x,&y) in a.iter().zip(b){m=m.max((x-y).abs())}m}
fn fnv_pcm(samples:&[f32])->u64{let mut h=0xcbf29ce484222325u64;for s in samples{for b in s.to_bits().to_le_bytes(){h^=b as u64;h=h.wrapping_mul(0x100000001b3)}}h}
fn percentile(v:&[u128],pct:usize)->u128{if v.is_empty(){0}else{v[((v.len()-1)*pct/100).min(v.len()-1)]}}
fn compiler_version()->String{std::process::Command::new("rustc").arg("--version").output().ok().and_then(|o|String::from_utf8(o.stdout).ok()).unwrap_or_else(||"unknown".into()).trim().replace(',',';')}
fn active_features()->String{
    let mut f=Vec::new();
    macro_rules! feature { ($name:literal) => { if cfg!(feature=$name) { f.push($name); } }; }
    feature!("simd"); feature!("benchmarking"); feature!("perf-instrumentation"); feature!("auto-detect");
    feature!("codec-flac"); feature!("codec-mp1"); feature!("codec-mp2"); feature!("codec-mp3"); feature!("codec-pcm"); feature!("codec-vorbis");
    feature!("demux-aiff"); feature!("demux-flac"); feature!("demux-matroska"); feature!("demux-ogg"); feature!("demux-wav");
    feature!("metadata-ape"); feature!("metadata-id3v1"); feature!("metadata-id3v2"); feature!("metadata-iso-mp4"); feature!("metadata-riff"); feature!("metadata-vorbis-flac"); feature!("metadata-vorbis-ogg");
    f.join("+")
}
fn emit(w:&mut File,kind:&str,id:&str,metric:&str,value:f64,unit:&str,compiler:&str,features:&str,isa:&str)->std::io::Result<()>{writeln!(w,"{kind},{id},{metric},{value:.6},{unit},{compiler},{features},{isa}")}
fn emit_text(w:&mut File,kind:&str,id:&str,metric:&str,value:&str,unit:&str,compiler:&str,features:&str,isa:&str)->std::io::Result<()>{writeln!(w,"{kind},{id},{metric},{value},{unit},{compiler},{features},{isa}")}
#[cfg(target_os = "linux")]
fn peak_rss_kib() -> u64 {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("VmHWM:") {
                return value
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
            }
        }
    }
    0
}

#[cfg(target_os = "windows")]
fn peak_rss_kib() -> u64 {
    let pid = std::process::id().to_string();
    let script = format!("(Get-Process -Id {pid}).PeakWorkingSet64");
    for shell in ["powershell.exe", "pwsh.exe"] {
        if let Ok(output) = std::process::Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
        {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if let Ok(bytes) = text.trim().parse::<u64>() { return bytes / 1024; }
                }
            }
        }
    }
    0
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn peak_rss_kib() -> u64 {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

fn write_summary(csv: &Path) -> std::io::Result<()> {
    let data = fs::read_to_string(csv)?;
    let md = csv.with_extension("md");
    let mut report = File::create(md)?;
    let mut decode_rows = Vec::<(String, String, String, String)>::new();
    let mut regressible_rows = 0usize;
    for line in data.lines().skip(1) {
        let columns: Vec<&str> = line.split(',').collect();
        if columns.len() < 8 { continue; }
        if columns[0] == "decode" && matches!(columns[2], "realtime_factor" | "open_latency" | "allocations_per_frame" | "peak_rss") {
            decode_rows.push((columns[1].into(), columns[2].into(), columns[3].into(), columns[4].into()));
        }
        if columns[4] != "hash" { regressible_rows += 1; }
    }
    writeln!(report, "# mata-audio performance run")?;
    writeln!(report, "\n- Generated: {:?}", std::time::SystemTime::now())?;
    writeln!(report, "- Rows: {}", data.lines().count().saturating_sub(1))?;
    writeln!(report, "- Numeric/regressible rows: {regressible_rows}")?;
    writeln!(report, "- Runtime ISA: `{:?}`", simd::runtime_isa())?;
    writeln!(report, "\n## Decoder summary\n")?;
    writeln!(report, "| Corpus | Metric | Value | Unit |")?;
    writeln!(report, "|---|---|---:|---|")?;
    for (id, metric, value, unit) in decode_rows {
        writeln!(report, "| {id} | {metric} | {value} | {unit} |")?;
    }
    writeln!(report, "\nThe CSV beside this report is the machine-readable source of truth.")?;
    Ok(())
}
