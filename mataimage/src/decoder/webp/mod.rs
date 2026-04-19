use std::collections::HashMap;
use crate::ImageError;

/// Fully decoded WebP image
pub struct WebP {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA pixels
}

impl WebP {
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        // Check RIFF header
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
            return Err(ImageError::new("Invalid WebP file"));
        }

        // Search for VP8L chunk
        let mut i = 12;
        while i + 8 <= bytes.len() {
            let chunk_type = &bytes[i..i + 4];
            let chunk_size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
            let chunk_data = &bytes[i + 8..i + 8 + chunk_size];

            if chunk_type == b"VP8L" {
                return decode_vp8l(chunk_data);
            }

            i += 8 + chunk_size + (chunk_size % 2); // chunks are padded
        }

        Err(ImageError::new("No VP8L chunk found"))
    }
}

// ------------------------ BitReader ------------------------

struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn read_bits(&mut self, n: usize) -> Result<u32, ImageError> {
        if n == 0 || n > 32 {
            return Err(ImageError::new("Invalid bit count"));
        }

        let mut val = 0u32;
        for i in 0..n {
            let byte_idx = self.bit_pos / 8;
            if byte_idx >= self.data.len() {
                return Err(ImageError::new("BitReader overflow"));
            }
            let bit = (self.data[byte_idx] >> (self.bit_pos % 8)) & 1;
            val |= (bit as u32) << i;
            self.bit_pos += 1;
        }
        Ok(val)
    }

    fn byte_align(&mut self) {
        self.bit_pos = (self.bit_pos + 7) & !7;
    }
}

// ------------------------ Huffman ------------------------

struct Huffman {
    table: HashMap<u32, (u16, u8)>, // code -> (symbol, bit length)
}

impl Huffman {
    fn new(code_lengths: &[u8]) -> Self {
        let mut bl_count = [0u16; 16];
        for &len in code_lengths {
            if len > 0 { bl_count[len as usize] += 1; }
        }

        let mut next_code = [0u32; 16];
        let mut code = 0;
        for bits in 1..16 {
            code = (code + bl_count[bits - 1] as u32) << 1;
            next_code[bits] = code;
        }

        let mut table = HashMap::new();
        for (symbol, &len) in code_lengths.iter().enumerate() {
            if len != 0 {
                let c = next_code[len as usize];
                next_code[len as usize] += 1;
                table.insert(c, (symbol as u16, len));
            }
        }

        Self { table }
    }

    fn decode(&self, br: &mut BitReader) -> Result<u16, ImageError> {
        let mut code = 0;
        for len in 1..16 {
            code |= br.read_bits(1)?;
            if let Some(&(symbol, l)) = self.table.get(&code) {
                if l as usize == len { return Ok(symbol); }
            }
            code <<= 1;
        }
        Err(ImageError::new("Invalid Huffman code"))
    }
}

// ------------------------ VP8L decode ------------------------

