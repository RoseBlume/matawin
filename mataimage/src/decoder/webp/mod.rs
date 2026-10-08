//! WebP image decoding implementation.
//!
//! This module parses WebP input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::pixels::pack_rgba;
use std::io::{Read, Seek};

use crate::decoder::bit_reader::LsbBitReader as BitReader;

mod huffman;
use huffman::Huffman;

/// Contains the lossy VP8 decoding implementation used by WebP.
mod vp8_lossy;
use crate::ImageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the compression mode used by a WebP image.
pub enum WebPCompression {
    /// Identifies WebP lossless compression using the VP8L bitstream format.
    Lossless,
    /// Identifies WebP lossy compression using the VP8 bitstream format.
    Lossy,
    /// Animated WebP whose frames use both VP8 and VP8L bitstreams.
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents a decoded WebP image, including still or animated frame data.
pub struct WebP {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,
    /// Stores the compression mode declared or detected for the image.
    pub compression: WebPCompression,
    /// Records whether the decoded image contains meaningful alpha information.
    pub has_alpha: bool,
    /// Records whether the WebP container contains animation frames.
    pub is_animated: bool,

    /// Per-frame duration in milliseconds. Static images contain one zero entry.
    pub frame_durations_ms: Vec<u32>,

    /// Animation loop count from ANIM. `Some(0)` means loop forever.
    /// Static WebP images use `None`.
    pub loop_count: Option<u16>,

    /// Animation background color in public 0xRRGGBBAA format.
    /// Static images use transparent black.
    pub background_color: u32,

    /// Original encoded WebP bytes.
    pub data: Vec<u8>,

    /// Fully composited frames in 0xRRGGBBAA format.
    frames: Vec<Vec<u32>>,
}

impl WebP {
    #[inline]
    /// Constructs a rich WebP decoder from borrowed input.
    ///
    /// The encoded RIFF source is copied once because the rich object retains it.
    /// Use [`crate::Image::from_bytes`] for render-only decoding.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::from_bytes(bytes.to_vec())
    }

    /// Decode an owned WebP buffer without copying the encoded data again.
    ///
    /// The rich object intentionally retains `data`; use [`crate::Image::from_bytes`]
    /// when only renderable pixels are required.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, ImageError> {
        let decoded = decode_webp(&data)?;
        Ok(Self {
            width: decoded.width as u32,
            height: decoded.height as u32,
            compression: decoded.compression,
            has_alpha: decoded.has_alpha,
            is_animated: decoded.is_animated,
            frame_durations_ms: decoded.frame_durations_ms,
            loop_count: decoded.loop_count,
            background_color: decoded.background_color,
            frames: decoded.frames,
            data,
        })
    }

    /// Explicit owned-source constructor for callers that want the rich WebP
    /// representation and source retention.
    #[inline]
    pub fn parse_owned(data: Vec<u8>) -> Result<Self, ImageError> {
        Self::from_bytes(data)
    }

    /// Parses the supplied encoded input into this format-specific representation.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .map_err(|e| ImageError::new(format!("Failed to read WebP: {e}")))?;
        Self::from_bytes(data)
    }

    /// Borrow the first decoded frame. This preserves the old static-image API.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        self.frames.first().map(Vec::as_slice).unwrap_or(&[])
    }

    /// Borrow all fully composited frames without cloning them.
    #[inline]
    pub fn frames_ref(&self) -> &[Vec<u32>] {
        &self.frames
    }

    /// Move the first decoded frame out. For animated images use `into_frames()`
    /// when all frames are required.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.frames.into_iter().next().unwrap_or_default()
    }

    /// Move all decoded frames out without cloning them.
    #[inline]
    pub fn into_frames(self) -> Vec<Vec<u32>> {
        self.frames
    }

    #[inline]
    /// Returns the number of decoded frames available from this image.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Borrow the original encoded WebP data without cloning it.
    #[inline]
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Decodes WebP directly from borrowed input without retaining the RIFF bytes.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let info = crate::ImageInfo::read(bytes)?;
    options.checked_rgba_bytes(info.width, info.height)?;
    let frame_limit = match options.animation {
        crate::AnimationMode::FirstFrame => Some(1),
        crate::AnimationMode::AllFrames => None,
    };
    let decoded = decode_webp_with_policy(
        bytes,
        frame_limit,
        Some(options.limits.max_frames),
    )?;
    let width = u32::try_from(decoded.width)
        .map_err(|_| ImageError::new("WebP width exceeds u32"))?;
    let height = u32::try_from(decoded.height)
        .map_err(|_| ImageError::new("WebP height exceeds u32"))?;
    crate::Image::from_frame_pixels(width, height, decoded.frames, options)
}

impl super::Decoder for WebP {
    #[inline]
    fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    fn height(&self) -> u32 {
        self.height
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        let mut options = crate::DecodeOptions::default();
        options.animation = crate::AnimationMode::AllFrames;
        crate::Image::from_frame_pixels(
            this.width,
            this.height,
            this.frames,
            &options,
        )
    }
}

#[derive(Debug)]
/// Stores one internally decoded WebP image plane as packed RGBA pixels.
struct DecodedImage {
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the image, frame, or plane height in pixels.
    height: usize,
    /// Stores the compression mode declared or detected for the image.
    compression: WebPCompression,
    /// Records whether the decoded image contains meaningful alpha information.
    has_alpha: bool,
    /// Stores the decoded packed pixel values produced by the decoder.
    pixels: Vec<u32>,
}

#[derive(Debug)]
/// Collects decoded WebP dimensions, compression mode, and output frames.
struct DecodedWebP {
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the image, frame, or plane height in pixels.
    height: usize,
    /// Stores the compression mode declared or detected for the image.
    compression: WebPCompression,
    /// Records whether the decoded image contains meaningful alpha information.
    has_alpha: bool,
    /// Records whether the decoded WebP result contains animation.
    is_animated: bool,
    /// Stores decoded WebP frame durations in milliseconds.
    frame_durations_ms: Vec<u32>,
    /// Stores the decoded WebP animation loop count.
    loop_count: Option<u16>,
    /// Stores the animation background color in the decoder's packed pixel representation.
    background_color: u32,
    /// Stores the decoded frames in display order.
    frames: Vec<Vec<u32>>,
}

/// Decodes webp data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_webp(data: &[u8]) -> Result<DecodedWebP, ImageError> {
    decode_webp_with_policy(data, None, None)
}

