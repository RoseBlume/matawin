//! P3 regression coverage for streaming, scaled, and optimized decode paths.

use std::io::{self, Read};
use std::path::PathBuf;

use mataimage::{DecodeOptions, Image};

fn asset(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/assets").join(name)).unwrap()
}

struct ChunkedReader<'a> {
    data: &'a [u8],
    position: usize,
    state: u64,
    max_chunk: usize,
}

impl<'a> ChunkedReader<'a> {
    fn new(data: &'a [u8], seed: u64, max_chunk: usize) -> Self {
        Self { data, position: 0, state: seed, max_chunk: max_chunk.max(1) }
    }
}

impl Read for ChunkedReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.position == self.data.len() || out.is_empty() { return Ok(0); }
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let requested = 1 + (self.state as usize % self.max_chunk);
        let count = requested.min(out.len()).min(self.data.len() - self.position);
        out[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

#[cfg(feature = "png")]
#[test]
fn streaming_png_matches_slice_for_random_reader_boundaries() {
    for name in ["png_rgba.png", "png_many_idat.png", "png_filter_4.png", "png_noise.png"] {
        let bytes = asset(name);
        let expected = Image::from_bytes(&bytes).unwrap();
        for seed in 1..=32u64 {
            let mut reader = ChunkedReader::new(&bytes, seed.wrapping_mul(0x9E37_79B9), 257);
            let got = Image::from_reader(&mut reader).unwrap();
            assert_eq!((got.width, got.height), (expected.width, expected.height), "{name} seed={seed}");
            assert_eq!(got.pixels, expected.pixels, "{name} seed={seed}");
        }
    }
}

#[cfg(feature = "jpeg")]
#[test]
fn jpeg_native_scales_have_expected_dimensions() {
    use mataimage::{Jpeg, JpegScale};
    let bytes = asset("jpeg_420.jpg");
    let full = Jpeg::new(&bytes).unwrap();
    for (scale, divisor) in [
        (JpegScale::Half, 2u32),
        (JpegScale::Quarter, 4),
        (JpegScale::Eighth, 8),
    ] {
        let scaled = Jpeg::from_bytes_scaled(bytes.clone(), scale).unwrap();
        assert_eq!(scaled.width, full.width.div_ceil(divisor));
        assert_eq!(scaled.height, full.height.div_ceil(divisor));
        assert_eq!(scaled.pixels_ref().len(), scaled.width as usize * scaled.height as usize);
    }
}

#[cfg(feature = "jpeg")]
#[test]
fn single_and_multi_thread_options_are_pixel_identical() {
    let bytes = asset("jpeg_420.jpg");
    let mut one = DecodeOptions::default();
    one.max_threads = Some(1);
    let mut many = DecodeOptions::default();
    many.max_threads = Some(8);
    let a = Image::from_bytes_with_options(&bytes, one).unwrap();
    let b = Image::from_bytes_with_options(&bytes, many).unwrap();
    assert_eq!(a.pixels, b.pixels);
}

#[test]
fn rich_and_lightweight_first_frame_outputs_match() {
    #[cfg(feature = "png")]
    {
        let bytes = asset("png_rgba.png");
        let light = Image::from_bytes(&bytes).unwrap();
        let rich = mataimage::Png::new(&bytes).unwrap().into_pixels().unwrap();
        assert_eq!(light.pixels, rich);
    }
    #[cfg(feature = "jpeg")]
    {
        let bytes = asset("jpeg_420.jpg");
        let light = Image::from_bytes(&bytes).unwrap();
        let rich = mataimage::Jpeg::new(&bytes).unwrap();
        assert_eq!(light.pixels.as_slice(), rich.pixels_ref());
    }
    #[cfg(feature = "webp")]
    {
        let bytes = asset("webp_vp8l.webp");
        let light = Image::from_bytes(&bytes).unwrap();
        let rich = mataimage::WebP::new(&bytes).unwrap();
        assert_eq!(light.pixels.as_slice(), rich.pixels_ref());
    }
    #[cfg(feature = "gif")]
    {
        let bytes = asset("gif_small.gif");
        let light = Image::from_bytes(&bytes).unwrap();
        let rich = mataimage::Gif::new(&bytes).unwrap();
        assert_eq!(light.pixels.as_slice(), rich.pixels_ref());
    }
}

#[cfg(feature = "gif")]
fn repartition_first_gif_image_subblocks(input: &[u8], seed: u64) -> Vec<u8> {
    assert!(input.len() >= 13 && (&input[..6] == b"GIF87a" || &input[..6] == b"GIF89a"));
    let mut pos = 13usize;
    let packed = input[10];
    if packed & 0x80 != 0 { pos += 3 * (1usize << ((packed & 7) + 1)); }
    while pos < input.len() {
        match input[pos] {
            0x21 => {
                pos += 2;
                while pos < input.len() {
                    let n = input[pos] as usize; pos += 1;
                    if n == 0 { break; }
                    pos += n;
                }
            }
            0x2c => {
                let descriptor = pos;
                pos += 10;
                let image_packed = input[descriptor + 9];
                if image_packed & 0x80 != 0 { pos += 3 * (1usize << ((image_packed & 7) + 1)); }
                pos += 1; // LZW minimum code size
                let data_start = pos;
                let mut compressed = Vec::new();
                while pos < input.len() {
                    let n = input[pos] as usize; pos += 1;
                    if n == 0 { break; }
                    compressed.extend_from_slice(&input[pos..pos + n]);
                    pos += n;
                }
                let suffix = pos;
                let mut out = input[..data_start].to_vec();
                let mut state = seed;
                let mut consumed = 0usize;
                while consumed < compressed.len() {
                    state ^= state << 13; state ^= state >> 7; state ^= state << 17;
                    let n = (1 + state as usize % 255).min(compressed.len() - consumed);
                    out.push(n as u8);
                    out.extend_from_slice(&compressed[consumed..consumed + n]);
                    consumed += n;
                }
                out.push(0);
                out.extend_from_slice(&input[suffix..]);
                return out;
            }
            0x3b => break,
            other => panic!("unexpected GIF block 0x{other:02x}"),
        }
    }
    panic!("GIF has no image descriptor")
}

#[cfg(feature = "gif")]
#[test]
fn gif_lzw_is_independent_of_subblock_partitioning() {
    let source = asset("gif_small.gif");
    let expected = Image::from_bytes(&source).unwrap();
    for seed in 1..=32u64 {
        let repartitioned = repartition_first_gif_image_subblocks(&source, seed * 0x1f123bb5);
        let got = Image::from_bytes(&repartitioned).unwrap();
        assert_eq!(got.pixels, expected.pixels, "seed={seed}");
    }
}

#[test]
fn malformed_public_decode_never_panics() {
    let malformed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/assets/malformed");
    for entry in std::fs::read_dir(malformed).unwrap().map(Result::unwrap) {
        let bytes = std::fs::read(entry.path()).unwrap();
        assert!(std::panic::catch_unwind(|| Image::from_bytes(&bytes)).is_ok(),
            "public decode panicked for {}", entry.path().display());
    }

    let mut state = 0x5043_4d41_5441_5033u64;
    for len in 0..=1024usize {
        let mut bytes = vec![0u8; len];
        for byte in &mut bytes {
            state ^= state << 13; state ^= state >> 7; state ^= state << 17;
            *byte = state as u8;
        }
        let result = std::panic::catch_unwind(|| Image::from_bytes(&bytes));
        assert!(result.is_ok(), "public decode panicked for len={len}");
    }
}

#[cfg(feature = "ico")]
#[test]
fn dib_backed_ico_decodes_without_synthetic_bmp() {
    let bytes = asset("ico_dib_24.ico");
    let image = Image::from_bytes(&bytes).unwrap();
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(image.pixels, vec![
        0xff0000ff, 0x00ff00ff,
        0x0000ffff, 0xffffffff,
    ]);
}
