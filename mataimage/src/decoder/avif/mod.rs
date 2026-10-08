//! AVIF/AV1 image decoding implementation.
//!
//! This module parses AVIF/AV1 input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::ImageError;
use crate::pixels::pack_rgba;

use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::headers::{
    Dav1dMatrixCoefficients,
    DAV1D_MC_BT2020_CL, DAV1D_MC_BT2020_NCL, DAV1D_MC_BT470BG,
    DAV1D_MC_BT601, DAV1D_MC_BT709, DAV1D_MC_IDENTITY,
    DAV1D_MC_SMPTE240,
    DAV1D_PIXEL_LAYOUT_I400, DAV1D_PIXEL_LAYOUT_I420,
    DAV1D_PIXEL_LAYOUT_I422, DAV1D_PIXEL_LAYOUT_I444,
};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref,
    dav1d_default_settings, dav1d_get_picture, dav1d_open,
    dav1d_picture_unref, dav1d_send_data,
};

use std::mem::MaybeUninit;
use std::ptr::{copy_nonoverlapping, NonNull};

/// `dav1d`/`rav1d` uses the negative POSIX errno value for EAGAIN.
const DAV1D_ERR_AGAIN: i32 = -11;

/// A decoded AV1 frame.
///
/// `data` stores one packed `0xRRGGBBAA` value per pixel.
pub struct Av1 {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,
    /// Stores decoded pixels in logical `0xRRGGBBAA` order.
    pub data: Vec<u32>,
}