fn decode_webp_with_policy(
    data: &[u8],
    frame_limit: Option<usize>,
    max_frames: Option<usize>,
) -> Result<DecodedWebP, ImageError> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return Err(ImageError::new("Invalid WebP RIFF signature"));
    }

    let riff_size = le_u32(&data[4..8]) as usize;
    if riff_size < 4 {
        return Err(ImageError::new("Invalid WebP RIFF size"));
    }

    let riff_end = 8usize
        .checked_add(riff_size)
        .ok_or_else(|| ImageError::new("WebP RIFF size overflow"))?;
    if riff_end > data.len() {
        return Err(ImageError::new("Truncated WebP RIFF container"));
    }

    let mut pos = 12usize;
    let mut vp8x_flags: Option<u8> = None;
    let mut canvas: Option<(usize, usize)> = None;
    let mut anim_data: Option<&[u8]> = None;
    let mut animation_frames: Vec<&[u8]> = Vec::new();
    let mut animation_frame_count = 0usize;

    // Static-image chunks.
    let mut vp8l_data: Option<&[u8]> = None;
    let mut vp8_data: Option<&[u8]> = None;
    let mut alpha_data: Option<&[u8]> = None;

    while pos + 8 <= riff_end {
        let fourcc = &data[pos..pos + 4];
        let size = le_u32(&data[pos + 4..pos + 8]) as usize;
        pos += 8;

        let end = pos
            .checked_add(size)
            .ok_or_else(|| ImageError::new("WebP chunk size overflow"))?;
        if end > riff_end {
            return Err(ImageError::new("WebP chunk extends past RIFF boundary"));
        }

        let chunk = &data[pos..end];

        match fourcc {
            b"VP8X" => {
                if vp8x_flags.is_some() {
                    return Err(ImageError::new("WebP contains multiple VP8X chunks"));
                }
                if chunk.len() != 10 {
                    return Err(ImageError::new("Invalid WebP VP8X chunk"));
                }

                let flags = chunk[0];
                if flags & 0xC1 != 0 || chunk[1..4] != [0, 0, 0] {
                    return Err(ImageError::new("Invalid reserved WebP VP8X flags"));
                }

                let width = le_u24(&chunk[4..7]) as usize + 1;
                let height = le_u24(&chunk[7..10]) as usize + 1;
                width
                    .checked_mul(height)
                    .ok_or_else(|| ImageError::new("WebP canvas dimensions overflow"))?;

                vp8x_flags = Some(flags);
                canvas = Some((width, height));
            }

            b"ANIM" => {
                if anim_data.replace(chunk).is_some() {
                    return Err(ImageError::new("WebP contains multiple ANIM chunks"));
                }
            }

            b"ANMF" => {
                animation_frame_count = animation_frame_count
                    .checked_add(1)
                    .ok_or_else(|| ImageError::new("WebP animation frame count overflow"))?;
                if max_frames.is_some_and(|limit| animation_frame_count > limit) {
                    return Err(ImageError::new("WebP animation exceeds configured frame limit"));
                }
                if frame_limit.is_none_or(|limit| animation_frames.len() < limit) {
                    animation_frames.push(chunk);
                }
            }

            b"ALPH" => {
                if alpha_data.replace(chunk).is_some() {
                    return Err(ImageError::new("WebP contains multiple top-level ALPH chunks"));
                }
            }

            b"VP8L" => {
                if vp8l_data.replace(chunk).is_some() {
                    return Err(ImageError::new("WebP contains multiple top-level VP8L bitstreams"));
                }
            }

            b"VP8 " => {
                if vp8_data.replace(chunk).is_some() {
                    return Err(ImageError::new("WebP contains multiple top-level VP8 bitstreams"));
                }
            }

            _ => {}
        }

        if size & 1 != 0 {
            if end >= riff_end || data[end] != 0 {
                return Err(ImageError::new("Invalid WebP RIFF padding"));
            }
        }

        pos = end + (size & 1);
    }

    if pos != riff_end {
        return Err(ImageError::new("Malformed WebP RIFF chunk layout"));
    }

    let animation_flag = vp8x_flags.is_some_and(|flags| flags & 0x02 != 0);

    if animation_flag {
        let (canvas_width, canvas_height) = canvas.ok_or_else(|| {
            ImageError::new("Animated WebP is missing its VP8X canvas dimensions")
        })?;

        let anim = anim_data.ok_or_else(|| {
            ImageError::new("Animated WebP is missing the required ANIM chunk")
        })?;

        if animation_frame_count == 0 {
            return Err(ImageError::new("Animated WebP contains no ANMF frames"));
        }
        if animation_frames.is_empty() {
            return Err(ImageError::new("WebP animation frame limit selected no frames"));
        }

        if vp8l_data.is_some() || vp8_data.is_some() || alpha_data.is_some() {
            return Err(ImageError::new(
                "Animated WebP must store image bitstreams inside ANMF chunks",
            ));
        }

        return decode_animated_webp(
            canvas_width,
            canvas_height,
            anim,
            &animation_frames,
        );
    }

    if !animation_frames.is_empty() {
        return Err(ImageError::new(
            "WebP contains ANMF chunks but the VP8X animation flag is not set",
        ));
    }

    if vp8l_data.is_some() && vp8_data.is_some() {
        return Err(ImageError::new("WebP contains multiple image bitstreams"));
    }

    let image = if let Some(lossless_data) = vp8l_data {
        if alpha_data.is_some() {
            return Err(ImageError::new(
                "Invalid WebP: VP8L must not use a separate ALPH chunk",
            ));
        }

        let lossless = decode_vp8l(lossless_data)?;
        DecodedImage {
            width: lossless.width,
            height: lossless.height,
            compression: WebPCompression::Lossless,
            has_alpha: lossless.has_alpha,
            pixels: lossless.pixels,
        }
    } else if let Some(lossy_data) = vp8_data {
        decode_vp8_chunks(lossy_data, alpha_data)?
    } else {
        return Err(ImageError::new("WebP contains no supported image bitstream"));
    };

    if let Some((cw, ch)) = canvas {
        if cw != image.width || ch != image.height {
            return Err(ImageError::new(format!(
                "WebP canvas size {cw}x{ch} does not match image size {}x{}",
                image.width, image.height
            )));
        }
    }

    Ok(DecodedWebP {
        width: image.width,
        height: image.height,
        compression: image.compression,
        has_alpha: image.has_alpha,
        is_animated: false,
        frame_durations_ms: vec![0],
        loop_count: None,
        background_color: 0x0000_0000,
        frames: vec![image.pixels],
    })
}

/// Decode a static lossy VP8 payload that was already found by a RIFF scan.
fn decode_vp8_chunks(
    vp8_data: &[u8],
    alpha_data: Option<&[u8]>,
) -> Result<DecodedImage, ImageError> {
    let decoded = vp8_lossy::decode(vp8_data)?;
    let alpha = match alpha_data {
        Some(chunk) => Some(decode_webp_alpha(chunk, decoded.width, decoded.height)?),
        None => None,
    };

    let pixels = match alpha.as_deref() {
        Some(a) => yuv420_to_rrggbbaa::<true>(&decoded, a),
        None => yuv420_to_rrggbbaa::<false>(&decoded, &[]),
    };

    Ok(DecodedImage {
        width: decoded.width,
        height: decoded.height,
        compression: WebPCompression::Lossy,
        has_alpha: alpha.is_some(),
        pixels,
    })
}

// ============================================================================
// Animation
// ============================================================================

#[derive(Debug, Clone, Copy)]
/// Stores global WebP animation timing and background information.
struct AnimationHeader {
    /// Stores the animation background color in the decoder's packed pixel representation.
    background_color: u32,
    /// Stores the number of times the WebP animation should repeat.
    loop_count: u16,
}

#[derive(Debug, Clone, Copy)]
/// Stores the position, dimensions, timing, and blending flags for one WebP animation frame.
struct FrameHeader {
    /// Stores the horizontal coordinate or offset.
    x: usize,
    /// Stores the vertical coordinate or offset.
    y: usize,
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the image, frame, or plane height in pixels.
    height: usize,
    /// Stores the display duration of this WebP animation frame in milliseconds.
    duration_ms: u32,
    /// Records whether the animation frame blends with the existing canvas contents.
    blend: bool,
    /// Records whether the frame rectangle is cleared to the animation background after display.
    dispose_to_background: bool,
}


/// Timing information for one streamed WebP animation frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebPFrameInfo {
    /// Frame duration in milliseconds.
    pub duration_ms: u32,
    /// Zero-based display-frame index.
    pub index: usize,
}

/// Low-memory sequential animated-WebP decoder.
///
/// Only one full compositing canvas is retained. Each ANMF payload is decoded on
/// demand, composited, copied into caller-owned output, and then released before
/// the following frame is decoded.
pub struct WebPFrameDecoder<'a> {
    data: &'a [u8],
    riff_end: usize,
    next_chunk_pos: usize,
    canvas_width: usize,
    canvas_height: usize,
    animation: AnimationHeader,
    canvas: Vec<u32>,
    frame_index: usize,
    finished: bool,
}

impl<'a> WebPFrameDecoder<'a> {
    /// Parses the RIFF/animation headers without decoding any frame bitstream.
    pub fn new(data: &'a [u8]) -> Result<Self, ImageError> {
        if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
            return Err(ImageError::new("Invalid WebP RIFF signature"));
        }
        let riff_size = le_u32(&data[4..8]) as usize;
        if riff_size < 4 { return Err(ImageError::new("Invalid WebP RIFF size")); }
        let riff_end = 8usize.checked_add(riff_size).ok_or_else(|| ImageError::new("WebP RIFF size overflow"))?;
        if riff_end > data.len() { return Err(ImageError::new("Truncated WebP RIFF container")); }

