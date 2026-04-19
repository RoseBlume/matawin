pub struct Pixels {
    pub data: Vec<Pixel>,
    pub format: PixelFormat,
}

impl Pixels {
    pub fn new(data: Vec<Pixel>, format: PixelFormat) -> Self {
        Self { data, format }
    }

    pub fn from_bytes(bytes: Vec<u8>, format: PixelFormat) -> Self {
        let data = match format {
            PixelFormat::Rgb8 => bytes.chunks(3).map(|chunk| {
                let r = chunk[0] as u32;
                let g = chunk[1] as u32;
                let b = chunk[2] as u32;
                Pixel::Rgb8(r as u8, g as u8, b as u8)
            }).collect(),
            PixelFormat::Rgba8 => bytes.chunks(4).map(|chunk| {
                let r = chunk[0] as u32;
                let g = chunk[1] as u32;
                let b = chunk[2] as u32;
                let a = chunk[3] as u32;
                Pixel::Rgba8(r as u8, g as u8, b as u8, a as u8)
            }).collect(),
            // Add handling for other formats as needed
            PixelFormat::Rgb16 => bytes.chunks(6).map(|chunk| {
                let r = ((chunk[0] as u32) << 8) | chunk[1] as u32;
                let g = ((chunk[2] as u32) << 8) | chunk[3] as u32;
                let b = ((chunk[4] as u32) << 8) | chunk[5] as u32;
                Pixel::Rgb16(r as u16, g as u16, b as u16)
            }).collect(),
            PixelFormat::Rgba16 => bytes.chunks(8).map(|chunk| {
                let r = ((chunk[0] as u32) << 8) | chunk[1] as u32;
                let g = ((chunk[2] as u32) << 8) | chunk[3] as u32;
                let b = ((chunk[4] as u32) << 8) | chunk[5] as u32;
                let a = ((chunk[6] as u32) << 8) | chunk[7] as u32;
                Pixel::Rgba16(r as u16, g as u16, b as u16, a as u16)
            }).collect(),
            PixelFormat::Rgb32 => bytes.chunks(12).map(|chunk| {
                let r = ((chunk[0] as u32) << 24) | ((chunk[1] as u32) << 16) | ((chunk[2] as u32) << 8) | chunk[3] as u32;
                let g = ((chunk[4] as u32) << 24) | ((chunk[5] as u32) << 16) | ((chunk[6] as u32) << 8) | chunk[7] as u32;
                let b = ((chunk[8] as u32) << 24) | ((chunk[9] as u32) << 16) | ((chunk[10] as u32) << 8) | chunk[11] as u32;
                Pixel::Rgb32(r as u32, g as u32, b as u32)
            }).collect(),
            PixelFormat::Rgba32 => bytes.chunks(16).map(|chunk| {
                let r = ((chunk[0] as u32) << 24) | ((chunk[1] as u32) << 16) | ((chunk[2] as u32) << 8) | chunk[3] as u32;
                let g = ((chunk[4] as u32) << 24) | ((chunk[5] as u32) << 16) | ((chunk[6] as u32) << 8) | chunk[7] as u32;
                let b = ((chunk[8] as u32) << 24) | ((chunk[9] as u32) << 16) | ((chunk[10] as u32) << 8) | chunk[11] as u32;
                let a = ((chunk[12] as u32) << 24) | ((chunk[13] as u32) << 16) | ((chunk[14] as u32) << 8) | chunk[15] as u32;
                Pixel::Rgba32(r as u32, g as u32, b as u32, a as u32)
            }).collect(),
        };

        Self { data, format }
    }

}




pub enum PixelFormat {
    Rgb8,
    Rgba8,
    Rgb16,
    Rgba16,
    Rgb32,
    Rgba32,
}


pub enum Pixel {
    Rgb8(u8, u8, u8),
    Rgba8(u8, u8, u8, u8),
    Rgb16(u16, u16, u16),
    Rgba16(u16, u16, u16, u16),
    Rgb32(u32, u32, u32),
    Rgba32(u32, u32, u32, u32),
}

impl Pixel {
    // Convert any Pixel to Rgb8
    pub fn to_rgb8(&self) -> Self {
        match self {
            Pixel::Rgb8(r, g, b) => Pixel::Rgb8(*r, *g, *b),
            Pixel::Rgba8(r, g, b, _) => Pixel::Rgb8(*r, *g, *b),
            Pixel::Rgb16(r, g, b) => Pixel::Rgb8((*r >> 8) as u8, (*g >> 8) as u8, (*b >> 8) as u8),
            Pixel::Rgba16(r, g, b, _) => Pixel::Rgb8((*r >> 8) as u8, (*g >> 8) as u8, (*b >> 8) as u8),
            Pixel::Rgb32(r, g, b) => Pixel::Rgb8((*r >> 24) as u8, (*g >> 24) as u8, (*b >> 24) as u8),
            Pixel::Rgba32(r, g, b, _) => Pixel::Rgb8((*r >> 24) as u8, (*g >> 24) as u8, (*b >> 24) as u8),
        }
    }