impl Av1 {
    /// Borrows decoded packed pixels without allocating.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        &self.data
    }

    /// Moves decoded packed pixels out of the decoder.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.data
    }

    /// Decode the first visible frame from an AV1 elementary stream or IVF file.
    ///
    /// Supported input:
    /// - Raw/low-overhead AV1 OBU stream
    /// - IVF containing AV1 (`AV01`)
    ///
    /// This does not parse MP4, WebM, Matroska, or AVIF containers.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::new_with_threads(bytes, None)
    }

    fn new_with_threads(bytes: &[u8], max_threads: Option<usize>) -> Result<Self, ImageError> {
        if bytes.is_empty() {
            return Err(ImageError::new("AV1 input is empty"));
        }

        let mut decoder = Rav1dDecoder::new(max_threads)?;

        if bytes.starts_with(b"DKIF") {
            Self::decode_ivf(bytes, &mut decoder)
        } else {
            match decoder.send_packet(bytes)? {
                Some(image) => Ok(image),
                None => match decoder.receive_picture()? {
                    Some(image) => Ok(image),
                    None => Err(ImageError::new(
                        "AV1 stream did not produce a decodable frame",
                    )),
                },
            }
        }
    }

    /// Decodes ivf data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_ivf(
        bytes: &[u8],
        decoder: &mut Rav1dDecoder,
    ) -> Result<Self, ImageError> {
        if bytes.len() < 32 {
            return Err(ImageError::new("Truncated IVF header"));
        }

        if &bytes[0..4] != b"DKIF" {
            return Err(ImageError::new("Invalid IVF signature"));
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != 0 {
            return Err(ImageError::new("Unsupported IVF version"));
        }

        let header_len =
            u16::from_le_bytes([bytes[6], bytes[7]]) as usize;

        if header_len < 32 || header_len > bytes.len() {
            return Err(ImageError::new("Invalid IVF header length"));
        }

        if &bytes[8..12] != b"AV01" {
            return Err(ImageError::new("IVF does not contain AV1"));
        }

        let mut offset = header_len;

        while offset < bytes.len() {
            if bytes.len() - offset < 12 {
                return Err(ImageError::new("Truncated IVF frame header"));
            }

            let frame_size = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize;

            offset += 12; // size + timestamp

            let end = offset
                .checked_add(frame_size)
                .ok_or_else(|| ImageError::new("IVF frame size overflow"))?;

            if end > bytes.len() {
                return Err(ImageError::new("Truncated IVF frame"));
            }

            if frame_size != 0 {
                if let Some(image) =
                    decoder.send_packet(&bytes[offset..end])?
                {
                    return Ok(image);
                }
            }

            offset = end;
        }

        match decoder.receive_picture()? {
            Some(image) => Ok(image),
            None => Err(ImageError::new(
                "IVF file did not produce a decodable AV1 frame",
            )),
        }
    }

    /// Converts a decoded rav1d picture into packed `0xRRGGBBAA` pixels and image dimensions.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn from_picture(pic: &Dav1dPicture) -> Result<Self, ImageError> {
        if pic.p.w <= 0 || pic.p.h <= 0 {
            return Err(ImageError::new("Invalid AV1 frame dimensions"));
        }

        let width = pic.p.w as usize;
        let height = pic.p.h as usize;
        let bit_depth = pic.p.bpc as usize;

        if bit_depth != 8 && bit_depth != 10 && bit_depth != 12 {
            return Err(ImageError::new("Unsupported AV1 bit depth"));
        }

        let layout = pic.p.layout;

        let (subsample_x, subsample_y, monochrome) =
            if layout == DAV1D_PIXEL_LAYOUT_I400 {
                (0usize, 0usize, true)
            } else if layout == DAV1D_PIXEL_LAYOUT_I420 {
                (1usize, 1usize, false)
            } else if layout == DAV1D_PIXEL_LAYOUT_I422 {
                (1usize, 0usize, false)
            } else if layout == DAV1D_PIXEL_LAYOUT_I444 {
                (0usize, 0usize, false)
            } else {
                return Err(ImageError::new(
                    "Unsupported AV1 pixel layout",
                ));
            };

        let len = width
            .checked_mul(height)
            .ok_or_else(|| ImageError::new("AV1 frame is too large"))?;

        let mut data = vec![0u32; len];

        let (full_range, matrix) = match pic.seq_hdr {
            Some(seq_ptr) => {
                let seq = unsafe { seq_ptr.as_ref() };
                (seq.color_range != 0, seq.mtrx)
            }
            None => (false, DAV1D_MC_BT709),
        };

        let max_sample = (1u32 << bit_depth) - 1;
        let chroma_mid = 1u32 << (bit_depth - 1);

        for y in 0..height {
            for x in 0..width {
                let yy =
                    Self::read_sample(pic, 0, x, y, bit_depth)? as u32;

                let (uu, vv) = if monochrome {
                    (chroma_mid, chroma_mid)
                } else {
                    let cx = x >> subsample_x;
                    let cy = y >> subsample_y;

                    (
                        Self::read_sample(
                            pic,
                            1,
                            cx,
                            cy,
                            bit_depth,
                        )? as u32,
                        Self::read_sample(
                            pic,
                            2,
                            cx,
                            cy,
                            bit_depth,
                        )? as u32,
                    )
                };

                let (r, g, b) = if monochrome {
                    let value = Self::luma_to_u8(
                        yy,
                        bit_depth,
                        full_range,
                    );
                    (value, value, value)
                } else if matrix == DAV1D_MC_IDENTITY {
                    // AV1 identity matrix represents GBR:
                    // Y = G, U = B, V = R.
                    (
                        Self::sample_to_u8(vv, max_sample),
                        Self::sample_to_u8(yy, max_sample),
                        Self::sample_to_u8(uu, max_sample),
                    )
                } else {
                    Self::yuv_to_rgb(
                        yy,
                        uu,
                        vv,
                        bit_depth,
                        full_range,
                        matrix,
                    )
                };

                let out = y * width + x;
                data[out] = pack_rgba(r, g, b, 255);
            }
        }

        Ok(Self {
            width: width as u32,
            height: height as u32,
            data,
        })
    }

    #[inline(always)]
    /// Reads sample from the supplied input while advancing the relevant cursor or bit position.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn read_sample(
        pic: &Dav1dPicture,
        plane: usize,
        x: usize,
        y: usize,
        bit_depth: usize,
    ) -> Result<u16, ImageError> {
        let ptr = pic.data[plane]
            .ok_or_else(|| ImageError::new("Missing AV1 image plane"))?;

        let stride = if plane == 0 {
            pic.stride[0]
        } else {
            pic.stride[1]
        };

        unsafe {
            let row = ptr
                .as_ptr()
                .cast::<u8>()
                .offset(stride * y as isize);

            if bit_depth <= 8 {
                Ok(*row.add(x) as u16)
            } else {
                let sample_ptr = row.add(x * 2).cast::<u16>();
                Ok(sample_ptr.read_unaligned())
            }
        }
    }

    #[inline(always)]
    /// Processes sample to u8 for use by the decoder's color or pixel reconstruction path.
    fn sample_to_u8(value: u32, max_sample: u32) -> u8 {
        (((value.min(max_sample) * 255) + max_sample / 2)
            / max_sample) as u8
    }

    #[inline(always)]
    /// Converts a decoded luma sample to an 8-bit display value using the active range and bit depth.
    fn luma_to_u8(
        y: u32,
        bit_depth: usize,
        full_range: bool,
    ) -> u8 {
        let max_sample = (1u32 << bit_depth) - 1;

        if full_range {
            return Self::sample_to_u8(y, max_sample);
        }

        let scale = 1u32 << (bit_depth - 8);
        let black = 16 * scale;
        let white = 235 * scale;

        if y <= black {
            0
        } else if y >= white {
            255
        } else {
            (((y - black) * 255 + (white - black) / 2)
                / (white - black)) as u8
        }
    }

    #[inline(always)]
    /// Converts one YUV sample triplet to 8-bit RGB using the selected matrix coefficients and range.
    fn yuv_to_rgb(
        y: u32,
        u: u32,
        v: u32,
        bit_depth: usize,
        full_range: bool,
        matrix: Dav1dMatrixCoefficients,
    ) -> (u8, u8, u8) {
        let max_sample = ((1u32 << bit_depth) - 1) as f32;

        let (yf, cb, cr) = if full_range {
            let center = (1u32 << (bit_depth - 1)) as f32;

            (
                y as f32 / max_sample,
                (u as f32 - center) / max_sample,
                (v as f32 - center) / max_sample,
            )
        } else {
            let scale = (1u32 << (bit_depth - 8)) as f32;

            (
                (y as f32 - 16.0 * scale) / (219.0 * scale),
                (u as f32 - 128.0 * scale) / (224.0 * scale),
                (v as f32 - 128.0 * scale) / (224.0 * scale),
            )
        };

        let (kr, kb) = Self::matrix_kr_kb(matrix);
        let kg = 1.0 - kr - kb;

        let r = yf + 2.0 * (1.0 - kr) * cr;
        let b = yf + 2.0 * (1.0 - kb) * cb;

        let g = yf
            - 2.0 * kb * (1.0 - kb) / kg * cb
            - 2.0 * kr * (1.0 - kr) / kg * cr;

        (
            Self::float_to_u8(r),
            Self::float_to_u8(g),
            Self::float_to_u8(b),
        )
    }

    #[inline(always)]
    /// Returns the luma weighting coefficients associated with the selected YUV matrix identifier.
    fn matrix_kr_kb(
        matrix: Dav1dMatrixCoefficients,
    ) -> (f32, f32) {
        if matrix == DAV1D_MC_BT601
            || matrix == DAV1D_MC_BT470BG
        {
            // BT.601 / BT.470BG / SMPTE 170M family.
            (0.2990, 0.1140)
        } else if matrix == DAV1D_MC_SMPTE240 {
            (0.2120, 0.0870)
        } else if matrix == DAV1D_MC_BT2020_NCL
            || matrix == DAV1D_MC_BT2020_CL
        {
            // BT.2020 constant-luminance technically needs a
            // different nonlinear conversion. This is the common
            // NCL approximation for 8-bit RGBA output.
            (0.2627, 0.0593)
        } else {
            // BT.709 is also the safest fallback for unspecified
            // modern AV1 video.
            (0.2126, 0.0722)
        }
    }

    #[inline(always)]
    /// Rounds and clamps a floating-point channel value to the `0..=255` range.
    fn float_to_u8(value: f32) -> u8 {
        (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }
}