        let mut pos = 12usize;
        let mut canvas = None;
        let mut flags = None;
        let mut animation = None;
        let mut first_anmf = None;
        let mut frame_count = 0usize;
        while pos + 8 <= riff_end {
            let header_pos = pos;
            let fourcc = &data[pos..pos + 4];
            let size = le_u32(&data[pos + 4..pos + 8]) as usize;
            pos += 8;
            let end = pos.checked_add(size).ok_or_else(|| ImageError::new("WebP chunk size overflow"))?;
            if end > riff_end { return Err(ImageError::new("WebP chunk extends past RIFF boundary")); }
            let chunk = &data[pos..end];
            match fourcc {
                b"VP8X" => {
                    if chunk.len() != 10 { return Err(ImageError::new("Invalid WebP VP8X chunk")); }
                    if flags.is_some() { return Err(ImageError::new("WebP contains multiple VP8X chunks")); }
                    let f = chunk[0];
                    if f & 0xC1 != 0 || chunk[1..4] != [0, 0, 0] { return Err(ImageError::new("Invalid reserved WebP VP8X flags")); }
                    flags = Some(f);
                    canvas = Some((le_u24(&chunk[4..7]) as usize + 1, le_u24(&chunk[7..10]) as usize + 1));
                }
                b"ANIM" => {
                    if animation.is_some() { return Err(ImageError::new("WebP contains multiple ANIM chunks")); }
                    animation = Some(parse_anim_header(chunk)?);
                }
                b"ANMF" => {
                    frame_count = frame_count.checked_add(1).ok_or_else(|| ImageError::new("WebP animation frame count overflow"))?;
                    if first_anmf.is_none() { first_anmf = Some(header_pos); }
                }
                _ => {}
            }
            if size & 1 != 0 && (end >= riff_end || data[end] != 0) {
                return Err(ImageError::new("Invalid WebP RIFF padding"));
            }
            pos = end + (size & 1);
        }
        if pos != riff_end { return Err(ImageError::new("Malformed WebP RIFF chunk layout")); }
        if !flags.is_some_and(|value| value & 0x02 != 0) {
            return Err(ImageError::unsupported("WebP is not animated"));
        }
        if frame_count == 0 { return Err(ImageError::new("Animated WebP contains no ANMF frames")); }
        let (canvas_width, canvas_height) = canvas.ok_or_else(|| ImageError::new("Animated WebP is missing VP8X canvas dimensions"))?;
        let animation = animation.ok_or_else(|| ImageError::new("Animated WebP is missing ANIM"))?;
        let background_color = animation.background_color;
        let pixel_count = canvas_width.checked_mul(canvas_height).ok_or_else(|| ImageError::new("WebP animation canvas size overflow"))?;
        Ok(Self {
            data,
            riff_end,
            next_chunk_pos: first_anmf.unwrap(),
            canvas_width,
            canvas_height,
            animation,
            canvas: vec![background_color; pixel_count],
            frame_index: 0,
            finished: false,
        })
    }

    /// Animation canvas width.
    #[inline]
    pub fn width(&self) -> u32 { self.canvas_width as u32 }

    /// Animation canvas height.
    #[inline]
    pub fn height(&self) -> u32 { self.canvas_height as u32 }

    /// Packed pixels required by `next_frame`.
    #[inline]
    pub fn required_pixels(&self) -> usize { self.canvas.len() }

    /// Animation loop count from the ANIM header (`0` means infinite).
    #[inline]
    pub const fn loop_count(&self) -> u16 { self.animation.loop_count }

    /// Decodes and composites the next display frame into caller-owned output.
    pub fn next_frame(&mut self, output: &mut [u32]) -> Result<Option<WebPFrameInfo>, ImageError> {
        if self.finished { return Ok(None); }
        if output.len() != self.canvas.len() {
            return Err(ImageError::buffer_size("WebP streaming destination size does not match canvas"));
        }
        let mut pos = self.next_chunk_pos;
        while pos + 8 <= self.riff_end {
            let fourcc: [u8; 4] = self.data[pos..pos + 4]
                .try_into()
                .expect("fourcc bounds checked by loop condition");
            let size = le_u32(&self.data[pos + 4..pos + 8]) as usize;
            let payload = pos + 8;
            let end = payload.checked_add(size).ok_or_else(|| ImageError::new("WebP chunk size overflow"))?;
            if end > self.riff_end { return Err(ImageError::new("WebP chunk extends past RIFF boundary")); }
            self.next_chunk_pos = end + (size & 1);
            pos = self.next_chunk_pos;
            if fourcc != *b"ANMF" { continue; }

            let (header, image) = decode_anmf_frame(&self.data[payload..end], self.canvas_width, self.canvas_height)?;
            render_animation_frame(
                &mut self.canvas,
                self.canvas_width,
                self.canvas_height,
                header,
                &image.pixels,
            );
            output.copy_from_slice(&self.canvas);
            if header.dispose_to_background {
                fill_animation_rect(
                    &mut self.canvas,
                    self.canvas_width,
                    header,
                    self.animation.background_color,
                );
            }
            let result = WebPFrameInfo { duration_ms: header.duration_ms, index: self.frame_index };
            self.frame_index += 1;
            return Ok(Some(result));
        }
        self.finished = true;
        Ok(None)
    }

    /// Releases excess canvas capacity. The current canvas length is retained.
    pub fn shrink_scratch(&mut self) {
        self.canvas.shrink_to_fit();
    }
}

/// Decodes animated webp data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_animated_webp(
    canvas_width: usize,
    canvas_height: usize,
    anim_data: &[u8],
    frame_chunks: &[&[u8]],
) -> Result<DecodedWebP, ImageError> {
    let animation = parse_anim_header(anim_data)?;
    let pixel_count = canvas_width
        .checked_mul(canvas_height)
        .ok_or_else(|| ImageError::new("WebP animation canvas size overflow"))?;

    // The ANIM color is stored as BGRA but every public pixel in this module is
    // represented as 0xRRGGBBAA.
    let mut canvas = vec![animation.background_color; pixel_count];
    let mut frames = Vec::with_capacity(frame_chunks.len());
    let mut durations = Vec::with_capacity(frame_chunks.len());
    let mut compression: Option<WebPCompression> = None;
    let mut has_alpha = (animation.background_color & 0xFF) != 0xFF;

    for (frame_index, &chunk) in frame_chunks.iter().enumerate() {
        let (header, image) = decode_anmf_frame(chunk, canvas_width, canvas_height)?;

        compression = Some(match compression {
            None => image.compression,
            Some(current) if current == image.compression => current,
            Some(_) => WebPCompression::Mixed,
        });

        has_alpha |= image.has_alpha;

        render_animation_frame(
            &mut canvas,
            canvas_width,
            canvas_height,
            header,
            &image.pixels,
        );

        // Store a fully assembled logical canvas. The final requested frame can
        // move the canvas rather than cloning it because no later frame needs
        // the compositing state. This is especially important for FirstFrame.
        let is_last_requested = frame_index + 1 == frame_chunks.len();
        if is_last_requested {
            frames.push(core::mem::take(&mut canvas));
        } else {
            frames.push(canvas.clone());
        }
        durations.push(header.duration_ms);

        // Disposal matters only when another frame will be rendered.
        if !is_last_requested && header.dispose_to_background {
            fill_animation_rect(
                &mut canvas,
                canvas_width,
                header,
                animation.background_color,
            );
        }
    }

    Ok(DecodedWebP {
        width: canvas_width,
        height: canvas_height,
        compression: compression.unwrap_or(WebPCompression::Lossless),
        has_alpha,
        is_animated: true,
        frame_durations_ms: durations,
        loop_count: Some(animation.loop_count),
        background_color: animation.background_color,
        frames,
    })
}

/// Parses anim header from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_anim_header(data: &[u8]) -> Result<AnimationHeader, ImageError> {
    if data.len() != 6 {
        return Err(ImageError::new("Invalid WebP ANIM chunk size"));
    }

    let background_color = pack_rgba(data[2], data[1], data[0], data[3]);
    let loop_count = le_u16(&data[4..6]);

    Ok(AnimationHeader {
        background_color,
        loop_count,
    })
}

/// Decodes anmf frame data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_anmf_frame(
    data: &[u8],
    canvas_width: usize,
    canvas_height: usize,
) -> Result<(FrameHeader, DecodedImage), ImageError> {
    if data.len() < 16 {
        return Err(ImageError::new("Truncated WebP ANMF frame header"));
    }

    let x = (le_u24(&data[0..3]) as usize)
        .checked_mul(2)
        .ok_or_else(|| ImageError::new("WebP animation frame X overflow"))?;
    let y = (le_u24(&data[3..6]) as usize)
        .checked_mul(2)
        .ok_or_else(|| ImageError::new("WebP animation frame Y overflow"))?;
    let width = le_u24(&data[6..9]) as usize + 1;
    let height = le_u24(&data[9..12]) as usize + 1;
    let duration_ms = le_u24(&data[12..15]);
    let flags = data[15];

    if flags & 0xFC != 0 {
        return Err(ImageError::new("Invalid reserved bits in WebP ANMF flags"));
    }

    let right = x
        .checked_add(width)
        .ok_or_else(|| ImageError::new("WebP animation frame width overflow"))?;
    let bottom = y
        .checked_add(height)
        .ok_or_else(|| ImageError::new("WebP animation frame height overflow"))?;
    if right > canvas_width || bottom > canvas_height {
        return Err(ImageError::new("WebP animation frame lies outside the canvas"));
    }

    // B = 0 means alpha blend, B = 1 means overwrite/no-blend.
    let blend = flags & 0x02 == 0;
    let dispose_to_background = flags & 0x01 != 0;

    let mut pos = 16usize;
    let mut alpha_data: Option<&[u8]> = None;
    let mut vp8_data: Option<&[u8]> = None;
    let mut vp8l_data: Option<&[u8]> = None;
    let mut seen_bitstream = false;

    while pos + 8 <= data.len() {
        let fourcc = &data[pos..pos + 4];
        let size = le_u32(&data[pos + 4..pos + 8]) as usize;
        pos += 8;

        let end = pos
            .checked_add(size)
            .ok_or_else(|| ImageError::new("WebP ANMF subchunk size overflow"))?;
        if end > data.len() {
            return Err(ImageError::new("WebP ANMF subchunk is truncated"));
        }

        let payload = &data[pos..end];

        match fourcc {
            b"ALPH" => {
                if seen_bitstream {
                    return Err(ImageError::new(
                        "WebP ANMF ALPH chunk must precede the frame bitstream",
                    ));
                }
                if alpha_data.replace(payload).is_some() {
                    return Err(ImageError::new("WebP ANMF contains multiple ALPH chunks"));
                }
            }

            b"VP8 " => {
                if seen_bitstream || vp8l_data.is_some() {
                    return Err(ImageError::new(
                        "WebP ANMF contains multiple image bitstreams",
                    ));
                }
                vp8_data = Some(payload);
                seen_bitstream = true;
            }

            b"VP8L" => {
                if seen_bitstream || vp8_data.is_some() {
                    return Err(ImageError::new(
                        "WebP ANMF contains multiple image bitstreams",
                    ));
                }
                vp8l_data = Some(payload);
                seen_bitstream = true;
            }

            _ => {}
        }

        if size & 1 != 0 {
            if end >= data.len() || data[end] != 0 {
                return Err(ImageError::new("Invalid WebP ANMF RIFF padding"));
            }
        }

        pos = end + (size & 1);
    }

    if pos != data.len() {
        return Err(ImageError::new("Malformed WebP ANMF frame-data layout"));
    }

    let image = if let Some(lossless_data) = vp8l_data {
        if alpha_data.is_some() {
            return Err(ImageError::new(
                "Invalid WebP ANMF: VP8L must not use a separate ALPH chunk",
            ));
        }

        let lossless = decode_vp8l(lossless_data)?;
        DecodedImage {
            width: lossless.width,
            height: lossless.height,
            compression: WebPCompression::Lossless,
            has_alpha: lossless.has_alpha,
            pixels: lossless.pixels,
        }
    } else if let Some(lossy_data) = vp8_data {
        decode_vp8_chunks(lossy_data, alpha_data)?
    } else {
        return Err(ImageError::new("WebP ANMF frame contains no image bitstream"));
    };

    if image.width != width || image.height != height {
        return Err(ImageError::new(format!(
            "WebP ANMF rectangle is {width}x{height}, but its bitstream decodes to {}x{}",
            image.width, image.height
        )));
    }

    Ok((
        FrameHeader {
            x,
            y,
            width,
            height,
            duration_ms,
            blend,
            dispose_to_background,
        },
        image,
    ))
}

