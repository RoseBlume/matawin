use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mataimage::{AnimationDecoder, AnimationMode, DecodeOptions, DecodeScratch, Image, ImageFormat, ImageInfo, MetadataMode};
use mataimage::metadata::ImageMetadata;
use mataimage::pixels::RgbaConvert;

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static LARGEST_ALLOCATION: AtomicU64 = AtomicU64::new(0);
static TRACK_PEAK: AtomicBool = AtomicBool::new(false);

#[inline]
fn update_peak(live: u64) {
    if !TRACK_PEAK.load(Ordering::Relaxed) {
        return;
    }
    let mut peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while live > peak {
        match PEAK_LIVE_BYTES.compare_exchange_weak(
            peak,
            live,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(current) => peak = current,
        }
    }
}

#[inline]
fn update_largest_allocation(size: u64) {
    if !TRACK_PEAK.load(Ordering::Relaxed) { return; }
    let mut largest = LARGEST_ALLOCATION.load(Ordering::Relaxed);
    while size > largest {
        match LARGEST_ALLOCATION.compare_exchange_weak(
            largest, size, Ordering::Relaxed, Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(current) => largest = current,
        }
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            update_largest_allocation(layout.size() as u64);
            let live = LIVE_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed)
                + layout.size() as u64;
            update_peak(live);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            update_largest_allocation(layout.size() as u64);
            let live = LIVE_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed)
                + layout.size() as u64;
            update_peak(live);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !new_ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            update_largest_allocation(new_size as u64);
            DEALLOCATED_BYTES.fetch_add(old.size() as u64, Ordering::Relaxed);
            if new_size >= old.size() {
                let delta = (new_size - old.size()) as u64;
                let live = LIVE_BYTES.fetch_add(delta, Ordering::Relaxed) + delta;
                update_peak(live);
            } else {
                LIVE_BYTES.fetch_sub((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        new_ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        LIVE_BYTES.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy)]
struct AllocationSnapshot {
    count: u64,
    allocated: u64,
    deallocated: u64,
}

fn allocation_snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        count: ALLOCATIONS.load(Ordering::Relaxed),
        allocated: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated: DEALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

fn allocation_delta(start: AllocationSnapshot, end: AllocationSnapshot) -> AllocationSnapshot {
    AllocationSnapshot {
        count: end.count.saturating_sub(start.count),
        allocated: end.allocated.saturating_sub(start.allocated),
        deallocated: end.deallocated.saturating_sub(start.deallocated),
    }
}

fn begin_peak_tracking() -> u64 {
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(baseline, Ordering::Relaxed);
    LARGEST_ALLOCATION.store(0, Ordering::Relaxed);
    TRACK_PEAK.store(true, Ordering::Relaxed);
    baseline
}

fn end_peak_tracking(baseline: u64) -> (u64, u64) {
    TRACK_PEAK.store(false, Ordering::Relaxed);
    (
        PEAK_LIVE_BYTES.load(Ordering::Relaxed).saturating_sub(baseline),
        LARGEST_ALLOCATION.load(Ordering::Relaxed),
    )
}

fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/assets")
}

fn asset(name: &str) -> PathBuf {
    assets_dir().join(name)
}

fn read_asset(name: &str) -> Vec<u8> {
    fs::read(asset(name)).unwrap_or_else(|error| panic!("failed to read benchmark asset {name}: {error}"))
}

fn print_environment() {
    println!("mataimage P3 benchmark suite");
    println!("profile: {}", if cfg!(debug_assertions) { "debug" } else { "release/bench" });
    println!("os: {}", std::env::consts::OS);
    println!("arch: {}", std::env::consts::ARCH);
    println!(
        "target_features: sse2={} ssse3={} avx2={} neon={}",
        cfg!(target_feature = "sse2"),
        cfg!(target_feature = "ssse3"),
        cfg!(target_feature = "avx2"),
        cfg!(target_feature = "neon"),
    );
    println!("features: simd={} avif={} tiff={} svg={}",
        cfg!(feature = "simd"),
        cfg!(feature = "avif"),
        cfg!(feature = "tiff"),
        cfg!(feature = "svg"));
    println!("rustflags: {}", option_env!("RUSTFLAGS").unwrap_or("<not captured at compile time>"));
    println!("target_cpu: {}", option_env!("MATAIMAGE_TARGET_CPU").unwrap_or("default"));

    if let Ok(output) = Command::new("rustc").arg("--version").output() {
        if output.status.success() {
            println!("compiler: {}", String::from_utf8_lossy(&output.stdout).trim());
        }
    }

    #[cfg(target_os = "linux")]
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        if let Some(line) = cpuinfo.lines().find(|line| line.starts_with("model name")) {
            if let Some((_, value)) = line.split_once(':') {
                println!("cpu: {}", value.trim());
            }
        }
    }

    println!();
}