/// Owns the native rav1d decoder context used to produce AV1 pictures.
struct Rav1dDecoder {
    /// Stores the active rav1d decoder context while AV1 packets are being processed.
    context: Option<Dav1dContext>,
}

impl Rav1dDecoder {
    /// Constructs and initializes a new instance of this type.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn new(max_threads: Option<usize>) -> Result<Self, ImageError> {
        let mut settings =
            MaybeUninit::<Dav1dSettings>::uninit();

        unsafe {
            dav1d_default_settings(
                NonNull::new_unchecked(settings.as_mut_ptr()),
            );
        }

        let mut settings = unsafe { settings.assume_init() };

        // Thread selection is process-wide cached by the shared performance
        // policy and may be capped per decode. A single frame of delay keeps
        // the first-visible-frame push/pull loop deterministic.
        let threads = max_threads
            .unwrap_or_else(crate::performance::available_threads)
            .max(1)
            .min(crate::performance::available_threads())
            .min(i32::MAX as usize);
        settings.n_threads = threads as i32;
        settings.max_frame_delay = 1;

        let mut context: Option<Dav1dContext> = None;

        let result = unsafe { dav1d_open(
            Some(NonNull::from(&mut context)),
            Some(NonNull::from(&mut settings)),
        )};

        if result.0 != 0 || context.is_none() {
            return Err(ImageError::new(
                "Failed to initialize rav1d decoder",
            ));
        }