/// Rasterizes animation frame onto the active SVG canvas.
fn render_animation_frame(
    canvas: &mut [u32],
    canvas_width: usize,
    canvas_height: usize,
    frame: FrameHeader,
    pixels: &[u32],
) {
    debug_assert!(frame.x + frame.width <= canvas_width);
    debug_assert!(frame.y + frame.height <= canvas_height);
    debug_assert_eq!(pixels.len(), frame.width * frame.height);

    for y in 0..frame.height {
        let src_start = y * frame.width;
        let dst_start = (frame.y + y) * canvas_width + frame.x;
        let src_row = &pixels[src_start..src_start + frame.width];
        let dst_row = &mut canvas[dst_start..dst_start + frame.width];

        if !frame.blend {
            dst_row.copy_from_slice(src_row);
            continue;
        }

        for (dst, &src) in dst_row.iter_mut().zip(src_row) {
            *dst = blend_rrggbbaa(src, *dst);
        }
    }
}

/// Fills animation rect in the destination image according to the active decoding or rendering state.
fn fill_animation_rect(
    canvas: &mut [u32],
    canvas_width: usize,
    frame: FrameHeader,
    color: u32,
) {
    for y in 0..frame.height {
        let start = (frame.y + y) * canvas_width + frame.x;
        canvas[start..start + frame.width].fill(color);
    }
}

/// Source-over blend for straight-alpha 0xRRGGBBAA pixels.
///
/// The WebP container recommends doing this in linear color space. This
/// integer implementation intentionally blends channel values directly, which
/// is much faster and matches the behavior commonly expected by game/sprite
/// renderers that do not carry an ICC-aware linear framebuffer.
#[inline(always)]
fn blend_rrggbbaa(src: u32, dst: u32) -> u32 {
    let sa = src & 0xFF;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return src;
    }

    let da = dst & 0xFF;
    let inv_sa = 255 - sa;

    // A_num is the output alpha numerator before dividing by 255:
    // sa * 255 + da * (255 - sa).
    let a_num = sa * 255 + da * inv_sa;
    if a_num == 0 {
        return 0;
    }

    #[inline(always)]
    /// Blends the supplied source color or frame data into the destination image.
    fn blend_channel(sc: u32, dc: u32, sa: u32, da: u32, inv_sa: u32, a_num: u32) -> u32 {
        let num = sc * sa * 255 + dc * da * inv_sa;
        (num + a_num / 2) / a_num
    }

    let sr = (src >> 24) & 0xFF;
    let sg = (src >> 16) & 0xFF;
    let sb = (src >> 8) & 0xFF;
    let dr = (dst >> 24) & 0xFF;
    let dg = (dst >> 16) & 0xFF;
    let db = (dst >> 8) & 0xFF;

    let r = blend_channel(sr, dr, sa, da, inv_sa, a_num);
    let g = blend_channel(sg, dg, sa, da, inv_sa, a_num);
    let b = blend_channel(sb, db, sa, da, inv_sa, a_num);
    let a = (a_num + 127) / 255;

    (r << 24) | (g << 16) | (b << 8) | a
}

/// Builds the lookup table used to scale luma values during YUV-to-RGB conversion.
const fn make_y_scale_table() -> [i32; 256] {
    let mut table = [0i32; 256];
    let mut i = 0usize;
    while i < 256 {
        let c = if i < 16 { 0 } else { i as i32 - 16 };
        table[i] = 298 * c;
        i += 1;
    }
    table
}

const Y_SCALE: [i32; 256] = make_y_scale_table();

/// Builds the lookup table used to convert chroma deltas during YUV-to-RGB conversion.
const fn make_chroma_table(multiplier: i32) -> [i32; 256] {
    let mut table = [0i32; 256];
    let mut i = 0usize;
    while i < 256 {
        table[i] = multiplier * (i as i32 - 128);
        i += 1;
    }
    table
}

const R_FROM_V: [i32; 256] = make_chroma_table(409);
const G_FROM_U: [i32; 256] = make_chroma_table(-100);
const G_FROM_V: [i32; 256] = make_chroma_table(-208);
const B_FROM_U: [i32; 256] = make_chroma_table(516);

#[inline(always)]
/// Clamps an integer channel value to the 8-bit `0..=255` range.
fn clip_u8(v: i32) -> u8 {
    if v < 0 { 0 } else if v > 255 { 255 } else { v as u8 }
}

#[inline(always)]
/// Writes yuv pixel into the destination buffer using the format required by this decoder.
unsafe fn write_yuv_pixel<const HAS_ALPHA: bool>(
    dst: *mut u32,
    alpha: *const u8,
    index: usize,
    y: u8,
    r_uv: i32,
    g_uv: i32,
    b_uv: i32,
) {
    // SAFETY: callers guarantee `index` is inside every active plane.
    unsafe {
        let ys = *Y_SCALE.get_unchecked(y as usize);
        let r = clip_u8((ys + r_uv) >> 8);
        let g = clip_u8((ys + g_uv) >> 8);
        let b = clip_u8((ys + b_uv) >> 8);
        let a = if HAS_ALPHA { *alpha.add(index) } else { 0xFF };
        *dst.add(index) = pack_rgba(r, g, b, a);
    }
}

/// Convert VP8's 4:2:0 planes in 2x2 groups. Each U/V sample is converted once
/// and reused for four luma pixels, which substantially reduces arithmetic.
fn yuv420_to_rrggbbaa<const HAS_ALPHA: bool>(
    decoded: &vp8_lossy::Decoded,
    alpha: &[u8],
) -> Vec<u32> {
    let width = decoded.width;
    let height = decoded.height;
    let total = width * height;
    debug_assert!(!HAS_ALPHA || alpha.len() == total);

    let mut pixels = vec![0u32; total];
    let dst = pixels.as_mut_ptr();
    let alpha_ptr = alpha.as_ptr();

    for y in (0..height).step_by(2) {
        let y0 = y * decoded.y_stride;
        let y1 = (y + 1) * decoded.y_stride;
        let out0 = y * width;
        let out1 = (y + 1) * width;
        let uv_row = (y >> 1) * decoded.uv_stride;
        let has_second_row = y + 1 < height;

        for x in (0..width).step_by(2) {
            let uv = uv_row + (x >> 1);
            // SAFETY: padded VP8 planes cover the coded macroblock dimensions.
            let u = unsafe { *decoded.u.get_unchecked(uv) as usize };
            let v = unsafe { *decoded.v.get_unchecked(uv) as usize };
            // Table lookups replace four integer multiplications per 2x2 block.
            let r_uv = unsafe { *R_FROM_V.get_unchecked(v) } + 128;
            let g_uv = unsafe { *G_FROM_U.get_unchecked(u) }
                + unsafe { *G_FROM_V.get_unchecked(v) } + 128;
            let b_uv = unsafe { *B_FROM_U.get_unchecked(u) } + 128;

            unsafe {
                let i00 = out0 + x;
                write_yuv_pixel::<HAS_ALPHA>(
                    dst, alpha_ptr, i00, *decoded.y.get_unchecked(y0 + x),
                    r_uv, g_uv, b_uv,
                );
                if x + 1 < width {
                    let i01 = i00 + 1;
                    write_yuv_pixel::<HAS_ALPHA>(
                        dst, alpha_ptr, i01, *decoded.y.get_unchecked(y0 + x + 1),
                        r_uv, g_uv, b_uv,
                    );
                }
                if has_second_row {
                    let i10 = out1 + x;
                    write_yuv_pixel::<HAS_ALPHA>(
                        dst, alpha_ptr, i10, *decoded.y.get_unchecked(y1 + x),
                        r_uv, g_uv, b_uv,
                    );
                    if x + 1 < width {
                        let i11 = i10 + 1;
                        write_yuv_pixel::<HAS_ALPHA>(
                            dst, alpha_ptr, i11, *decoded.y.get_unchecked(y1 + x + 1),
                            r_uv, g_uv, b_uv,
                        );
                    }
                }
            }
        }
    }

    pixels
}