#[cfg(target_os = "linux")]
fn peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_kib() -> Option<u64> {
    None
}

fn calibrated_iterations<F>(mut operation: F, max_iterations: u64) -> u64
where
    F: FnMut(),
{
    let target = Duration::from_millis(160);
    let mut iterations = 1u64;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        if start.elapsed() >= target || iterations >= max_iterations {
            return iterations;
        }
        iterations = (iterations * 2).min(max_iterations);
    }
}

fn output_bytes(image: &Image) -> u64 {
    let primary = image.pixels.len() as u64 * 4;
    primary + image.frames.iter().map(|frame| frame.pixels.len() as u64 * 4).sum::<u64>()
}

fn benchmark_decode(path: &Path, bytes: &[u8], dimensions: (u32, u32)) {
    let (width, height) = dimensions;
    let pixels = (width as u64).saturating_mul(height as u64);
    if pixels == 0 {
        return;
    }

    let mut options = DecodeOptions::default();
    options.animation = AnimationMode::AllFrames;

    let Ok(warm) = Image::from_bytes_with_options(bytes, options) else {
        return;
    };
    black_box(warm);

    let iterations = calibrated_iterations(
        || {
            black_box(Image::from_bytes_with_options(black_box(bytes), options).unwrap());
        },
        128,
    );

    let alloc_start = allocation_snapshot();
    let baseline = begin_peak_tracking();
    let rss_before = peak_rss_kib();
    let start = Instant::now();
    let mut total_output_bytes = 0u64;
    for _ in 0..iterations {
        let image = Image::from_bytes_with_options(black_box(bytes), options).unwrap();
        total_output_bytes = total_output_bytes.saturating_add(output_bytes(&image));
        black_box(image);
    }
    let elapsed = start.elapsed();
    let (peak_live, largest_allocation) = end_peak_tracking(baseline);
    let alloc = allocation_delta(alloc_start, allocation_snapshot());
    let rss_after = peak_rss_kib();

    let seconds = elapsed.as_secs_f64().max(f64::MIN_POSITIVE);
    let per_decode_ms = seconds * 1000.0 / iterations as f64;
    let mp_s = (pixels as f64 * iterations as f64 / 1_000_000.0) / seconds;
    let compressed_mb_s = (bytes.len() as f64 * iterations as f64 / 1_000_000.0) / seconds;
    let allocs_per_decode = alloc.count as f64 / iterations as f64;
    let allocated_per_decode = alloc.allocated as f64 / iterations as f64;
    let output_per_decode = total_output_bytes as f64 / iterations as f64;
    let temporary_per_decode = allocated_per_decode - output_per_decode;

    let peak_ratio = if output_per_decode > 0.0 { peak_live as f64 / output_per_decode } else { 0.0 };
    println!(
        "{:<30} {:>5}x{:<5} {:>8.3} ms {:>8.1} MP/s {:>8.1} MB/s {:>7.1} alloc {:>11.0} B alloc {:>11.0} B out {:>11.0} B temp {:>11} B peak {:>10} B maxalloc {:>5.2}x peak/out rss_hwm={}",
        path.file_name().unwrap().to_string_lossy(),
        width,
        height,
        per_decode_ms,
        mp_s,
        compressed_mb_s,
        allocs_per_decode,
        allocated_per_decode,
        output_per_decode,
        temporary_per_decode.max(0.0),
        peak_live,
        largest_allocation,
        peak_ratio,
        rss_after.or(rss_before).map(|value| format!("{value} KiB")).unwrap_or_else(|| "n/a".into()),
    );
}

fn benchmark_header_detection(bytes: &[u8]) {
    let iterations = calibrated_iterations(
        || {
            black_box(Image::format(black_box(bytes)).unwrap());
            black_box(Image::dimensions(black_box(bytes)).unwrap());
        },
        1 << 20,
    );
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(Image::format(black_box(bytes)).unwrap());
        black_box(Image::dimensions(black_box(bytes)).unwrap());
    }
    let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
    println!("format+dimensions: {ns:.1} ns/iteration ({iterations} iterations)");
}

