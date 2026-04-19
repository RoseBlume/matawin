use crate::ImageError;
use flate2::read::ZlibDecoder;
use std::io::Read;

pub struct Png {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Png {
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        if &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
            return Err(ImageError::new("Invalid PNG"));
        }

        let mut i = 8;

        let mut width = 0;
        let mut height = 0;
        let mut bit_depth = 0;
        let mut color_type = 0;
        let mut interlace = 0;

        let mut idat = Vec::new();
        let mut palette: Vec<[u8; 4]> = Vec::new();

        let mut trns: Option<Vec<u8>> = None;
        let mut trns_gray: Option<u16> = None;

        while i < bytes.len() {
            let len = u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
            let typ = &bytes[i + 4..i + 8];
            let data = &bytes[i + 8..i + 8 + len];

            match typ {
                b"IHDR" => {
                    width = u32::from_be_bytes(data[0..4].try_into().unwrap());
                    height = u32::from_be_bytes(data[4..8].try_into().unwrap());
                    bit_depth = data[8];
                    color_type = data[9];
                    interlace = data[12];
                }

                b"PLTE" => {
                    palette.clear();
                    for (i, c) in data.chunks_exact(3).enumerate() {
                        let alpha = trns
                            .as_ref()
                            .and_then(|t| t.get(i))
                            .copied()
                            .unwrap_or(255);

                        palette.push([c[0], c[1], c[2], alpha]);
                    }
                }

                b"tRNS" => {
                    match color_type {
                        0 => {
                            trns_gray = Some(u16::from_be_bytes([data[0], data[1]]));
                        }
                        3 => {
                            trns = Some(data.to_vec());
                        }
                        _ => {}
                    }
                }

                b"IDAT" => idat.extend_from_slice(data),
                b"IEND" => break,
                _ => {}
            }

            i += len + 12;
        }

        // Apply tRNS if it came after PLTE
        if let Some(trns_data) = &trns {
            for (i, alpha) in trns_data.iter().enumerate() {
                if let Some(entry) = palette.get_mut(i) {
                    entry[3] = *alpha;
                }
            }
        }

        let mut z = ZlibDecoder::new(&idat[..]);
        let mut raw = Vec::new();
        z.read_to_end(&mut raw).unwrap();

        let mut rgba = vec![0u8; (width * height * 4) as usize];

        if interlace == 0 {
            Self::decode_pass(
                &raw,
                width,
                height,
                width as usize,
                height as usize,
                bit_depth,
                color_type,
                &palette,
                trns_gray,
                &mut rgba,
                0,
                0,
                1,
                1,
            )?;
        } else {
            let passes = [
                (0, 0, 8, 8),
                (4, 0, 8, 8),
                (0, 4, 4, 8),
                (2, 0, 4, 4),
                (0, 2, 2, 4),
                (1, 0, 2, 2),
                (0, 1, 1, 2),
            ];

            let mut offset = 0;

            for (sx, sy, dx, dy) in passes {
                let pw = ((width + dx - sx - 1) / dx) as usize;
                let ph = ((height + dy - sy - 1) / dy) as usize;

                if pw == 0 || ph == 0 {
                    continue;
                }

                let consumed = Self::decode_pass(
                    &raw[offset..],
                    width,
                    height,
                    pw,
                    ph,
                    bit_depth,
                    color_type,
                    &palette,
                    trns_gray,
                    &mut rgba,
                    sx as usize,
                    sy as usize,
                    dx as usize,
                    dy as usize,
                )?;

                offset += consumed;
            }
        }

        Ok(Self { width, height, data: rgba })
    }

    fn decode_pass(
        raw: &[u8],
        width: u32,
        height: u32,
        pw: usize,
        ph: usize,
        bit_depth: u8,
        color_type: u8,
        palette: &[[u8; 4]],
        trns_gray: Option<u16>,
        out: &mut [u8],
        sx: usize,
        sy: usize,
        dx: usize,
        dy: usize,
    ) -> Result<usize, ImageError> {
        let bpp = bytes_per_pixel(color_type, bit_depth);
        let bits_pp = bits_per_pixel(color_type, bit_depth);
        let stride = (pw * bits_pp + 7) / 8;

        let mut recon = vec![0u8; ph * stride];

        let mut src = 0;
        let mut dst = 0;

        for y in 0..ph {
            let filter = raw[src];
            src += 1;

            let row = &raw[src..src + stride];
            src += stride;

            let (before, current_and_after) = recon.split_at_mut(dst);
            let (out_row, _) = current_and_after.split_at_mut(stride);

            apply_filter(filter, row, out_row, y, stride, bpp, before);

            dst += stride;
        }

        for y in 0..ph {
            let row = &recon[y * stride..(y + 1) * stride];
            let unpacked = unpack_bits(row, bit_depth, pw as u32);

            for x in 0..pw {
                let fx = sx + x * dx;
                let fy = sy + y * dy;

                if fx >= width as usize || fy >= height as usize {
                    continue;
                }

                let dst_i = (fy * width as usize + fx) * 4;

                write_pixel(
                    color_type,
                    bit_depth,
                    &unpacked,
                    x,
                    palette,
                    trns_gray,
                    out,
                    dst_i,
                )?;
            }
        }

        Ok(src)
    }
}

