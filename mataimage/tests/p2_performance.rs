use std::fs;
use std::path::PathBuf;

use mataimage::{
    AnimationDecoder, AnimationMode, DecodeOptions, DecodeScratch, Image, ImageError,
    ImageErrorKind, ImageFormat, ImageInfo, MetadataMode,
};
use mataimage::metadata::ImageMetadata;

fn asset(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/assets").join(name);
    fs::read(path).expect("benchmark/test asset")
}

#[test]
fn p2_static_errors_keep_borrowed_messages() {
    let error = ImageError::unsupported("static unsupported error");
    assert_eq!(error.kind(), ImageErrorKind::Unsupported);
    assert!(error.has_borrowed_message());
}

#[cfg(feature = "png")]
#[test]
fn p2_metadata_is_explicit_and_borrowed() {
    let bytes = asset("png_indexed_trns.png");
    let none = ImageMetadata::read(&bytes, MetadataMode::None).unwrap();
    assert!(none.blocks.is_empty());

    let essential = ImageMetadata::read(&bytes, MetadataMode::Essential).unwrap();
    assert!(!essential.blocks.is_empty());
    for block in &essential.blocks {
        let start = bytes.as_ptr() as usize;
        let end = start + bytes.len();
        let ptr = block.data.as_ptr() as usize;
        assert!(ptr >= start && ptr <= end);
    }

    let image = Image::from_bytes(&bytes).unwrap();
    assert_eq!(image.width, ImageInfo::from_bytes(&bytes).unwrap().width);
}

#[cfg(feature = "png")]
#[test]
fn p2_png_cross_decode_scratch_matches_regular_decode() {
    let bytes = asset("png_many_idat.png");
    let expected = Image::from_bytes(&bytes).unwrap();
    let mut scratch = DecodeScratch::new();
    let first = Image::from_bytes_with_scratch(&bytes, DecodeOptions::default(), &mut scratch).unwrap();
    let retained_after_first = scratch.retained_bytes();
    let second = Image::from_bytes_with_scratch(&bytes, DecodeOptions::default(), &mut scratch).unwrap();
    assert_eq!(first, expected);
    assert_eq!(second, expected);
    assert!(scratch.retained_bytes() >= 32_768);
    assert!(retained_after_first >= 32_768);
    scratch.clear();
    scratch.shrink();
    assert_eq!(scratch.retained_bytes(), 32_768);
}

#[cfg(feature = "gif")]
#[test]
fn p2_default_gif_decode_is_first_frame_and_stream_matches_materialized() {
    let bytes = asset("gif_disposal_3.gif");
    let info = ImageInfo::from_bytes(&bytes).unwrap();
    assert_eq!(info.format, ImageFormat::Gif);
    assert!(info.frame_count.unwrap_or(1) >= 1);

    let default_image = Image::from_bytes(&bytes).unwrap();
    assert_eq!(default_image.frame_count(), 1);

    let materialized = Image::from_bytes_animated(&bytes).unwrap();
    let expected: Vec<&[u32]> = materialized.frame_iter().collect();
    let mut decoder = AnimationDecoder::from_bytes(&bytes).unwrap();
    let mut output = vec![0u32; decoder.required_pixels()];
    let mut index = 0usize;
    while decoder.next_frame(&mut output).unwrap().is_some() {
        assert_eq!(output.as_slice(), expected[index]);
        index += 1;
    }
    assert_eq!(index, expected.len());
}

#[cfg(feature = "webp")]
#[test]
fn p2_default_webp_decode_is_first_frame_and_stream_matches_materialized() {
    let bytes = asset("webp_animated.webp");
    let default_image = Image::from_bytes(&bytes).unwrap();
    assert_eq!(default_image.frame_count(), 1);

    let mut all = DecodeOptions::default();
    all.animation = AnimationMode::AllFrames;
    let materialized = Image::from_bytes_with_options(&bytes, all).unwrap();
    let expected: Vec<&[u32]> = materialized.frame_iter().collect();
    let mut decoder = AnimationDecoder::from_bytes(&bytes).unwrap();
    let mut output = vec![0u32; decoder.required_pixels()];
    let mut index = 0usize;
    while decoder.next_frame(&mut output).unwrap().is_some() {
        assert_eq!(output.as_slice(), expected[index]);
        index += 1;
    }
    assert_eq!(index, expected.len());
}

#[cfg(feature = "ico")]
#[test]
fn p2_ico_directory_is_lazy_and_reports_all_sizes() {
    let bytes = asset("ico_many.ico");
    let reader = mataimage::IcoReader::new(&bytes).unwrap();
    let info = ImageInfo::from_bytes(&bytes).unwrap();
    assert_eq!(reader.len(), info.icon_sizes.len());
    assert!(!reader.is_empty());
    let first = reader.decode(0).unwrap();
    assert!(!first.pixels.is_empty());
}

#[cfg(feature = "fits")]
#[test]
fn p2_fits_hdu_reader_decodes_only_selected_plane() {
    let bytes = asset("fits_f32.fits");
    let reader = mataimage::FitsReader::new(&bytes).unwrap();
    assert!(!reader.is_empty());
    let first = reader.hdu_info(0).unwrap();
    assert!(first.is_image);
    let image = reader.decode_plane(0, 0).unwrap();
    assert_eq!(Some(image.width), first.width);
    assert_eq!(Some(image.height), first.height);
}

#[cfg(feature = "tiff")]
#[test]
fn p2_tiff_pages_index_before_pixel_decode() {
    let bytes = asset("tiff_rgb.tiff");
    let pages = mataimage::TiffPages::new(&bytes).unwrap();
    assert!(!pages.is_empty());
    assert!(pages.ifd_offset(0).is_some());
    let page = pages.decode_page_pixels(0).unwrap();
    assert!(page.width.unwrap_or(0) > 0);
    assert!(page.height.unwrap_or(0) > 0);
}