fn benchmark_load(path: &Path) {
    let Ok(bytes) = fs::read(path) else { return; };
    if Image::from_bytes(&bytes).is_err() {
        return;
    }
    let path_text = path.to_string_lossy();
    let iterations = 4u64;

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(Image::load(black_box(&path_text)).unwrap());
    }
    let load_ms = start.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    let start = Instant::now();
    for _ in 0..iterations {
        let encoded = fs::read(path).unwrap();
        black_box(Image::from_bytes(&encoded).unwrap());
    }
    let whole_ms = start.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    let start = Instant::now();
    for _ in 0..iterations {
        let file = fs::File::open(path).unwrap();
        let mut reader = BufReader::with_capacity(64 * 1024, file);
        black_box(Image::from_reader(&mut reader).unwrap());
    }
    let buffered_ms = start.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    println!(
        "filesystem/read adapters {}: Image::load={load_ms:.3} ms fs::read+from_bytes={whole_ms:.3} ms BufReader+from_reader={buffered_ms:.3} ms",
        path.file_name().unwrap().to_string_lossy(),
    );
}

fn format_enabled(format: ImageFormat) -> bool {
    match format {
        ImageFormat::Png => cfg!(feature = "png"),
        ImageFormat::Jpeg => cfg!(feature = "jpeg"),
        ImageFormat::WebP => cfg!(feature = "webp"),
        ImageFormat::Gif => cfg!(feature = "gif"),
        ImageFormat::Bmp => cfg!(feature = "bmp"),
        ImageFormat::Fits => cfg!(feature = "fits"),
        ImageFormat::Tiff => cfg!(feature = "tiff"),
        ImageFormat::Qoi => cfg!(feature = "qoi"),
        ImageFormat::Ico => cfg!(feature = "ico"),
        ImageFormat::Avif => cfg!(feature = "avif"),
        ImageFormat::Pgm => cfg!(feature = "pgm"),
        ImageFormat::Tga => cfg!(feature = "tga"),
        ImageFormat::Wbmp => cfg!(feature = "wbmp"),
        ImageFormat::Svg => cfg!(feature = "svg"),
    }
}

