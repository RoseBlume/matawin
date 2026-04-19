use crate::ImageError;

pub struct Pgm {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
}

impl Pgm {
    
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut i = 0;

        // --- Tokenizer (handles comments + whitespace) ---
        
        let magic = next_token(bytes, &mut i)
            .ok_or_else(|| ImageError::new("Missing magic number"))?;

        let width: u32 = next_token(bytes, &mut i)
            .ok_or_else(|| ImageError::new("Missing width"))?
            .parse()
            .map_err(|_| ImageError::new("Invalid width"))?;

        let height: u32 = next_token(bytes, &mut i)
            .ok_or_else(|| ImageError::new("Missing height"))?
            .parse()
            .map_err(|_| ImageError::new("Invalid height"))?;

        let maxval: u32 = next_token(bytes, &mut i)
            .ok_or_else(|| ImageError::new("Missing maxval"))?
            .parse()
            .map_err(|_| ImageError::new("Invalid maxval"))?;

        if maxval == 0 || maxval > 65535 {
            return Err(ImageError::new("Unsupported maxval"));
        }

        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("Image size overflow"))?;

        let mut data = vec![0u8; pixel_count * 4];

        match magic.as_str() {
            "P2" => {
                // --- ASCII ---
                for idx in 0..pixel_count {
                    let val: u32 = next_token(bytes, &mut i)
                        .ok_or_else(|| ImageError::new("Unexpected EOF in ASCII data"))?
                        .parse()
                        .map_err(|_| ImageError::new("Invalid pixel value"))?;

                    let gray = Self::scale_to_u8(val, maxval);

                    let o = idx * 4;
                    data[o] = gray;
                    data[o + 1] = gray;
                    data[o + 2] = gray;
                    data[o + 3] = 255;
                }
            }

            "P5" => {
                // Skip single whitespace after header
                if i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }

                if maxval <= 255 {
                    // --- 8-bit binary ---
                    if bytes.len() < i + pixel_count {
                        return Err(ImageError::new("Not enough pixel data"));
                    }

                    let pixels = &bytes[i..i + pixel_count];

                    for (idx, &val) in pixels.iter().enumerate() {
                        let gray = Self::scale_to_u8(val as u32, maxval);

                        let o = idx * 4;
                        data[o] = gray;
                        data[o + 1] = gray;
                        data[o + 2] = gray;
                        data[o + 3] = 255;
                    }
                } else {
                    // --- 16-bit binary (big-endian per spec) ---
                    let needed = pixel_count * 2;

                    if bytes.len() < i + needed {
                        return Err(ImageError::new("Not enough 16-bit pixel data"));
                    }

                    for idx in 0..pixel_count {
                        let hi = bytes[i + idx * 2] as u16;
                        let lo = bytes[i + idx * 2 + 1] as u16;
                        let val = ((hi << 8) | lo) as u32;

                        let gray = Self::scale_to_u8(val, maxval);

                        let o = idx * 4;
                        data[o] = gray;
                        data[o + 1] = gray;
                        data[o + 2] = gray;
                        data[o + 3] = 255;
                    }
                }
            }

            _ => {
                return Err(ImageError::new("Unsupported PGM format (expected P2 or P5)"));
            }
        }

        Ok(Self {
            width,
            height,
            data,
        })
    }


    
    fn scale_to_u8(value: u32, maxval: u32) -> u8 {
        if maxval == 255 {
            value as u8
        } else {
            ((value * 255) / maxval) as u8
        }
    }
}

fn next_token(bytes: &[u8], i: &mut usize) -> Option<String> {
    while *i < bytes.len() {
        match bytes[*i] {
            b'#' => {
                while *i < bytes.len() && bytes[*i] != b'\n' {
                    *i += 1;
                }
            }
            b' ' | b'\n' | b'\r' | b'\t' => *i += 1,
            _ => break,
        }
    }

    if *i >= bytes.len() {
        return None;
    }

    let start = *i;

    while *i < bytes.len() {
        match bytes[*i] {
            b' ' | b'\n' | b'\r' | b'\t' => break,
            _ => *i += 1,
        }
    }

    Some(String::from_utf8_lossy(&bytes[start..*i]).to_string())
}


impl super::Decoder for Pgm {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn as_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }
    fn pixels(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity((self.width * self.height) as usize);
        for chunk in self.data.chunks(4) {
            if chunk.len() == 4 {
                let r = chunk[0] as u32;
                let g = chunk[1] as u32;
                let b = chunk[2] as u32;
                let a = chunk[3] as u32;
                out.push((r << 24) | (g << 16) | (b << 8) | a);
            }
        }
        out
    }
}