use std::io;
use std::time::{Instant};
use image::{ImageBuffer, Rgba};
// use std::fs::File;
// use mataimage::Decoder;
fn main () {
    // unsafe { backtrace_on_stack_overflow::enable() };
    let start = Instant::now();
    /*
    Image loaded in: 809µs
Image data: Png {
    header: PngHeader {
        width: 17,
        height: 12,
        bit_depth: 8,
        color_type: TruecolorWithAlpha,
        compression_method: 0,
        filter_method: 0,
        interlace_method: 0,
    },
    palette: None,
    srgb: None,
    icc_profile: None,
    text_chunks: [],
    physical: None,
    gamma: None,
    time: None,
    content_light_level: None,
    chromaticity: None,
    transparency: None,
    significant_bits: None,
     */
    let path = "test1.png";
    // let mut file = File::open(path).expect("Failed to open file");
    // let image = match mataimage::Png::parse(&mut file) {
    let image = match mataimage::Image::load(path) {
        Ok(image) => {
            let duration = start.elapsed();
            println!("Image loaded in: {:#?}", duration);
            // println!(
            //     "Image data:\nheader={:#?}\npalette={:#?}\nsrgb={:#?}\nicc_profile={:#?}\ntext_chunks={:#?}\nphysical={:#?}\ngamma={:#?}\ntime={:#?}\ncontent_light_level={:#?}\nchromaticity={:#?}\ntransparency={:#?}\nsignificant_bits={:#?}",
            //     image.header,
            //     image.palette,
            //     image.srgb,
            //     image.icc_profile,
            //     image.text_chunks,
            //     image.physical,
            //     image.gamma,
            //     image.time,
            //     image.content_light_level,
            //     image.chromaticity,
            //     image.transparency,
            //     image.significant_bits,
            // );
            image
        },
        Err(err) => {
            eprintln!("Failed to load image: {}", err);
            return;
        }
    };

    let pixels: Vec<u32> = image.frames[0].pixels.clone();
    let width = image.width;
    let height = image.height;

    if width == 0 || height == 0 || pixels.is_empty() {
        eprintln!(
            "Loaded image has no pixels: width={}, height={}, pixel_count={}",
            width,
            height,
            pixels.len()
        );
        return;
    }

    if pixels.len() != (width * height) as usize {
        eprintln!(
            "Loaded image pixel buffer length mismatch: width={}, height={}, pixel_count={}, expected={}",
            width,
            height,
            pixels.len(),
            width * height
        );
        return;
    }

    let outpath = "output.bmp";
    let mut pixel_rows: Vec<Vec<u32>> = Vec::with_capacity(height as usize);
    for y in 0..height {
        let start = (y * width) as usize;
        let end = start + width as usize;

        if end > pixels.len() {
            eprintln!(
                "Invalid image row slice: y={}, start={}, end={}, pixel_count={}",
                y,
                start,
                end,
                pixels.len()
            );
            return;
        }

        pixel_rows.push(pixels[start..end].to_vec());
    }

    if let Err(err) = pixels_to_bmp(&pixel_rows, outpath) {
        eprintln!("Failed to write BMP file: {}", err);
    }
    let duration = start.elapsed();
    println!("Time Elapsed: {:#?}", duration);
}

pub fn pixels_to_bmp(
    pixels: &[Vec<u32>],
    path: impl AsRef<std::path::Path>,
) -> io::Result<()> {
    if pixels.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot write an empty pixel buffer",
        ));
    }

    let height = pixels.len();
    let width = pixels[0].len();

    if width == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot write a pixel buffer with zero width",
        ));
    }

    // Make sure every row has the same width.
    for (y, row) in pixels.iter().enumerate() {
        if row.len() != width {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Row {} has width {}, expected {}",
                    y,
                    row.len(),
                    width
                ),
            ));
        }
    }

    let mut image = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(
        width as u32,
        height as u32,
    );

    for (y, row) in pixels.iter().enumerate() {
        for (x, pixel) in row.iter().enumerate() {
            let r = ((pixel >> 24) & 0xff) as u8;
            let g = ((pixel >> 16) & 0xff) as u8;
            let b = ((pixel >> 8) & 0xff) as u8;
            let a = (pixel & 0xff) as u8;

            image.put_pixel(
                x as u32,
                y as u32,
                Rgba([r, g, b, a]),
            );
        }
    }

    image
        .save(path)
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::Other,
                error,
            )
        })?;

    Ok(())
}