/// Decode the optional ALPH chunk used by lossy WebP.
fn decode_webp_alpha(data: &[u8], width: usize, height: usize) -> Result<Vec<u8>, ImageError> {
    if data.is_empty() {
        return Err(ImageError::new("Truncated WebP ALPH chunk"));
    }

    let header = data[0];
    let compression = header & 0x03;
    let filter = (header >> 2) & 0x03;
    let preprocessing = (header >> 4) & 0x03;
    let reserved = header >> 6;
    if reserved != 0 || preprocessing > 1 {
        return Err(ImageError::new("Invalid WebP ALPH header"));
    }

    let count = width.checked_mul(height)
        .ok_or_else(|| ImageError::new("WebP alpha plane size overflow"))?;
    let payload = &data[1..];

    let mut alpha = match compression {
        0 => {
            if payload.len() < count {
                return Err(ImageError::new("Truncated uncompressed WebP alpha plane"));
            }
            payload[..count].to_vec()
        }
        1 => {
            // Compressed ALPH uses a VP8L stream without the normal 5-byte
            // signature/dimension header. Recreate that header and reuse the
            // module's lossless decoder. The decoded alpha value is stored in
            // VP8L's green channel.
            if width == 0 || height == 0 || width > 16384 || height > 16384 {
                return Err(ImageError::new("Invalid WebP alpha dimensions"));
            }
            let bits = ((width as u32 - 1) & 0x3fff)
                | (((height as u32 - 1) & 0x3fff) << 14);
            let mut vp8l = Vec::with_capacity(payload.len() + 5);
            vp8l.push(0x2f);
            vp8l.extend_from_slice(&bits.to_le_bytes());
            vp8l.extend_from_slice(payload);
            let decoded = decode_vp8l(&vp8l)?;
            if decoded.width != width || decoded.height != height {
                return Err(ImageError::new("WebP compressed alpha dimensions are invalid"));
            }
            decoded.pixels.into_iter()
                .map(|p| ((p >> 16) & 0xff) as u8)
                .collect()
        }
        _ => return Err(ImageError::new(format!(
            "Unsupported WebP alpha compression method {compression}"
        ))),
    };

    // Undo the alpha prediction filter. Specialize once by filter type so the
    // per-pixel hot loop contains no mode switch. Residual addition is modulo 256.
    match filter {
        0 => {}
        1 => {
            // Horizontal, except the first pixel of each later row uses top.
            for y in 0..height {
                let row = y * width;
                if y != 0 {
                    alpha[row] = alpha[row].wrapping_add(alpha[row - width]);
                }
                for x in 1..width {
                    let i = row + x;
                    alpha[i] = alpha[i].wrapping_add(alpha[i - 1]);
                }
            }
        }
        2 => {
            // First row predicts horizontally; all remaining rows predict from top.
            for x in 1..width {
                alpha[x] = alpha[x].wrapping_add(alpha[x - 1]);
            }
            for y in 1..height {
                let row = y * width;
                let top = row - width;
                for x in 0..width {
                    let i = row + x;
                    alpha[i] = alpha[i].wrapping_add(alpha[top + x]);
                }
            }
        }
        3 => {
            // Gradient: first row is horizontal, first column is vertical.
            for x in 1..width {
                alpha[x] = alpha[x].wrapping_add(alpha[x - 1]);
            }
            for y in 1..height {
                let row = y * width;
                let top = row - width;
                alpha[row] = alpha[row].wrapping_add(alpha[top]);
                for x in 1..width {
                    let i = row + x;
                    let pred = (alpha[i - 1] as i32
                        + alpha[top + x] as i32
                        - alpha[top + x - 1] as i32)
                        .clamp(0, 255) as u8;
                    alpha[i] = alpha[i].wrapping_add(pred);
                }
            }
        }
        _ => unreachable!(),
    }

    Ok(alpha)
}

#[derive(Debug)]
/// Stores the intermediate state of a decoded WebP lossless image.
struct LosslessImage {
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the image, frame, or plane height in pixels.
    height: usize,
    /// Records whether the decoded image contains meaningful alpha information.
    has_alpha: bool,
    /// Stores the decoded packed pixel values produced by the decoder.
    pixels: Vec<u32>, // 0xRRGGBBAA
}

/// Decodes vp8l data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_vp8l(data: &[u8]) -> Result<LosslessImage, ImageError> {
    if data.len() < 5 || data[0] != 0x2F {
        return Err(ImageError::new("Invalid VP8L signature"));
    }

    let header = le_u32(&data[1..5]);
    let width = ((header & 0x3FFF) + 1) as usize;
    let height = (((header >> 14) & 0x3FFF) + 1) as usize;
    let has_alpha = ((header >> 28) & 1) != 0;
    let version = (header >> 29) & 0x7;
    if version != 0 {
        return Err(ImageError::new(format!("Unsupported VP8L version {version}")));
    }

    let mut bits = BitReader::new(&data[5..]);
    let mut current_width = width;
    let current_height = height;
    let mut transforms = Vec::new();
    let mut seen = [false; 4];

    while bits.read_bit()? != 0 {
        let kind = bits.read_bits(2)? as usize;
        if seen[kind] {
            return Err(ImageError::new("VP8L transform appears more than once"));
        }
        seen[kind] = true;

        match kind {
            0 => {
                let size_bits = bits.read_bits(3)? as usize + 2;
                let tw = (current_width).div_ceil(1usize << size_bits);
                let th = (current_height).div_ceil(1usize << size_bits);
                let map = decode_lossless_image_data(&mut bits, tw, th, false)?;
                transforms.push(Transform::Predictor {
                    width: current_width,
                    height: current_height,
                    size_bits,
                    map_width: tw,
                    map,
                });
            }
            1 => {
                let size_bits = bits.read_bits(3)? as usize + 2;
                let tw = (current_width).div_ceil(1usize << size_bits);
                let th = (current_height).div_ceil(1usize << size_bits);
                let map = decode_lossless_image_data(&mut bits, tw, th, false)?;
                transforms.push(Transform::Color {
                    width: current_width,
                    height: current_height,
                    size_bits,
                    map_width: tw,
                    map,
                });
            }
            2 => {
                transforms.push(Transform::SubtractGreen);
            }
            3 => {
                let table_size = bits.read_bits(8)? as usize + 1;
                let mut table = decode_lossless_image_data(&mut bits, table_size, 1, false)?;
                let mut previous = 0u32;
                for color in &mut table {
                    *color = add_argb(*color, previous);
                    previous = *color;
                }

                let width_bits = if table_size <= 2 { 3 }
                    else if table_size <= 4 { 2 }
                    else if table_size <= 16 { 1 }
                    else { 0 };

                let original_width = current_width;
                current_width = (current_width).div_ceil(1usize << width_bits);
                transforms.push(Transform::ColorIndexing {
                    original_width,
                    height: current_height,
                    width_bits,
                    table,
                });
            }
            _ => unreachable!(),
        }
    }

    let mut pixels = decode_lossless_image_data(
        &mut bits,
        current_width,
        current_height,
        true,
    )?;
    let mut image_width = current_width;

    let transform_count = transforms.len();
    let mut converted_to_rgba = false;
    for (transform_index, transform) in transforms.into_iter().rev().enumerate() {
        let final_inverse_transform = transform_index + 1 == transform_count;
        match transform {
            Transform::Predictor { width: w, height: h, size_bits, map_width, map } => {
                if image_width != w || pixels.len() != w * h {
                    return Err(ImageError::new("VP8L predictor transform dimension mismatch"));
                }
                inverse_predictor(&mut pixels, w, h, size_bits, map_width, &map)?;
            }
            Transform::Color { width: w, height: h, size_bits, map_width, map } => {
                if image_width != w || pixels.len() != w * h {
                    return Err(ImageError::new("VP8L color transform dimension mismatch"));
                }
                inverse_color_transform(&mut pixels, w, h, size_bits, map_width, &map);
            }
            Transform::SubtractGreen if final_inverse_transform => {
                // When subtract-green is the final inverse transform, fuse it
                // with VP8L's ARGB -> public RRGGBBAA conversion. This removes
                // one complete image-sized memory pass.
                inverse_subtract_green_to_rgba(&mut pixels);
                converted_to_rgba = true;
            }
            Transform::SubtractGreen => inverse_subtract_green(&mut pixels),
            Transform::ColorIndexing { original_width, height: h, width_bits, table } => {
                pixels = inverse_color_indexing(
                    &pixels,
                    image_width,
                    original_width,
                    h,
                    width_bits,
                    &table,
                )?;
                image_width = original_width;
            }
        }
    }

    if image_width != width || pixels.len() != width * height {
        return Err(ImageError::new("VP8L final image dimensions are invalid"));
    }

    // VP8L internally uses 0xAARRGGBB. If the final subtract-green pass did
    // not already fuse this conversion, use the shared SIMD dispatch now.
    if !converted_to_rgba {
        crate::simd::argb_to_rrggbbaa_in_place(&mut pixels);
    }
    Ok(LosslessImage { width, height, has_alpha, pixels })
}

