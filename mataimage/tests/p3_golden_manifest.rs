//! Validates the independently generated P3 pixel-hash manifest.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mataimage::Image;

fn fnv1a_rgba(pixels: &[u32]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &pixel in pixels {
        for byte in pixel.to_be_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn manifest() -> BTreeMap<String, (u32, u32, u64)> {
    let text = include_str!("../benches/assets/GOLDEN_PIXELS.tsv");
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut columns = line.split('\t');
            let name = columns.next().unwrap().to_owned();
            let width = columns.next().unwrap().parse().unwrap();
            let height = columns.next().unwrap().parse().unwrap();
            let hash = u64::from_str_radix(columns.next().unwrap(), 16).unwrap();
            (name, (width, height, hash))
        })
        .collect()
}

#[test]
fn every_valid_benchmark_file_has_a_golden_reference() {
    let manifest = manifest();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/assets");
    let mut valid_assets = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if !path.is_file() { continue; }
        let name = path.file_name().unwrap().to_string_lossy();
        if matches!(name.as_ref(), "README.md" | "MANIFEST.tsv" | "GOLDEN_PIXELS.tsv") { continue; }
        valid_assets.push(name.into_owned());
    }
    valid_assets.sort();
    assert_eq!(manifest.len(), valid_assets.len());
    for name in valid_assets {
        assert!(manifest.contains_key(&name), "missing golden reference for {name}");
    }
}

#[cfg(feature = "png")]
#[test]
fn lossless_png_goldens_match_reference_pixels() {
    let manifest = manifest();
    for name in [
        "tiny_rgba.png", "png_rgb.png", "png_rgba.png", "png_gray.png",
        "png_gray_alpha.png", "png_indexed_trns.png", "png_filter_0.png",
        "png_filter_1.png", "png_filter_2.png", "png_filter_3.png", "png_filter_4.png",
        "png_many_idat.png", "wide_rgb.png", "tall_rgb.png",
    ] {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/assets").join(name)
        ).unwrap();
        let image = Image::from_bytes(&bytes).unwrap();
        let &(width, height, expected) = manifest.get(name).unwrap();
        assert_eq!((image.width, image.height), (width, height), "{name}");
        assert_eq!(fnv1a_rgba(&image.pixels), expected, "{name}");
    }
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_golden_matches_reference_pixels() {
    let manifest = manifest();
    let name = "qoi_rgba.qoi";
    let bytes = include_bytes!("../benches/assets/qoi_rgba.qoi");
    let image = Image::from_bytes(bytes).unwrap();
    assert_eq!(fnv1a_rgba(&image.pixels), manifest.get(name).unwrap().2);
}