fn write_pixel(
    ct: u8,
    bd: u8,
    data: &[u8],
    x: usize,
    palette: &[[u8; 4]],
    trns_gray: Option<u16>,
    out: &mut [u8],
    dst: usize,
) -> Result<(), ImageError> {
    match ct {
        // Grayscale
        0 => {
            let raw = data[x] as u32;
            let g = scale(raw, bd);

            let alpha = if let Some(t) = trns_gray {
                if raw as u16 == t { 0 } else { 255 }
            } else {
                255
            };

            out[dst..dst + 4].copy_from_slice(&[g, g, g, alpha]);
        }

        // RGB
        2 => {
            let i = x * 3;
            let r = scale(data[i] as u32, bd);
            let g = scale(data[i + 1] as u32, bd);
            let b = scale(data[i + 2] as u32, bd);

            out[dst..dst + 4].copy_from_slice(&[r, g, b, 255]);
        }

        // Indexed
        3 => {
            let c = palette[data[x] as usize];
            out[dst..dst + 4].copy_from_slice(&c);
        }

        // Grayscale + Alpha
        4 => {
            let i = x * 2;
            let g = scale(data[i] as u32, bd);
            let a = scale(data[i + 1] as u32, bd);

            out[dst..dst + 4].copy_from_slice(&[g, g, g, a]);
        }

        // RGBA
        6 => {
            let i = x * 4;
            let r = scale(data[i] as u32, bd);
            let g = scale(data[i + 1] as u32, bd);
            let b = scale(data[i + 2] as u32, bd);
            let a = scale(data[i + 3] as u32, bd);

            out[dst..dst + 4].copy_from_slice(&[r, g, b, a]);
        }

        _ => return Err(ImageError::new("Unsupported color type")),
    }

    Ok(())
}



fn apply_filter(
    filter: u8,
    row: &[u8],
    out: &mut [u8],
    y: usize,
    stride: usize,
    bpp: usize,
    prev_rows: &[u8],
) {
    match filter {
        0 => out.copy_from_slice(row),

        1 => {
            for i in 0..stride {
                let left = if i >= bpp { out[i - bpp] } else { 0 };
                out[i] = row[i].wrapping_add(left);
            }
        }

        2 => {
            for i in 0..stride {
                let up = if y > 0 {
                    prev_rows[prev_rows.len() - stride + i]
                } else {
                    0
                };
                out[i] = row[i].wrapping_add(up);
            }
        }

        3 => {
            for i in 0..stride {
                let left = if i >= bpp { out[i - bpp] } else { 0 };
                let up = if y > 0 {
                    prev_rows[prev_rows.len() - stride + i]
                } else {
                    0
                };
                out[i] = row[i].wrapping_add(((left as u16 + up as u16) / 2) as u8);
            }
        }

        4 => {
            for i in 0..stride {
                let a = if i >= bpp { out[i - bpp] } else { 0 };

                let b = if y > 0 {
                    prev_rows[prev_rows.len() - stride + i]
                } else {
                    0
                };

                let c = if y > 0 && i >= bpp {
                    prev_rows[prev_rows.len() - stride + i - bpp]
                } else {
                    0
                };

                out[i] = row[i].wrapping_add(paeth(a, b, c));
            }
        }

        _ => {}
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
}

fn bits_per_pixel(ct: u8, bd: u8) -> usize {
    match ct {
        0 => bd as usize,
        2 => 3 * bd as usize,
        3 => bd as usize,
        4 => 2 * bd as usize,
        6 => 4 * bd as usize,
        _ => 0,
    }
}

fn bytes_per_pixel(ct: u8, bd: u8) -> usize {
    ((bits_per_pixel(ct, bd) + 7) / 8).max(1)
}

fn unpack_bits(row: &[u8], bit_depth: u8, width: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(width as usize);

    match bit_depth {
        8 => return row.to_vec(),
        4 => for b in row {
            out.push(b >> 4);
            if out.len() < width as usize { out.push(b & 0xF); }
        },
        2 => for b in row {
            for s in (0..8).step_by(2).rev() {
                if out.len() >= width as usize { break; }
                out.push((b >> s) & 3);
            }
        },
        1 => for b in row {
            for s in (0..8).rev() {
                if out.len() >= width as usize { break; }
                out.push((b >> s) & 1);
            }
        },
        _ => {}
    }

    out
}

fn scale(v: u32, bd: u8) -> u8 {
    match bd {
        8 => v as u8,
        16 => (v >> 8) as u8,
        4 => (v * 17) as u8,
        2 => (v * 85) as u8,
        1 => (v * 255) as u8,
        _ => 0,
    }
}



impl super::Decoder for Png {
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