fn decode_vp8l(data: &[u8]) -> Result<WebP, ImageError> {
    if data[0] != 0x2F {
        return Err(ImageError::new("Invalid VP8L signature"));
    }

    let bits = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    let width = (bits & 0x3FFF) + 1;
    let height = ((bits >> 14) & 0x3FFF) + 1;

    let mut br = BitReader::new(&data[5..]);
    let transform_flags = br.read_bits(3)? as u8;

    // ---------------- VP8L Huffman table parsing ----------------
    fn read_vp8l_huffman(br: &mut BitReader, num_symbols: usize) -> Result<Huffman, ImageError> {
        let mut code_lengths = vec![0u8; num_symbols];
        let mut i = 0;

        while i < num_symbols {
            let symbol = br.read_bits(4)? as u8;

            match symbol {
                0..=14 => {
                    code_lengths[i] = symbol;
                    i += 1;
                }
                15 => {
                    // Read repeat count
                    let extra = br.read_bits(4)? as usize;
                    let run_len = extra + 2;

                    // Repeat zeros or previous value
                    for _ in 0..run_len {
                        if i >= num_symbols {
                            break;
                        }
                        code_lengths[i] = 0;
                        i += 1;
                    }
                }
                _ => unreachable!(),
            }
        }

        Ok(Huffman::new(&code_lengths))
    }

    // VP8L standard: 288 literal/length symbols, 32 distance symbols
    let literal_len_tree = read_vp8l_huffman(&mut br, 288)?;
    let distance_tree = read_vp8l_huffman(&mut br, 32)?;

    // ---------------- LZ77 decoding ----------------
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    // Initialize color cache if present
    let mut color_cache: Option<Vec<[u8; 4]>> = None;
    if transform_flags & 0x4 != 0 {
        let cache_bits = (br.read_bits(4)? as usize) + 1;
        let cache_size = 1 << cache_bits;
        color_cache = Some(vec![[0u8; 4]; cache_size]);
    }

    while pixels.len() < (width * height * 4) as usize {
        let symbol = match literal_len_tree.decode(&mut br) {
            Ok(s) => s,
            Err(_) => break,
        };

        if symbol < 256 {
            pixels.push(symbol as u8);
        } else if symbol == 256 {
            // Color cache lookup
            if let Some(ref mut cache) = color_cache {
                let idx = br.read_bits(cache.len().trailing_zeros() as usize)? as usize;
                if idx >= cache.len() {
                    return Err(ImageError::new("Invalid color cache index"));
                }
                let color = cache[idx];
                pixels.extend_from_slice(&color);
            } else {
                return Err(ImageError::new("Color cache used but not initialized"));
            }
        } else {
            // LZ77 length-distance pair
            let length = (symbol - 257 + 3) as usize;
            let distance_symbol = match distance_tree.decode(&mut br) {
                Ok(d) => d,
                Err(_) => break,
            };
            let distance = (distance_symbol as usize) + 1;

            if distance > pixels.len() {
                return Err(ImageError::new("Invalid LZ77 distance"));
            }

            let start = pixels.len();
            for i in 0..length {
                let val = pixels[start - distance + i];
                pixels.push(val);
            }
        }

        // Update color cache after each pixel
        if let Some(ref mut cache) = color_cache {
            if pixels.len() >= 4 {
                let color = [
                    pixels[pixels.len() - 4],
                    pixels[pixels.len() - 3],
                    pixels[pixels.len() - 2],
                    pixels[pixels.len() - 1],
                ];
                let idx = (pixels.len() / 4 - 1) % cache.len();
                cache[idx] = color;
            }
        }
    }

    // Pad remaining pixels with zeros if necessary
    while pixels.len() < (width * height * 4) as usize {
        pixels.push(0);
    }

    // ---------------- Apply transforms ----------------
    apply_transforms(&mut pixels, width as usize, height as usize, transform_flags);

    Ok(WebP {
        width,
        height,
        data: pixels,
    })
}
// ------------------------ Transforms ------------------------

fn apply_transforms(pixels: &mut [u8], width: usize, height: usize, transform_flags: u8) {
    if transform_flags & 0x1 != 0 {
        // Predictor transform
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                if x > 0 && y > 0 {
                    let left_idx = (y * width + (x - 1)) * 4;
                    let top_idx = ((y - 1) * width + x) * 4;
                    let top_left_idx = ((y - 1) * width + (x - 1)) * 4;
                    for c in 0..4 {
                        let pred = pixels[left_idx + c]
                            .wrapping_add(pixels[top_idx + c])
                            .wrapping_sub(pixels[top_left_idx + c]);
                        pixels[idx + c] = pixels[idx + c].wrapping_add(pred);
                    }
                } else if x > 0 {
                    for c in 0..4 {
                        pixels[idx + c] = pixels[idx + c].wrapping_add(pixels[(y * width + (x - 1)) * 4 + c]);
                    }
                } else if y > 0 {
                    for c in 0..4 {
                        pixels[idx + c] = pixels[idx + c].wrapping_add(pixels[((y - 1) * width + x) * 4 + c]);
                    }
                }
            }
        }
    }

    if transform_flags & 0x2 != 0 {
        // Subtract-green transform
        for i in 0..(width * height) {
            let idx = i * 4;
            let g = pixels[idx + 1];
            pixels[idx] = pixels[idx].wrapping_add(g);
            pixels[idx + 2] = pixels[idx + 2].wrapping_add(g);
        }
    }

    // Color transform can be implemented here
}

impl super::Decoder for WebP {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn as_bytes(&self) -> Vec<u8> { self.data.clone() }
    fn pixels(&self) -> Vec<u32> {
        let data = self.as_bytes();
        // Convert RGBA bytes to u32 pixels
        data.chunks(4).filter_map(|chunk| {
            if chunk.len() == 4 {
                Some(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            } else {
                None
            }
        }).collect()
    }
}