        Ok(Self { context })
    }

    /// Submits encoded AV1 packet data to rav1d and returns a frame if decoding produces one immediately.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn send_packet(
        &mut self,
        bytes: &[u8],
    ) -> Result<Option<Av1>, ImageError> {
        if bytes.is_empty() {
            return Ok(None);
        }

        let mut input = Rav1dInput::new(bytes)?;

        while input.data.sz != 0 {
            let result = unsafe { dav1d_send_data(
                self.context,
                Some(NonNull::from(&mut input.data)),
            ) };

            if result.0 != 0 && result.0 != DAV1D_ERR_AGAIN {
                return Err(ImageError::new(
                    "rav1d rejected AV1 bitstream data",
                ));
            }

            if let Some(image) = self.receive_picture()? {
                return Ok(Some(image));
            }

            // EAGAIN means rav1d could not consume more data until
            // output is drained. If no output was available, retrying
            // cannot make progress, so treat it as malformed input.
            if result.0 == DAV1D_ERR_AGAIN {
                return Err(ImageError::new(
                    "rav1d decoder stalled while consuming AV1 data",
                ));
            }
        }

        self.receive_picture()
    }

    /// Retrieves the next decoded AV1 picture from rav1d when one is available.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn receive_picture(
        &mut self,
    ) -> Result<Option<Av1>, ImageError> {
        let mut picture = Dav1dPicture::default();

        let result = unsafe { dav1d_get_picture(
            self.context,
            Some(NonNull::from(&mut picture)),
        ) };

        if result.0 == DAV1D_ERR_AGAIN {
            return Ok(None);
        }

        if result.0 != 0 {
            return Err(ImageError::new(
                "rav1d failed while decoding AV1 frame",
            ));
        }

        let picture = Rav1dPictureGuard { picture };
        let image = Av1::from_picture(&picture.picture)?;

        Ok(Some(image))
    }
}

impl Drop for Rav1dDecoder {
    /// Releases the native or owned resources associated with this value.
    fn drop(&mut self) {
        unsafe { dav1d_close(Some(NonNull::from(&mut self.context)))};
    }
}

/// Owns a rav1d input packet and the backing data supplied to the decoder.
struct Rav1dInput {
    /// Stores the rav1d input packet data submitted to the decoder.
    data: Dav1dData,
}

impl Rav1dInput {
    /// Constructs a new decoder value from the supplied encoded byte slice.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut data = Dav1dData::default();

        let dst = unsafe { dav1d_data_create(
            Some(NonNull::from(&mut data)),
            bytes.len(),
        ) };

        if dst.is_null() {
            return Err(ImageError::new(
                "Failed to allocate rav1d input buffer",
            ));
        }

        unsafe {
            copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        }

        Ok(Self { data })
    }
}

impl Drop for Rav1dInput {
    /// Releases the native or owned resources associated with this value.
    fn drop(&mut self) {
        unsafe { dav1d_data_unref(Some(NonNull::from(&mut self.data))) };
    }
}

/// Provides scoped ownership of a rav1d picture so it is released on drop.
struct Rav1dPictureGuard {
    /// Stores the rav1d picture released automatically when this guard is dropped.
    picture: Dav1dPicture,
}

impl Drop for Rav1dPictureGuard {
    /// Releases the native or owned resources associated with this value.
    fn drop(&mut self) {
        unsafe { dav1d_picture_unref(Some(NonNull::from(
            &mut self.picture,
        ))) };
    }
}

/// Decodes the first AV1 frame into the lightweight packed representation.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let decoded = Av1::new_with_threads(bytes, options.max_threads)?;
    options.checked_rgba_bytes(decoded.width, decoded.height)?;
    crate::Image::from_packed_pixels(decoded.width, decoded.height, decoded.data, options)
}

impl super::Decoder for Av1 {
    #[inline(always)]
    fn width(&self) -> u32 {
        self.width
    }

    #[inline(always)]
    fn height(&self) -> u32 {
        self.height
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        crate::Image::from_packed_pixels(
            this.width,
            this.height,
            this.data,
            &crate::DecodeOptions::default(),
        )
    }
}