#[derive(Debug)]
/// Represents a reversible transform used by WebP lossless decoding.
enum Transform {
    /// Represents the WebP lossless predictor transform and its decoded predictor-mode image.
    Predictor {
        width: usize,
        height: usize,
        size_bits: usize,
        map_width: usize,
        map: Vec<u32>,
    },
    /// Represents the WebP lossless cross-color transform and its per-block transform data.
    Color {
        width: usize,
        height: usize,
        size_bits: usize,
        map_width: usize,
        map: Vec<u32>,
    },
    /// Represents the WebP lossless subtract-green transform.
    SubtractGreen,
    /// Represents the WebP lossless color-indexing transform and its decoded palette.
    ColorIndexing {
        original_width: usize,
        height: usize,
        width_bits: usize,
        table: Vec<u32>,
    },
}

#[derive(Debug)]
/// Groups the Huffman tables used together for one WebP lossless meta-code.
struct HuffmanGroup {
    /// Stores the Huffman decoder for the green/literal channel alphabet.
    green: Huffman,
    /// Stores the Huffman decoder for the red-channel alphabet.
    red: Huffman,
    /// Stores the Huffman decoder for the blue-channel alphabet.
    blue: Huffman,
    /// Stores the Huffman decoder for the alpha-channel alphabet.
    alpha: Huffman,
    /// Stores the Huffman decoder for backward-distance codes.
    distance: Huffman,
}

/// Reads huffman from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_huffman(bits: &mut BitReader<'_>, alphabet_size: usize) -> Result<Huffman, ImageError> {
    let simple = bits.read_bit()? != 0;
    let mut lengths = vec![0u8; alphabet_size];

    if simple {
        let num_symbols = bits.read_bit()? as usize + 1;
        let first_is_8 = bits.read_bit()? as u32;
        let first_bits = 1 + 7 * first_is_8;
        let symbol0 = bits.read_bits(first_bits)? as usize;
        if symbol0 >= alphabet_size {
            return Err(ImageError::new("VP8L simple Huffman symbol is out of range"));
        }
        lengths[symbol0] = 1;
        if num_symbols == 2 {
            let symbol1 = bits.read_bits(8)? as usize;
            if symbol1 >= alphabet_size {
                return Err(ImageError::new("VP8L simple Huffman symbol is out of range"));
            }
            lengths[symbol1] = 1;
        }
        return Huffman::from_lengths(&lengths);
    }

    const ORDER: [usize; 19] = [17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let num_code_lengths = bits.read_bits(4)? as usize + 4;
    let mut code_length_lengths = [0u8; 19];
    for i in 0..num_code_lengths {
        code_length_lengths[ORDER[i]] = bits.read_bits(3)? as u8;
    }
    let code_length_huffman = Huffman::from_lengths(&code_length_lengths)?;

    let max_symbol = if bits.read_bit()? == 0 {
        alphabet_size
    } else {
        let length_nbits = 2 + 2 * bits.read_bits(3)?;
        2 + bits.read_bits(length_nbits)? as usize
    };
    if max_symbol > alphabet_size {
        return Err(ImageError::new("VP8L Huffman max symbol exceeds alphabet"));
    }

    let mut i = 0usize;
    let mut codes_left = max_symbol;
    let mut previous_nonzero = 8u8;
    while i < alphabet_size && codes_left != 0 {
        codes_left -= 1;
        let code = code_length_huffman.decode(bits)? as u8;
        match code {
            0..=15 => {
                lengths[i] = code;
                if code != 0 { previous_nonzero = code; }
                i += 1;
            }
            16 => {
                let repeat = 3 + bits.read_bits(2)? as usize;
                if i + repeat > alphabet_size {
                    return Err(ImageError::new("VP8L Huffman repeat exceeds alphabet"));
                }
                for slot in &mut lengths[i..i + repeat] { *slot = previous_nonzero; }
                i += repeat;
            }
            17 => {
                let repeat = 3 + bits.read_bits(3)? as usize;
                if i + repeat > alphabet_size {
                    return Err(ImageError::new("VP8L Huffman zero repeat exceeds alphabet"));
                }
                i += repeat;
            }
            18 => {
                let repeat = 11 + bits.read_bits(7)? as usize;
                if i + repeat > alphabet_size {
                    return Err(ImageError::new("VP8L Huffman zero repeat exceeds alphabet"));
                }
                i += repeat;
            }
            _ => return Err(ImageError::new("Invalid VP8L code-length symbol")),
        }
    }

    Huffman::from_lengths(&lengths)
}

/// Decodes lossless image data data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_lossless_image_data(
    bits: &mut BitReader<'_>,
    width: usize,
    height: usize,
    allow_meta: bool,
) -> Result<Vec<u32>, ImageError> {
    let total = width.checked_mul(height)
        .ok_or_else(|| ImageError::new("VP8L image size overflow"))?;

    let cache_bits = if bits.read_bit()? == 0 {
        0usize
    } else {
        let n = bits.read_bits(4)? as usize;
        if !(1..=11).contains(&n) {
            return Err(ImageError::new("Invalid VP8L color-cache size"));
        }
        n
    };
    let cache_size = if cache_bits == 0 { 0 } else { 1usize << cache_bits };

    let meta = if allow_meta && bits.read_bit()? != 0 {
        let prefix_bits = bits.read_bits(3)? as usize + 2;
        let meta_width = (width).div_ceil(1usize << prefix_bits);
        let meta_height = (height).div_ceil(1usize << prefix_bits);
        let image = decode_lossless_image_data(bits, meta_width, meta_height, false)?;
        let max_group = image.iter()
            .map(|&p| ((p >> 8) & 0xFFFF) as usize)
            .max()
            .unwrap_or(0);
        Some(MetaImage {
            prefix_bits,
            width: meta_width,
            image,
            group_count: max_group + 1,
        })
    } else {
        None
    };

    let group_count = meta.as_ref().map_or(1, |m| m.group_count);
    let mut groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        groups.push(HuffmanGroup {
            green: read_huffman(bits, 256 + 24 + cache_size)?,
            red: read_huffman(bits, 256)?,
            blue: read_huffman(bits, 256)?,
            alpha: read_huffman(bits, 256)?,
            distance: read_huffman(bits, 40)?,
        });
    }

    // Pre-size the output. This avoids Vec capacity checks and growth in the
    // literal/LZ hot path and lets bulk backward references write directly.
    let mut pixels = vec![0u32; total];
    let mut cache = vec![0u32; cache_size];
    let mut pos = 0usize;
    let mut x = 0usize;
    let mut y = 0usize;
    let mut cached_meta_x = usize::MAX;
    let mut cached_meta_y = usize::MAX;
    let mut cached_group_index = 0usize;

    while pos < total {
        let group_index = if let Some(meta) = &meta {
            let meta_x = x >> meta.prefix_bits;
            let meta_y = y >> meta.prefix_bits;
            if meta_x != cached_meta_x || meta_y != cached_meta_y {
                let map_index = meta_y * meta.width + meta_x;
                let p = unsafe { *meta.image.get_unchecked(map_index) };
                cached_group_index = ((p >> 8) & 0xFFFF) as usize;
                cached_meta_x = meta_x;
                cached_meta_y = meta_y;
            }
            cached_group_index
        } else {
            0
        };
        // `group_count` was derived from the maximum value in the meta image.
        let group = unsafe { groups.get_unchecked(group_index) };

        let symbol = group.green.decode(bits)? as usize;
        if symbol < 256 {
            // Channel codes occur in red, blue, alpha order after green.
            let red = group.red.decode(bits)? as u8;
            let blue = group.blue.decode(bits)? as u8;
            let alpha = group.alpha.decode(bits)? as u8;
            let color = pack_argb(alpha, red, symbol as u8, blue);
            unsafe { *pixels.get_unchecked_mut(pos) = color; }
            if cache_size != 0 {
                cache_insert_nonempty(&mut cache, cache_bits, color);
            }
            pos += 1;
            x += 1;
            if x == width { x = 0; y += 1; }
        } else if symbol < 280 {
            let length = decode_prefix_value(bits, symbol - 256)?;
            let distance_prefix = group.distance.decode(bits)? as usize;
            let distance_code = decode_prefix_value(bits, distance_prefix)?;
            let distance = map_distance(distance_code, width)?;
            if distance == 0 || distance > pos {
                return Err(ImageError::new("Invalid VP8L backward-reference distance"));
            }
            let end = pos.checked_add(length)
                .ok_or_else(|| ImageError::new("VP8L backward-reference length overflow"))?;
            if end > total {
                return Err(ImageError::new("VP8L backward reference exceeds image"));
            }

            if cache_size == 0 {
                lz_copy_no_cache(&mut pixels, pos, distance, length);
                pos = end;
            } else {
                while pos < end {
                    let color = unsafe { *pixels.get_unchecked(pos - distance) };
                    unsafe { *pixels.get_unchecked_mut(pos) = color; }
                    cache_insert_nonempty(&mut cache, cache_bits, color);
                    pos += 1;
                }
            }

            let advance = x + length;
            y += advance / width;
            x = advance % width;
        } else {
            if cache_size == 0 {
                return Err(ImageError::new("VP8L uses color cache without declaring one"));
            }
            let index = symbol - 280;
            if index >= cache_size {
                return Err(ImageError::new("VP8L color-cache index is out of range"));
            }
            let color = unsafe { *cache.get_unchecked(index) };
            unsafe { *pixels.get_unchecked_mut(pos) = color; }
            cache_insert_nonempty(&mut cache, cache_bits, color);
            pos += 1;
            x += 1;
            if x == width { x = 0; y += 1; }
        }
    }

    Ok(pixels)
}

