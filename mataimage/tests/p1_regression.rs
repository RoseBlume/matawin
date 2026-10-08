use mataimage::{AnimationMode, DecodeLimits, DecodeOptions, Image, ImageFormat};

fn assert_decodes(bytes: &[u8], expected_format: ImageFormat) {
    assert_eq!(Image::format(bytes).unwrap(), expected_format);
    let (width, height) = Image::dimensions(bytes).unwrap();
    let image = Image::from_bytes(bytes).unwrap();
    assert_eq!((image.width, image.height), (width, height));
    assert_eq!(image.pixels.len(), width as usize * height as usize);
}

#[test]
fn lightweight_still_formats_decode_directly() {
    assert_decodes(include_bytes!("../benches/assets/png_rgba.png"), ImageFormat::Png);
    assert_decodes(include_bytes!("../benches/assets/jpeg_420.jpg"), ImageFormat::Jpeg);
    assert_decodes(include_bytes!("../benches/assets/bmp_rgb.bmp"), ImageFormat::Bmp);
    assert_decodes(include_bytes!("../benches/assets/qoi_rgba.qoi"), ImageFormat::Qoi);
    assert_decodes(include_bytes!("../benches/assets/pgm_p5.pgm"), ImageFormat::Pgm);
    assert_decodes(include_bytes!("../benches/assets/tga_rgb.tga"), ImageFormat::Tga);
    assert_decodes(include_bytes!("../benches/assets/wbmp.wbmp"), ImageFormat::Wbmp);
    assert_decodes(include_bytes!("../benches/assets/fits_u8.fits"), ImageFormat::Fits);
}

#[test]
fn first_frame_mode_does_not_retain_animation_frames() {
    for bytes in [
        include_bytes!("../benches/assets/gif_small.gif").as_slice(),
        include_bytes!("../benches/assets/webp_animated.webp").as_slice(),
    ] {
        let mut options = DecodeOptions::default();
        options.animation = AnimationMode::FirstFrame;
        let image = Image::from_bytes_with_options(bytes, options).unwrap();
        assert_eq!(image.frame_count(), 1);
        assert!(image.frames.is_empty());
    }
}

#[test]
fn ico_lightweight_path_selects_one_resolution() {
    let bytes = include_bytes!("../benches/assets/ico_many.ico");
    let mut options = DecodeOptions::default();
    options.preferred_icon_size = Some(64);
    let image = Image::from_bytes_with_options(bytes, options).unwrap();
    assert_eq!((image.width, image.height), (64, 64));
    assert_eq!(image.frame_count(), 1);
}

#[test]
fn output_dimensions_use_rescale_without_extra_frame_storage() {
    let bytes = include_bytes!("../benches/assets/png_rgba.png");
    let mut options = DecodeOptions::default();
    options.output_dimensions = Some((73, 41));
    let image = Image::from_bytes_with_options(bytes, options).unwrap();
    assert_eq!((image.width, image.height), (73, 41));
    assert_eq!(image.pixels.len(), 73 * 41);
}

#[test]
fn resource_limits_are_checked_before_output_allocation() {
    let bytes = include_bytes!("../benches/assets/4k_rgba.png");
    let mut options = DecodeOptions::default();
    options.limits = DecodeLimits {
        max_pixels: 1000,
        ..DecodeLimits::default()
    };
    assert!(Image::from_bytes_with_options(bytes, options).is_err());
}

#[test]
fn nearest_rescale_reuses_destination_capacity() {
    let source = Image::from_bytes(include_bytes!("../benches/assets/png_rgb.png")).unwrap();
    let mut destination = Image {
        width: 0,
        height: 0,
        pixels: Vec::with_capacity(512 * 512),
        frames: Vec::new(),
    };
    let capacity = destination.pixels.capacity();
    source.rescale_into(&mut destination, 256, 256);
    assert_eq!((destination.width, destination.height), (256, 256));
    assert_eq!(destination.pixels.len(), 256 * 256);
    assert_eq!(destination.pixels.capacity(), capacity);
}

#[test]
fn integer_downscale_uses_exact_nearest_samples() {
    let mut image = Image {
        width: 4,
        height: 4,
        pixels: (0u32..16).collect(),
        frames: Vec::new(),
    };
    let capacity = image.pixels.capacity();
    image.rescale(2, 2);
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(image.pixels, vec![0, 2, 8, 10]);
    assert!(image.pixels.capacity() <= capacity);
}

#[test]
fn non_integer_in_place_shrink_matches_reference_mapping() {
    let mut image = Image {
        width: 7,
        height: 5,
        pixels: (0u32..35).collect(),
        frames: Vec::new(),
    };
    image.rescale(4, 3);

    let mut expected = Vec::new();
    for y in 0..3usize {
        let source_y = y * 5 / 3;
        for x in 0..4usize {
            let source_x = x * 7 / 4;
            expected.push((source_y * 7 + source_x) as u32);
        }
    }
    assert_eq!(image.pixels, expected);
}

#[test]
fn still_image_uses_no_additional_frame_backing_allocation() {
    let image = Image::from_bytes(include_bytes!("../benches/assets/png_rgba.png")).unwrap();
    assert!(image.frames.is_empty());
    assert_eq!(image.frames.capacity(), 0);
    assert_eq!(image.frame_count(), 1);
}

#[test]
fn rational_rescale_mapping_does_not_drift_at_integer_boundaries() {
    let source = Image {
        width: 10,
        height: 7,
        pixels: (0u32..70).collect(),
        frames: Vec::new(),
    };
    let mut destination = Image {
        width: 0,
        height: 0,
        pixels: Vec::new(),
        frames: Vec::new(),
    };
    source.rescale_into(&mut destination, 6, 4);

    let mut expected = Vec::new();
    for y in 0..4usize {
        let source_y = y * 7 / 4;
        for x in 0..6usize {
            let source_x = x * 10 / 6;
            expected.push((source_y * 10 + source_x) as u32);
        }
    }
    assert_eq!(destination.pixels, expected);
}
