use mataimage::{Image, ImageError};
use std::path::Path;


/// Convert a raw pixel value into 0xRRGGBB
fn pixel_to_rgb(pixel: u32) -> u32 {
    // If your internal Image stores RGBA as 0xRRGGBBAA, strip alpha:
    (pixel >> 8) & 0x00FFFFFF
}





/// Load an image file into a 2D bitmap buffer.
///
/// The image is loaded using the `image` crate and converted into
/// a `Vec<Vec<u32>>` where each pixel is stored in ARGB format.
///
/// # Panics
/// Panics if the image cannot be opened or decoded.
pub fn load_image_2d<P: AsRef<Path>>(path: P) -> Result<Vec<Vec<u32>>, ImageError> {
    let img = Image::load(path.as_ref().to_str().unwrap())?;

    let width = img.width as usize;
    let height = img.height as usize;

    let mut buffer = vec![vec![0u32; width]; height];

    for y in 0..height {
        for x in 0..width {
            let pixel = img.pixels[y * width + x];
            buffer[y][x] = pixel_to_rgb(pixel);
        }
    }

    Ok(buffer)
}
/// Loads a sprite sheet image and slices it into individual sprites.
///
/// The image is divided into a grid of equally sized sprites, where each sprite
/// has a fixed width and height in pixels. Sprites are extracted left-to-right,
/// top-to-bottom, and returned as a flat vector.
///
/// Each sprite is represented as a 2D pixel buffer (`Vec<Vec<u32>>`) in
/// row-major order. Pixel values are encoded as `0xRRGGBB`.
///
/// # Parameters
/// - `path`: Path to the sprite sheet image file.
/// - `sprite_width`: Width of a single sprite in pixels.
/// - `sprite_height`: Height of a single sprite in pixels.
///
/// # Returns
/// - `Ok(Vec<Vec<Vec<u32>>>)` containing all extracted sprites.
///   - Outer `Vec`: list of sprites
///   - Middle `Vec`: rows of a sprite (Y axis)
///   - Inner `Vec`: pixels in a row (X axis)
///
/// # Errors
/// Returns an `image::ImageError` if the image cannot be loaded or decoded.
///
/// # Notes
/// - If the image dimensions are not evenly divisible by `sprite_width` or
///   `sprite_height`, any leftover pixels on the right or bottom edges are ignored.
/// - Alpha channels are discarded; only RGB data is used.
///
/// # Example
/// ```no_run
/// let sprites = load_sprite_sheet("sprites.png", 32, 64)?;
/// let first_sprite = &sprites[0];
/// ```

/// Load a sprite sheet and slice into individual sprites (pixels in 0xRRGGBB format).
pub fn load_sprite_sheet<P: AsRef<Path>>(
    path: P,
    sprite_width: u32,
    sprite_height: u32,
) -> Result<Vec<Vec<Vec<u32>>>, ImageError> {
    let img = Image::load(path.as_ref().to_str().unwrap())?;

    let sheet_width = img.width;
    let sheet_height = img.height;

    let sprites_x = sheet_width / sprite_width;
    let sprites_y = sheet_height / sprite_height;

    let mut sprites = Vec::new();

    for sy in 0..sprites_y {
        for sx in 0..sprites_x {
            let mut sprite =
                vec![vec![0u32; sprite_width as usize]; sprite_height as usize];

            for y in 0..sprite_height {
                for x in 0..sprite_width {
                    let px = sx * sprite_width + x;
                    let py = sy * sprite_height + y;

                    let idx = (py * sheet_width + px) as usize;
                    let pixel = img.pixels[idx];

                    sprite[y as usize][x as usize] = pixel_to_rgb(pixel);
                }
            }

            sprites.push(sprite);
        }
    }

    Ok(sprites)
}