    // Convert any Pixel to Rgba8
    pub fn to_rgba8(&self) -> Self {
        match self {
            Pixel::Rgb8(r, g, b) => Pixel::Rgba8(*r, *g, *b, 255),
            Pixel::Rgba8(r, g, b, a) => Pixel::Rgba8(*r, *g, *b, *a),
            Pixel::Rgb16(r, g, b) => Pixel::Rgba8((*r >> 8) as u8, (*g >> 8) as u8, (*b >> 8) as u8, 255),
            Pixel::Rgba16(r, g, b, a) => Pixel::Rgba8((*r >> 8) as u8, (*g >> 8) as u8, (*b >> 8) as u8, (*a >> 8) as u8),
            Pixel::Rgb32(r, g, b) => Pixel::Rgba8((*r >> 24) as u8, (*g >> 24) as u8, (*b >> 24) as u8, 255),
            Pixel::Rgba32(r, g, b, a) => Pixel::Rgba8((*r >> 24) as u8, (*g >> 24) as u8, (*b >> 24) as u8, (*a >> 24) as u8),
        }
    }

    // Convert any Pixel to Rgb16
    pub fn to_rgb16(&self) -> Self {
        match self {
            Pixel::Rgb8(r, g, b) => Pixel::Rgb16((*r as u16) << 8, (*g as u16) << 8, (*b as u16) << 8),
            Pixel::Rgba8(r, g, b, _) => Pixel::Rgb16((*r as u16) << 8, (*g as u16) << 8, (*b as u16) << 8),
            Pixel::Rgb16(r, g, b) => Pixel::Rgb16(*r, *g, *b),
            Pixel::Rgba16(r, g, b, _) => Pixel::Rgb16(*r, *g, *b),
            Pixel::Rgb32(r, g, b) => Pixel::Rgb16((*r >> 16) as u16, (*g >> 16) as u16, (*b >> 16) as u16),
            Pixel::Rgba32(r, g, b, _) => Pixel::Rgb16((*r >> 16) as u16, (*g >> 16) as u16, (*b >> 16) as u16),
        }
    }

    // Convert any Pixel to Rgba16
    pub fn to_rgba16(&self) -> Self {
        match self {
            Pixel::Rgb8(r, g, b) => Pixel::Rgba16((*r as u16) << 8, (*g as u16) << 8, (*b as u16) << 8, 65535),
            Pixel::Rgba8(r, g, b, a) => Pixel::Rgba16((*r as u16) << 8, (*g as u16) << 8, (*b as u16) << 8, (*a as u16) << 8),
            Pixel::Rgb16(r, g, b) => Pixel::Rgba16(*r, *g, *b, 65535),
            Pixel::Rgba16(r, g, b, a) => Pixel::Rgba16(*r, *g, *b, *a),
            Pixel::Rgb32(r, g, b) => Pixel::Rgba16((*r >> 16) as u16, (*g >> 16) as u16, (*b >> 16) as u16, 65535),
            Pixel::Rgba32(r, g, b, a) => Pixel::Rgba16((*r >> 16) as u16, (*g >> 16) as u16, (*b >> 16) as u16, (*a >> 16) as u16),
        }
    }

    // Convert any Pixel to Rgb32
    pub fn to_rgb32(&self) -> Self {
        match self {
            Pixel::Rgb8(r, g, b) => Pixel::Rgb32((*r as u32) << 24, (*g as u32) << 24, (*b as u32) << 24),
            Pixel::Rgba8(r, g, b, _) => Pixel::Rgb32((*r as u32) << 24, (*g as u32) << 24, (*b as u32) << 24),
            Pixel::Rgb16(r, g, b) => Pixel::Rgb32((*r as u32) << 16, (*g as u32) << 16, (*b as u32) << 16),
            Pixel::Rgba16(r, g, b, _) => Pixel::Rgb32((*r as u32) << 16, (*g as u32) << 16, (*b as u32) << 16),
            Pixel::Rgb32(r, g, b) => Pixel::Rgb32(*r, *g, *b),
            Pixel::Rgba32(r, g, b, _) => Pixel::Rgb32(*r, *g, *b),
        }
    }

    // Convert any Pixel to Rgba32
    pub fn to_rgba32(&self) -> Self {
        match self {
            Pixel::Rgb8(r, g, b) => Pixel::Rgba32((*r as u32) << 24, (*g as u32) << 24, (*b as u32) << 24, 4294967295),
            Pixel::Rgba8(r, g, b, a) => Pixel::Rgba32((*r as u32) << 24, (*g as u32) << 24, (*b as u32) << 24, (*a as u32) << 24),
            Pixel::Rgb16(r, g, b) => Pixel::Rgba32((*r as u32) << 16, (*g as u32) << 16, (*b as u32) << 16, 4294967295),
            Pixel::Rgba16(r, g, b, a) => Pixel::Rgba32((*r as u32) << 16, (*g as u32) << 16, (*b as u32) << 16, (*a as u32) << 16),
            Pixel::Rgb32(r, g, b) => Pixel::Rgba32(*r, *g, *b, 4294967295),
            Pixel::Rgba32(r, g, b, a) => Pixel::Rgba32(*r, *g, *b, *a),
        }
    }
}