#[inline(always)]
/// Inserts a non-empty pixel value into the WebP lossless color cache.
fn cache_insert_nonempty(cache: &mut [u32], cache_bits: usize, color: u32) {
    debug_assert!(!cache.is_empty() && cache_bits != 0);
    let hash = 0x1E35_A7BDu32.wrapping_mul(color);
    let index = (hash >> (32 - cache_bits)) as usize;
    unsafe { *cache.get_unchecked_mut(index) = color; }
}

#[inline]
/// Copies an LZ77 back-reference without updating a color cache.
fn lz_copy_no_cache(
    pixels: &mut [u32],
    pos: usize,
    distance: usize,
    length: usize,
) {
    debug_assert!(distance != 0 && distance <= pos && pos + length <= pixels.len());
    if length == 0 { return; }

    if distance == 1 {
        let color = unsafe { *pixels.get_unchecked(pos - 1) };
        pixels[pos..pos + length].fill(color);
        return;
    }

    unsafe {
        let ptr = pixels.as_mut_ptr();
        if length <= distance {
            std::ptr::copy_nonoverlapping(ptr.add(pos - distance), ptr.add(pos), length);
            return;
        }

        // Seed one full period, then repeatedly double the initialized prefix.
        std::ptr::copy_nonoverlapping(ptr.add(pos - distance), ptr.add(pos), distance);
        let mut copied = distance;
        while copied < length {
            let n = (length - copied).min(copied);
            std::ptr::copy_nonoverlapping(ptr.add(pos), ptr.add(pos + copied), n);
            copied += n;
        }
    }
}

/// Stores WebP meta-prefix image data used to select Huffman groups.
struct MetaImage {
    /// Stores the number of low-order prefix bits used to map meta-image pixels to Huffman groups.
    prefix_bits: usize,
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the decoded meta-prefix image containing Huffman-group selectors.
    image: Vec<u32>,
    /// Stores the number of Huffman groups referenced by the meta-prefix image.
    group_count: usize,
}

/// Decodes prefix value data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_prefix_value(bits: &mut BitReader<'_>, prefix: usize) -> Result<usize, ImageError> {
    if prefix < 4 { return Ok(prefix + 1); }
    let extra_bits = (prefix - 2) >> 1;
    let offset = (2 + (prefix & 1)) << extra_bits;
    Ok(offset + bits.read_bits(extra_bits as u32)? as usize + 1)
}

const DISTANCE_MAP: [(i16, i16); 120] = [
    (0,1),(1,0),(1,1),(-1,1),(0,2),(2,0),(1,2),(-1,2),(2,1),(-2,1),
    (2,2),(-2,2),(0,3),(3,0),(1,3),(-1,3),(3,1),(-3,1),(2,3),(-2,3),
    (3,2),(-3,2),(0,4),(4,0),(1,4),(-1,4),(4,1),(-4,1),(3,3),(-3,3),
    (2,4),(-2,4),(4,2),(-4,2),(0,5),(3,4),(-3,4),(4,3),(-4,3),(5,0),
    (1,5),(-1,5),(5,1),(-5,1),(2,5),(-2,5),(5,2),(-5,2),(4,4),(-4,4),
    (3,5),(-3,5),(5,3),(-5,3),(0,6),(6,0),(1,6),(-1,6),(6,1),(-6,1),
    (2,6),(-2,6),(6,2),(-6,2),(4,5),(-4,5),(5,4),(-5,4),(3,6),(-3,6),
    (6,3),(-6,3),(0,7),(7,0),(1,7),(-1,7),(5,5),(-5,5),(7,1),(-7,1),
    (4,6),(-4,6),(6,4),(-6,4),(2,7),(-2,7),(7,2),(-7,2),(3,7),(-3,7),
    (7,3),(-7,3),(5,6),(-5,6),(6,5),(-6,5),(8,0),(4,7),(-4,7),(7,4),
    (-7,4),(8,1),(8,2),(6,6),(-6,6),(8,3),(5,7),(-5,7),(7,5),(-7,5),
    (8,4),(6,7),(-6,7),(7,6),(-7,6),(8,5),(7,7),(-7,7),(8,6),(8,7),
];

/// Maps a WebP lossless distance code to the corresponding backward pixel distance.
///
/// Returns an error when the supplied data cannot be processed as required.
fn map_distance(code: usize, width: usize) -> Result<usize, ImageError> {
    if code == 0 { return Err(ImageError::new("Invalid VP8L distance code 0")); }
    if code > 120 { return Ok(code - 120); }
    let (x, y) = DISTANCE_MAP[code - 1];
    let dist = x as isize + y as isize * width as isize;
    Ok(dist.max(1) as usize)
}

/// Reverses the WebP lossless predictor transform for the decoded image.
///
/// Returns an error when the supplied data cannot be processed as required.
fn inverse_predictor(
    pixels: &mut [u32], width: usize, height: usize,
    size_bits: usize, map_width: usize, map: &[u32],
) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Ok(());
    }

    pixels[0] = add_argb(pixels[0], 0xFF00_0000);
    for x in 1..width {
        pixels[x] = add_argb(pixels[x], pixels[x - 1]);
    }

    let block_size = 1usize << size_bits;
    for y in 1..height {
        let row = y * width;
        pixels[row] = add_argb(pixels[row], pixels[row - width]);
        let map_row = (y >> size_bits) * map_width;
        let mut x = 1usize;

        while x < width {
            let bx = x >> size_bits;
            let x_end = ((bx + 1) * block_size).min(width);
            let mode_pixel = unsafe { *map.get_unchecked(map_row + bx) };
            let mode = green(mode_pixel);
            if mode > 13 {
                return Err(ImageError::new(format!("Invalid VP8L predictor mode {mode}")));
            }
            apply_predictor_run(pixels, width, row, x, x_end, mode);
            x = x_end;
        }
    }
    Ok(())
}

#[inline]
/// Applies predictor run to the supplied decoding or rendering state.
fn apply_predictor_run(
    pixels: &mut [u32],
    width: usize,
    row: usize,
    start: usize,
    end: usize,
    mode: u8,
) {
    #[inline(always)]
    /// Returns the top-right neighboring pixel used by WebP lossless predictor modes.
    fn tr(pixels: &[u32], width: usize, row: usize, x: usize) -> u32 {
        if x + 1 < width { pixels[row - width + x + 1] } else { pixels[row - width] }
    }

    match mode {
        0 => for x in start..end {
            let i = row + x;
            pixels[i] = add_argb(pixels[i], 0xFF00_0000);
        },
        1 => for x in start..end {
            let i = row + x;
            pixels[i] = add_argb(pixels[i], pixels[i - 1]);
        },
        2 => for x in start..end {
            let i = row + x;
            pixels[i] = add_argb(pixels[i], pixels[i - width]);
        },
        3 => for x in start..end {
            let i = row + x;
            pixels[i] = add_argb(pixels[i], tr(pixels, width, row, x));
        },
        4 => for x in start..end {
            let i = row + x;
            pixels[i] = add_argb(pixels[i], pixels[i - width - 1]);
        },
        5 => for x in start..end {
            let i = row + x;
            let l = pixels[i - 1];
            let t = pixels[i - width];
            let pred = avg_argb(avg_argb(l, tr(pixels, width, row, x)), t);
            pixels[i] = add_argb(pixels[i], pred);
        },
        6 => for x in start..end {
            let i = row + x;
            let pred = avg_argb(pixels[i - 1], pixels[i - width - 1]);
            pixels[i] = add_argb(pixels[i], pred);
        },
        7 => for x in start..end {
            let i = row + x;
            let pred = avg_argb(pixels[i - 1], pixels[i - width]);
            pixels[i] = add_argb(pixels[i], pred);
        },
        8 => for x in start..end {
            let i = row + x;
            let pred = avg_argb(pixels[i - width - 1], pixels[i - width]);
            pixels[i] = add_argb(pixels[i], pred);
        },
        9 => for x in start..end {
            let i = row + x;
            let pred = avg_argb(pixels[i - width], tr(pixels, width, row, x));
            pixels[i] = add_argb(pixels[i], pred);
        },
        10 => for x in start..end {
            let i = row + x;
            let ltl = avg_argb(pixels[i - 1], pixels[i - width - 1]);
            let ttr = avg_argb(pixels[i - width], tr(pixels, width, row, x));
            pixels[i] = add_argb(pixels[i], avg_argb(ltl, ttr));
        },
        11 => for x in start..end {
            let i = row + x;
            let pred = select_predictor(pixels[i - 1], pixels[i - width], pixels[i - width - 1]);
            pixels[i] = add_argb(pixels[i], pred);
        },
        12 => for x in start..end {
            let i = row + x;
            let l = pixels[i - 1];
            let t = pixels[i - width];
            let tl = pixels[i - width - 1];
            let pred = map_channels3(l, t, tl, |a, b, c| clamp_i32(a as i32 + b as i32 - c as i32));
            pixels[i] = add_argb(pixels[i], pred);
        },
        13 => for x in start..end {
            let i = row + x;
            let a = avg_argb(pixels[i - 1], pixels[i - width]);
            let tl = pixels[i - width - 1];
            let pred = map_channels2(a, tl, |v, p| clamp_i32(v as i32 + (v as i32 - p as i32) / 2));
            pixels[i] = add_argb(pixels[i], pred);
        },
        _ => unreachable!(),
    }
}