fn benchmark_rich_decoder(name: &str, bytes: &[u8], format: ImageFormat) {
    if !format_enabled(format) {
        return;
    }
    let iterations = 4u64;
    let start = Instant::now();
    for _ in 0..iterations {
        match format {
            ImageFormat::Png => {
                #[cfg(feature = "png")]
                black_box(mataimage::Png::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Jpeg => {
                #[cfg(feature = "jpeg")]
                black_box(mataimage::Jpeg::new(black_box(bytes)).unwrap());
            }
            ImageFormat::WebP => {
                #[cfg(feature = "webp")]
                black_box(mataimage::WebP::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Gif => {
                #[cfg(feature = "gif")]
                black_box(mataimage::Gif::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Bmp => {
                #[cfg(feature = "bmp")]
                black_box(mataimage::Bmp::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Fits => {
                #[cfg(feature = "fits")]
                black_box(mataimage::Fits::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Tiff => {
                #[cfg(feature = "tiff")]
                black_box(mataimage::TiffImage::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Qoi => {
                #[cfg(feature = "qoi")]
                black_box(mataimage::Qoi::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Ico => {
                #[cfg(feature = "ico")]
                black_box(mataimage::Ico::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Avif => {
                #[cfg(feature = "avif")]
                black_box(mataimage::Avif::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Pgm => {
                #[cfg(feature = "pgm")]
                black_box(mataimage::Pgm::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Tga => {
                #[cfg(feature = "tga")]
                black_box(mataimage::Tga::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Wbmp => {
                #[cfg(feature = "wbmp")]
                black_box(mataimage::Wbmp::new(black_box(bytes)).unwrap());
            }
            ImageFormat::Svg => {
                #[cfg(feature = "svg")]
                black_box(mataimage::Svg::new(black_box(bytes)).unwrap());
            }
        }
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / iterations as f64;
    println!("rich decoder {name:<20}: {ms:.3} ms/construct");
}

fn benchmark_pixel_conversion() {
    let pixels: Vec<u32> = (0..1_048_576u32)
        .map(|value| {
            let r = (value & 0xff) as u8;
            let g = ((value >> 5) & 0xff) as u8;
            let b = ((value >> 11) & 0xff) as u8;
            u32::from_be_bytes([r, g, b, 255])
        })
        .collect();
    let mut output = vec![0u16; pixels.len()];
    let iterations = 16u64;
    let start = Instant::now();
    for _ in 0..iterations {
        for (destination, &pixel) in output.iter_mut().zip(&pixels) {
            *destination = pixel.rgba_to_u16_rgb();
        }
        black_box(&output);
    }
    let seconds = start.elapsed().as_secs_f64().max(f64::MIN_POSITIVE);
    let mp_s = pixels.len() as f64 * iterations as f64 / 1_000_000.0 / seconds;
    println!("pixel conversion RGBA8888->RGB565: {mp_s:.1} MP/s");
}

fn benchmark_rescale_case(image: &Image, width: u32, height: u32, label: &str) {
    let mut destination = Image { width: 0, height: 0, pixels: Vec::new(), frames: Vec::new() };
    let iterations = 6u64;
    image.rescale_into(&mut destination, width, height);
    let start = Instant::now();
    for _ in 0..iterations {
        image.rescale_into(&mut destination, width, height);
        black_box(&destination);
    }
    let seconds = start.elapsed().as_secs_f64().max(f64::MIN_POSITIVE);
    let pixels = width as f64 * height as f64 * iterations as f64;
    println!("rescale {label:<6} {:>5}x{:<5} -> {:>5}x{:<5}: {:>8.1} MP/s",
        image.width, image.height, width, height, pixels / 1_000_000.0 / seconds);
}

fn benchmark_rescale_suite() {
    let bytes = read_asset("4k_rgba.png");
    let Ok(image) = Image::from_bytes(&bytes) else { return; };
    let cases = [
        (image.width / 4, image.height / 4, "0.25x"),
        (image.width / 2, image.height / 2, "0.5x"),
        (image.width * 3 / 4, image.height * 3 / 4, "0.75x"),
        (image.width * 3 / 2, image.height * 3 / 2, "1.5x"),
        (image.width * 2, image.height * 2, "2x"),
        (image.width * 4, image.height * 4, "4x"),
    ];
    for (width, height, label) in cases {
        benchmark_rescale_case(&image, width, height, label);
    }

    for name in ["wide_rgb.png", "tall_rgb.png"] {
        let bytes = read_asset(name);
        if let Ok(image) = Image::from_bytes(&bytes) {
            benchmark_rescale_case(&image, image.width / 2, image.height / 2, name);
        }
    }
}

fn benchmark_thread_scaling() {
    let bytes = read_asset("jpeg_4k_420.jpg");
    println!("JPEG thread scaling:");
    for threads in [1usize, 2, 4, 8] {
        let mut options = DecodeOptions::default();
        options.max_threads = Some(threads);
        if Image::from_bytes_with_options(&bytes, options).is_err() {
            return;
        }
        let iterations = 3u64;
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(Image::from_bytes_with_options(&bytes, options).unwrap());
        }
        println!("  max_threads={threads}: {:.3} ms/decode",
            start.elapsed().as_secs_f64() * 1000.0 / iterations as f64);
    }
}

fn benchmark_thread_creation_overhead() {
    println!("scoped thread creation overhead:");
    for workers in [1usize, 2, 4, 8] {
        let rounds = 64u64;
        let start = Instant::now();
        for _ in 0..rounds {
            std::thread::scope(|scope| {
                for worker in 0..workers {
                    scope.spawn(move || black_box(worker));
                }
            });
        }
        let us = start.elapsed().as_secs_f64() * 1_000_000.0 / rounds as f64;
        println!("  {workers} worker(s): {us:.2} us/scope");
    }
}

fn benchmark_named_stage(label: &str, asset_name: &str, iterations: u64) {
    let bytes = read_asset(asset_name);
    if Image::from_bytes(&bytes).is_err() {
        return;
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(Image::from_bytes(black_box(&bytes)).unwrap());
    }
    println!("stage proxy {label:<28}: {:.3} ms/decode ({asset_name})",
        start.elapsed().as_secs_f64() * 1000.0 / iterations as f64);
}

fn benchmark_stage_scenarios() {
    // These inputs are selected so the named stage is a large fraction of the
    // end-to-end workload. Hardware profilers can then attribute the exact hot
    // functions without making private codec internals part of the public API.
    benchmark_named_stage("PNG inflate", "png_noise.png", 8);
    benchmark_named_stage("PNG unfilter", "png_filter_4.png", 16);
    benchmark_named_stage("JPEG entropy", "jpeg_noise_420.jpg", 6);
    benchmark_named_stage("JPEG IDCT", "jpeg_444.jpg", 8);
    benchmark_named_stage("JPEG YCbCr conversion", "jpeg_4k_420.jpg", 3);
    benchmark_named_stage("WebP VP8L Huffman", "webp_vp8l.webp", 12);
    benchmark_named_stage("WebP VP8 inverse transform", "webp_vp8.webp", 12);
    benchmark_named_stage("GIF LZW", "gif_large.gif", 4);
    benchmark_named_stage("animation compositing", "webp_animated.webp", 4);
    benchmark_named_stage("BitReader/DEFLATE symbols", "png_many_idat.png", 32);
    benchmark_named_stage("Huffman construction", "tiny_rgba.png", 128);
}

fn benchmark_error_paths() {
    let directory = assets_dir().join("malformed");
    let Ok(entries) = fs::read_dir(directory) else { return; };
    let mut files: Vec<_> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
    files.sort();
    for path in files {
        let Ok(bytes) = fs::read(&path) else { continue; };
        let iterations = 512u64;
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(Image::from_bytes(black_box(&bytes)).err());
        }
        let us = start.elapsed().as_secs_f64() * 1_000_000.0 / iterations as f64;
        println!("malformed {:<32}: {us:.2} us/error",
            path.file_name().unwrap().to_string_lossy());
    }
}

fn benchmark_p2_header_metadata_and_scratch() {
    println!("P2 header/metadata/scratch:");
    let png = read_asset("png_many_idat.png");
    let iterations = calibrated_iterations(
        || { black_box(ImageInfo::from_bytes(black_box(&png)).unwrap()); },
        1 << 18,
    );
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(ImageInfo::from_bytes(black_box(&png)).unwrap());
    }
    println!("  ImageInfo: {:.1} ns/call", start.elapsed().as_nanos() as f64 / iterations as f64);

    let iterations = calibrated_iterations(
        || { black_box(ImageMetadata::read(black_box(&png), MetadataMode::All).unwrap()); },
        1 << 16,
    );
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(ImageMetadata::read(black_box(&png), MetadataMode::All).unwrap());
    }
    println!("  borrowed metadata index: {:.1} ns/call", start.elapsed().as_nanos() as f64 / iterations as f64);

    let mut scratch = DecodeScratch::new();
    let iterations = 12u64;
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(Image::from_bytes_with_scratch(&png, DecodeOptions::default(), &mut scratch).unwrap());
    }
    println!("  PNG reused scratch: {:.3} ms/decode, retained={} B",
        start.elapsed().as_secs_f64() * 1000.0 / iterations as f64,
        scratch.retained_bytes());
}

fn benchmark_p2_streaming_animation(asset_name: &str) {
    let bytes = read_asset(asset_name);
    let Ok(mut decoder) = AnimationDecoder::from_bytes(&bytes) else { return; };
    let mut output = vec![0u32; decoder.required_pixels()];
    let baseline = begin_peak_tracking();
    let start = Instant::now();
    let mut frames = 0u64;
    while decoder.next_frame(&mut output).unwrap().is_some() {
        frames += 1;
        black_box(&output);
    }
    let elapsed = start.elapsed();
    let (peak, largest_allocation) = end_peak_tracking(baseline);
    println!("  streaming {asset_name:<22}: frames={frames:<4} {:.3} ms total, peak-live={} B, max-allocation={} B",
        elapsed.as_secs_f64() * 1000.0, peak, largest_allocation);

    let mut options = DecodeOptions::default();
    options.animation = AnimationMode::AllFrames;
    let baseline = begin_peak_tracking();
    let start = Instant::now();
    let image = Image::from_bytes_with_options(&bytes, options).unwrap();
    let (materialized_peak, materialized_largest) = end_peak_tracking(baseline);
    println!("  materialized {asset_name:<19}: frames={:<4} {:.3} ms total, peak-live={} B, max-allocation={} B",
        image.frame_count(), start.elapsed().as_secs_f64() * 1000.0, materialized_peak, materialized_largest);
    black_box(image);
}

fn print_storage_overhead() {
    println!("storage sizes:");
    println!("  Image       = {} bytes", core::mem::size_of::<Image>());
    println!("  ImageFrame  = {} bytes", core::mem::size_of::<mataimage::ImageFrame>());
    println!("  Vec<u32>    = {} bytes", core::mem::size_of::<Vec<u32>>());
    println!("  Box<[u32]>  = {} bytes", core::mem::size_of::<Box<[u32]>>());
    println!("  still images in the current representation do not allocate the additional-frame Vec backing store");
}

#[allow(dead_code)]
enum StorageProbe {
    Still(Vec<u32>),
    Animated(Vec<Vec<u32>>),
}

fn benchmark_storage_layouts() {
    const ITERATIONS: usize = 20_000;
    const PIXELS: usize = 64;

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(Image {
            width: 8,
            height: 8,
            pixels: vec![0u32; PIXELS],
            frames: Vec::new(),
        });
    }
    let current = start.elapsed().as_secs_f64();

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(vec![vec![0u32; PIXELS]]);
    }
    let always_nested = start.elapsed().as_secs_f64();

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(StorageProbe::Still(vec![0u32; PIXELS]));
    }
    let storage_enum = start.elapsed().as_secs_f64();

    println!(
        "storage layout construction ({ITERATIONS} still images): current={:.3} ms always-Vec<Vec<u32>>={:.3} ms enum={:.3} ms",
        current * 1000.0,
        always_nested * 1000.0,
        storage_enum * 1000.0,
    );
    println!(
        "storage object sizes: current={} B nested-vec={} B enum={} B",
        core::mem::size_of::<Image>(),
        core::mem::size_of::<Vec<Vec<u32>>>(),
        core::mem::size_of::<StorageProbe>(),
    );
}

fn reference_qoi_rgba(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() >= 14 && &bytes[..4] == b"qoif");
    let width = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let height = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let count = width.checked_mul(height).unwrap();
    let mut out = Vec::with_capacity(count * 4);
    let mut index = [[0u8; 4]; 64];
    let mut pixel = [0u8, 0, 0, 255];
    let mut pos = 14usize;
    let mut run = 0usize;
    for _ in 0..count {
        if run != 0 {
            run -= 1;
        } else {
            let tag = bytes[pos];
            pos += 1;
            match tag {
                0xfe => {
                    pixel[0] = bytes[pos]; pixel[1] = bytes[pos + 1]; pixel[2] = bytes[pos + 2];
                    pos += 3;
                }
                0xff => {
                    pixel.copy_from_slice(&bytes[pos..pos + 4]);
                    pos += 4;
                }
                _ if tag & 0xc0 == 0x00 => pixel = index[(tag & 0x3f) as usize],
                _ if tag & 0xc0 == 0x40 => {
                    pixel[0] = pixel[0].wrapping_add(((tag >> 4) & 3).wrapping_sub(2));
                    pixel[1] = pixel[1].wrapping_add(((tag >> 2) & 3).wrapping_sub(2));
                    pixel[2] = pixel[2].wrapping_add((tag & 3).wrapping_sub(2));
                }
                _ if tag & 0xc0 == 0x80 => {
                    let extra = bytes[pos]; pos += 1;
                    let dg = (tag & 0x3f).wrapping_sub(32);
                    pixel[0] = pixel[0].wrapping_add(dg).wrapping_add((extra >> 4).wrapping_sub(8));
                    pixel[1] = pixel[1].wrapping_add(dg);
                    pixel[2] = pixel[2].wrapping_add(dg).wrapping_add((extra & 0x0f).wrapping_sub(8));
                }
                _ => run = (tag & 0x3f) as usize,
            }
            let hash = (pixel[0] as usize * 3
                + pixel[1] as usize * 5
                + pixel[2] as usize * 7
                + pixel[3] as usize * 11) & 63;
            index[hash] = pixel;
        }
        out.extend_from_slice(&pixel);
    }
    out
}

fn benchmark_reference_libraries() {
    println!("reference-library comparisons (same process allocator accounting):");
    for name in [
        "tiny_rgba.png", "4k_rgba.png", "jpeg_4k_420.jpg",
        "webp_vp8.webp", "gif_small.gif", "qoi_rgba.qoi",
    ] {
        let bytes = read_asset(name);
        let iterations = if name.starts_with("tiny_") { 64 } else { 4 };

        let baseline = begin_peak_tracking();
        let start = Instant::now();
        let mut reference_output = 0u64;
        for _ in 0..iterations {
            if name.ends_with(".qoi") {
                let rgba = reference_qoi_rgba(black_box(&bytes));
                reference_output = reference_output.saturating_add(rgba.len() as u64);
                black_box(rgba);
            } else {
                let image = image::load_from_memory(black_box(&bytes)).unwrap().to_rgba8();
                reference_output = reference_output.saturating_add(image.as_raw().len() as u64);
                black_box(image);
            }
        }
        let reference_elapsed = start.elapsed();
        let (reference_peak, reference_max) = end_peak_tracking(baseline);

        let baseline = begin_peak_tracking();
        let start = Instant::now();
        let mut mata_output = 0u64;
        for _ in 0..iterations {
            let image = Image::from_bytes(black_box(&bytes)).unwrap();
            mata_output = mata_output.saturating_add(output_bytes(&image));
            black_box(image);
        }
        let mata_elapsed = start.elapsed();
        let (mata_peak, mata_max) = end_peak_tracking(baseline);

        println!(
            "  {name:<22} mata={:>8.3} ms image={:>8.3} ms  mata_peak={:>10} B image_peak={:>10} B  mata_max={:>9} B image_max={:>9} B  output={} B",
            mata_elapsed.as_secs_f64() * 1000.0 / iterations as f64,
            reference_elapsed.as_secs_f64() * 1000.0 / iterations as f64,
            mata_peak, reference_peak, mata_max, reference_max,
            (mata_output.max(reference_output) / iterations as u64),
        );
    }
    println!("  tiny_rgba.png is reported separately so startup/small-image latency is not hidden by large-image throughput.");
    println!("  qoi_rgba.qoi uses a small local QOI-spec reference decoder instead of another full image framework.");
}

fn main() {
    print_environment();
    print_storage_overhead();
    benchmark_storage_layouts();

    let header_bytes = read_asset("tiny_rgba.png");
    benchmark_header_detection(&header_bytes);
    println!();

    println!("end-to-end decode:");
    println!("file                           dimensions      latency      pixels      input      allocations       allocated      output        temporary       peak-live");

    let filter = std::env::var("MATAIMAGE_BENCH_FILTER").ok();
    let mut files: Vec<_> = fs::read_dir(assets_dir())
        .expect("benchmark asset directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            !matches!(name, "README.md" | "MANIFEST.tsv")
        })
        .filter(|path| filter.as_ref().is_none_or(|needle| path.to_string_lossy().contains(needle)))
        .collect();
    files.sort();

    for path in &files {
        let Ok(bytes) = fs::read(path) else { continue; };
        let Ok(info) = Image::dimensions(&bytes) else { continue; };
        benchmark_decode(path, &bytes, info);
    }

    println!();
    benchmark_load(&asset("1080p_rgba.png"));

    println!("\nrich format constructors:");
    for name in [
        "png_rgba.png",
        "jpeg_420.jpg",
        "webp_vp8.webp",
        "gif_small.gif",
        "bmp_rgb.bmp",
        "fits_u8.fits",
        "ico_many.ico",
        "qoi_rgba.qoi",
        "pgm_p5.pgm",
        "tga_rgb.tga",
        "wbmp.wbmp",
        "tiff_rgb.tiff",
        "svg_simple.svg",
    ] {
        let bytes = read_asset(name);
        if let Ok(format) = Image::format(&bytes) {
            benchmark_rich_decoder(name, &bytes, format);
        }
    }

    println!("\npixel conversion:");
    benchmark_pixel_conversion();

    println!("\nrescale suite:");
    benchmark_rescale_suite();

    println!("\nthread scaling:");
    benchmark_thread_scaling();
    benchmark_thread_creation_overhead();

    println!("\nstage-focused end-to-end proxies:");
    benchmark_stage_scenarios();

    println!("\nP2 low-memory APIs:");
    benchmark_p2_header_metadata_and_scratch();
    benchmark_p2_streaming_animation("gif_120_frames.gif");
    benchmark_p2_streaming_animation("webp_120_frames.webp");

    println!("\nreference libraries:");
    benchmark_reference_libraries();

    println!("\nmalformed/error paths:");
    benchmark_error_paths();

    println!("\nFor cycles/pixel, instructions/pixel, branch misses, L1/LLC misses, SIMD/scalar, debug/release, and thread sweeps use scripts/profile-bench.sh and scripts/compare-bench.sh.");
}
