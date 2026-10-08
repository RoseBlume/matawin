use std::fs;
use std::path::{Path, PathBuf};

use mataimage::{DecodeOptions, Decoder, Image};

fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/assets").join(name)
}

#[cfg(feature = "png")]
#[test]
fn lightweight_png_contains_only_renderable_output() {
    let bytes = fs::read(asset("png_rgba.png")).unwrap();
    let image = Image::from_bytes(&bytes).unwrap();
    assert_eq!(image.pixels().len(), image.width as usize * image.height as usize);
    assert_eq!(image.frame_count(), 1);
}

#[cfg(feature = "gif")]
#[test]
fn generic_animation_frames_are_full_canvas() {
    let bytes = fs::read(asset("gif_small.gif")).unwrap();
    let image = Image::from_bytes(&bytes).unwrap();
    let expected = image.width as usize * image.height as usize;
    assert_eq!(image.pixels.len(), expected);
    assert!(image.frames.iter().all(|frame| frame.pixels.len() == expected));
}

#[cfg(feature = "ico")]
#[test]
fn ico_generic_decode_selects_one_resolution_but_icon_set_keeps_all() {
    let bytes = fs::read(asset("ico_many.ico")).unwrap();
    let image = Image::from_bytes(&bytes).unwrap();
    assert_eq!(image.frame_count(), 1, "ICO resolutions are not animation frames");

    let set = mataimage::IconSet::new(&bytes).unwrap();
    assert!(set.len() > 1);
    assert!(set.frames().iter().all(|frame| {
        frame.pixels.len() == frame.width as usize * frame.height as usize
    }));
    assert!(set.preferred_frame(None).is_some());
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_rich_data_is_decoded_rgba_not_encoded_payload() {
    let bytes = fs::read(asset("qoi_rgba.qoi")).unwrap();
    let rich = mataimage::Qoi::new(&bytes).unwrap();
    assert_eq!(rich.data.len(), rich.width as usize * rich.height as usize * 4);

    let image = Image::from_bytes(&bytes).unwrap();
    assert_eq!(image.pixels.len(), rich.width as usize * rich.height as usize);
    let first = image.pixels[0].to_be_bytes();
    assert_eq!(&rich.data[..4], &first);
}

#[cfg(feature = "qoi")]
#[test]
fn qoi_rejects_zero_dimensions() {
    let mut bytes = vec![0u8; 22];
    bytes[..4].copy_from_slice(b"qoif");
    bytes[8..12].copy_from_slice(&1u32.to_be_bytes());
    bytes[12] = 4;
    bytes[13] = 0;
    assert!(Image::from_bytes(&bytes).is_err());
}

#[cfg(feature = "tga")]
#[test]
fn tga_rejects_zero_dimensions() {
    let mut bytes = vec![0u8; 18];
    bytes[2] = 2;
    bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
    bytes[16] = 24;
    assert!(Image::from_bytes(&bytes).is_err());
}

#[cfg(feature = "wbmp")]
#[test]
fn wbmp_rejects_zero_dimensions() {
    let bytes = [0u8, 0, 0, 1];
    assert!(Image::from_bytes(&bytes).is_err());
}

#[cfg(feature = "png")]
#[test]
fn image_decode_limits_reject_pathological_canvas_requests() {
    let mut options = DecodeOptions::default();
    options.limits.max_pixels = 1;
    let bytes = fs::read(asset("tiny_rgba.png")).unwrap();
    assert!(Image::from_bytes_with_options(&bytes, options).is_err());
}


#[cfg(feature = "jpeg")]
#[test]
fn consuming_decoder_trait_transfers_jpeg_pixels() {
    let bytes = fs::read(asset("jpeg_420.jpg")).unwrap();
    let rich = mataimage::Jpeg::new(&bytes).unwrap();
    let width = rich.width;
    let height = rich.height;
    let image = Box::new(rich).into_image().unwrap();
    assert_eq!((image.width, image.height), (width, height));
    assert_eq!(image.pixels.len(), width as usize * height as usize);
}

#[cfg(feature = "gif")]
#[test]
fn consuming_decoder_trait_moves_animation_frames() {
    let bytes = fs::read(asset("gif_small.gif")).unwrap();
    let rich = mataimage::Gif::new(&bytes).unwrap();
    let frame_count = rich.frame_count();
    let image = Box::new(rich).into_image().unwrap();
    assert_eq!(image.frame_count(), frame_count);
}