#[inline(always)]
/// Selects and evaluates the WebP lossless predictor indicated by the predictor-mode code.
fn select_predictor(l: u32, t: u32, tl: u32) -> u32 {
    let mut dl = 0i32;
    let mut dt = 0i32;
    let mut shift = 0u32;
    while shift < 32 {
        let lc = ((l >> shift) & 0xFF) as i32;
        let tc = ((t >> shift) & 0xFF) as i32;
        let tlc = ((tl >> shift) & 0xFF) as i32;
        let p = lc + tc - tlc;
        dl += (p - lc).abs();
        dt += (p - tc).abs();
        shift += 8;
    }
    if dl < dt { l } else { t }
}

/// Reverses the WebP lossless color-transform step for all decoded pixels.
fn inverse_color_transform(
    pixels: &mut [u32], width: usize, height: usize,
    size_bits: usize, map_width: usize, map: &[u32],
) {
    let block_size = 1usize << size_bits;
    let map_height = (height).div_ceil(block_size);
    for by in 0..map_height {
        let y0 = by * block_size;
        let y1 = (y0 + block_size).min(height);
        for bx in 0..map_width {
            let x0 = bx * block_size;
            if x0 >= width { break; }
            let x1 = (x0 + block_size).min(width);
            let cte = map[by * map_width + bx];
            let green_to_red = blue(cte) as i8;
            let green_to_blue = green(cte) as i8;
            let red_to_blue = red(cte) as i8;
            for y in y0..y1 {
                let base = y * width;
                for x in x0..x1 {
                    let i = base + x;
                    let p = pixels[i];
                    let a = alpha(p);
                    let g = green(p);
                    let r0 = red(p);
                    let b0 = blue(p);
                    let r = r0.wrapping_add(color_delta(green_to_red, g));
                    let b = b0
                        .wrapping_add(color_delta(green_to_blue, g))
                        .wrapping_add(color_delta(red_to_blue, r));
                    pixels[i] = pack_argb(a, r, g, b);
                }
            }
        }
    }
}

#[inline]
/// Computes the signed color-channel delta used by the WebP lossless color transform.
fn color_delta(t: i8, c: u8) -> u8 {
    (((t as i32 * c as i8 as i32) >> 5) & 0xFF) as u8
}

/// Reverses the WebP lossless subtract-green transform.
fn inverse_subtract_green(pixels: &mut [u32]) {
    for p in pixels {
        let value = *p;
        let g = (value >> 8) & 0xFF;
        let r = (((value >> 16) & 0xFF).wrapping_add(g)) & 0xFF;
        let b = ((value & 0xFF).wrapping_add(g)) & 0xFF;
        *p = (value & 0xFF00_FF00) | (r << 16) | b;
    }
}

#[inline]
/// Reverses subtract-green and converts VP8L ARGB to public RRGGBBAA in one pass.
fn inverse_subtract_green_to_rgba(pixels: &mut [u32]) {
    for p in pixels {
        let value = *p;
        let a = (value >> 24) as u8;
        let g = (value >> 8) as u8;
        let r = (value >> 16) as u8;
        let b = value as u8;
        let r = r.wrapping_add(g);
        let b = b.wrapping_add(g);
        *p = ((r as u32) << 24)
            | ((g as u32) << 16)
            | ((b as u32) << 8)
            | a as u32;
    }
}

/// Expands color indices through the decoded WebP lossless palette.
///
/// Returns an error when the supplied data cannot be processed as required.
fn inverse_color_indexing(
    input: &[u32], input_width: usize, output_width: usize, height: usize,
    width_bits: usize, table: &[u32],
) -> Result<Vec<u32>, ImageError> {
    if input.len() != input_width * height {
        return Err(ImageError::new("VP8L palette input size mismatch"));
    }
    let mut output = vec![0u32; output_width * height];
    for y in 0..height {
        let src = &input[y * input_width..(y + 1) * input_width];
        let dst = &mut output[y * output_width..(y + 1) * output_width];
        let mut dx = 0usize;
        for &packed in src {
            if dx >= output_width { break; }
            let g = green(packed) as usize;
            match width_bits {
                0 => {
                    if g >= table.len() { return Err(ImageError::new("VP8L palette index is out of range")); }
                    dst[dx] = table[g]; dx += 1;
                }
                1 => for shift in [0usize, 4] {
                    if dx == output_width { break; }
                    let index = (g >> shift) & 0x0F;
                    if index >= table.len() { return Err(ImageError::new("VP8L palette index is out of range")); }
                    dst[dx] = table[index]; dx += 1;
                },
                2 => for shift in [0usize, 2, 4, 6] {
                    if dx == output_width { break; }
                    let index = (g >> shift) & 0x03;
                    if index >= table.len() { return Err(ImageError::new("VP8L palette index is out of range")); }
                    dst[dx] = table[index]; dx += 1;
                },
                3 => for shift in 0..8 {
                    if dx == output_width { break; }
                    let index = (g >> shift) & 1;
                    if index >= table.len() { return Err(ImageError::new("VP8L palette index is out of range")); }
                    dst[dx] = table[index]; dx += 1;
                },
                _ => return Err(ImageError::new("Invalid VP8L palette packing")),
            }
        }
    }
    Ok(output)
}

#[inline]
/// Packs argb components into the decoder's compact pixel representation.
fn pack_argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}
/// Extracts the alpha channel from a packed ARGB pixel.
#[inline] fn alpha(p: u32) -> u8 { (p >> 24) as u8 }
/// Extracts the red channel from a packed ARGB pixel.
#[inline] fn red(p: u32) -> u8 { (p >> 16) as u8 }
/// Extracts the green channel from a packed ARGB pixel.
#[inline] fn green(p: u32) -> u8 { (p >> 8) as u8 }
/// Extracts the blue channel from a packed ARGB pixel.
#[inline] fn blue(p: u32) -> u8 { p as u8 }

#[inline(always)]
/// Adds corresponding packed ARGB channels with byte-wise wrapping semantics.
fn add_argb(a: u32, b: u32) -> u32 {
    let even = (a & 0x00FF_00FF).wrapping_add(b & 0x00FF_00FF) & 0x00FF_00FF;
    let odd = ((a >> 8) & 0x00FF_00FF)
        .wrapping_add((b >> 8) & 0x00FF_00FF) & 0x00FF_00FF;
    even | (odd << 8)
}

#[inline(always)]
/// Computes the per-channel rounded average of two packed ARGB pixels.
fn avg_argb(a: u32, b: u32) -> u32 {
    (a & b).wrapping_add(((a ^ b) & 0xFEFE_FEFE) >> 1)
}

#[inline(always)]
/// Applies a two-input channel function independently to all four ARGB channels.
fn map_channels2(a: u32, b: u32, f: impl Fn(u8, u8) -> u8) -> u32 {
    pack_argb(
        f(alpha(a), alpha(b)), f(red(a), red(b)),
        f(green(a), green(b)), f(blue(a), blue(b)),
    )
}

#[inline(always)]
/// Applies a three-input channel function independently to all four ARGB channels.
fn map_channels3(a: u32, b: u32, c: u32, f: impl Fn(u8, u8, u8) -> u8) -> u32 {
    pack_argb(
        f(alpha(a), alpha(b), alpha(c)), f(red(a), red(b), red(c)),
        f(green(a), green(b), green(c)), f(blue(a), blue(b), blue(c)),
    )
}

/// Clamps the supplied value to the valid numeric range used by the decoder.
#[inline] fn clamp_i32(v: i32) -> u8 { v.clamp(0, 255) as u8 }
/// Reads an unsigned 16-bit little-endian integer from the first two bytes of the supplied slice.
#[inline] fn le_u16(b: &[u8]) -> u16 { u16::from_le_bytes([b[0], b[1]]) }
/// Reads an unsigned 32-bit little-endian integer from the first four bytes of the supplied slice.
#[inline] fn le_u32(b: &[u8]) -> u32 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }
/// Reads an unsigned 24-bit little-endian integer from the first three bytes of the supplied slice.
#[inline] fn le_u24(b: &[u8]) -> u32 { b[0] as u32 | ((b[1] as u32) << 8) | ((b[2] as u32) << 16) }

