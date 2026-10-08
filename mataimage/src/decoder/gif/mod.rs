//! GIF image decoding implementation.
//!
//! This module parses GIF input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::decoder::byte_reader::{
    read_u16_le_advance as read_u16_le, read_u8_advance as read_u8,
};
use crate::pixels::pack_rgba;
use crate::ImageError;

/// Fixed-capacity GIF color table. GIF palettes contain at most 256 entries,
/// so keeping the table inline avoids a heap allocation for every local frame palette.
#[derive(Debug, Clone)]
struct GifPalette {
    entries: [u32; 256],
    len: usize,
}

impl GifPalette {
    #[inline]
    fn get(&self, index: usize) -> Option<&u32> {
        (index < self.len).then(|| &self.entries[index])
    }
}

#[derive(Debug, Clone)]
/// Represents a decoded GIF image and its animation frames.
pub struct Gif {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// Complete composited frames.
    ///
    /// Each pixel is 0xRRGGBBAA.
    pub frames: Vec<Vec<u32>>,

    /// Delay for each frame in hundredths of a second.
    ///
    /// GIF stores delays in units of 1/100 second.
    pub frame_delays: Vec<u16>,

    /// Number of animation loops.
    ///
    /// `None` means no NETSCAPE loop extension was present.
    ///
    /// `Some(0)` means loop forever.
    pub loop_count: Option<u16>,
}

// ============================================================================
// Graphic Control Extension
// ============================================================================

#[derive(Debug, Clone, Copy)]
/// Stores the GIF Graphic Control Extension state applied to the next image frame.
struct GraphicControl {
    /// Stores the GIF disposal method applied after displaying the associated frame.
    disposal_method: u8,
    /// Stores the optional palette index treated as transparent for the associated frame.
    transparent_index: Option<u8>,
    /// Stores the frame delay in hundredths of a second.
    delay: u16,
}

impl Default for GraphicControl {
    /// Returns the default value used to initialize this decoding or rendering state.
    fn default() -> Self {
        Self {
            disposal_method: 0,
            transparent_index: None,
            delay: 0,
        }
    }
}

// ============================================================================
// GIF implementation
// ============================================================================

impl Gif {
    /// Borrows all fully composited GIF frames without cloning them.
    #[inline]
    pub fn frames_ref(&self) -> &[Vec<u32>] {
        &self.frames
    }

    /// Borrows the first fully composited frame.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        self.frames.first().map(Vec::as_slice).unwrap_or(&[])
    }

    /// Moves all decoded frames out of this rich GIF decoder.
    #[inline]
    pub fn into_frames(self) -> Vec<Vec<u32>> {
        self.frames
    }

    /// Moves the first decoded frame out. Use [`Self::into_frames`] to retain
    /// every animation frame.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.frames.into_iter().next().unwrap_or_default()
    }

    /// Constructs a new decoder value from the supplied encoded byte slice.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::decode_with_policy(bytes, None, None)
    }

    fn decode_with_policy(
        bytes: &[u8],
        frame_limit: Option<usize>,
        max_frames: Option<usize>,
    ) -> Result<Self, ImageError> {
        if bytes.len() < 13 {
            return Err(ImageError::new(
                "File is too small to contain a GIF header",
            ));
        }

        // --------------------------------------------------------------------
        // Signature
        // --------------------------------------------------------------------

        if &bytes[0..6] != b"GIF87a"
            && &bytes[0..6] != b"GIF89a"
        {
            return Err(ImageError::new(
                "Invalid GIF signature",
            ));
        }

        let mut offset = 6usize;

        // --------------------------------------------------------------------
        // Logical Screen Descriptor
        // --------------------------------------------------------------------

        let width =
            read_u16_le(bytes, &mut offset)? as u32;

        let height =
            read_u16_le(bytes, &mut offset)? as u32;

        if width == 0 || height == 0 {
            return Err(ImageError::new(
                "GIF dimensions cannot be zero",
            ));
        }

        let packed =
            read_u8(bytes, &mut offset)?;

        let global_color_table_flag =
            packed & 0x80 != 0;

        let global_color_table_size =
            1usize << ((packed & 0x07) + 1);

        let background_color_index =
            read_u8(bytes, &mut offset)?;

        // Pixel aspect ratio.
        let _pixel_aspect_ratio =
            read_u8(bytes, &mut offset)?;

        // --------------------------------------------------------------------
        // Global Color Table
        // --------------------------------------------------------------------

        let global_palette =
            if global_color_table_flag {
                Some(
                    read_color_table(
                        bytes,
                        &mut offset,
                        global_color_table_size,
                    )?
                )
            } else {
                None
            };

        let pixel_count =
            (width as usize)
                .checked_mul(height as usize)
                .ok_or_else(|| {
                    ImageError::new(
                        "GIF dimensions overflow",
                    )
                })?;

        // --------------------------------------------------------------------
        // Background color
        //
        // We keep the initial canvas transparent. This generally behaves
        // better for GIFs intended as sprite assets.
        // --------------------------------------------------------------------

        let background_pixel =
            global_palette
                .as_ref()
                .and_then(|palette| {
                    palette.get(
                        background_color_index as usize
                    )
                })
                .copied()
                .unwrap_or(0x000000FF);

        let mut canvas =
            vec![0x00000000u32; pixel_count];

        let mut frames =
            Vec::new();

        let mut frame_delays =
            Vec::new();

        let mut loop_count =
            None;

        let mut graphic_control =
            GraphicControl::default();

        // Reused across animation frames. GIF LZW has fixed 4096-entry
        // dictionaries, so keeping one decoder-level scratch block avoids
        // recreating those arrays for every frame. Disposal method 3 also
        // reuses one canvas backup instead of allocating a new Vec per frame.
        let mut lzw_scratch = GifLzwScratch::new();
        let mut restore_canvas = Vec::new();

        // ====================================================================
        // Blocks
        // ====================================================================

        loop {
            let marker =
                read_u8(bytes, &mut offset)?;

            match marker {
                // ============================================================
                // Trailer
                // ============================================================

                0x3B => {
                    break;
                }

                // ============================================================
                // Extension
                // ============================================================

                0x21 => {
                    let label =
                        read_u8(bytes, &mut offset)?;

                    match label {
                        // ----------------------------------------------------
                        // Graphic Control Extension
                        // ----------------------------------------------------

                        0xF9 => {
                            graphic_control =
                                parse_graphic_control_extension(
                                    bytes,
                                    &mut offset,
                                )?;
                        }

                        // ----------------------------------------------------
                        // Application Extension
                        // ----------------------------------------------------

                        0xFF => {
                            let loops =
                                parse_application_extension(
                                    bytes,
                                    &mut offset,
                                )?;

                            if loops.is_some() {
                                loop_count = loops;
                            }
                        }

                        // ----------------------------------------------------
                        // Comment / Plain Text / unknown extensions
                        // ----------------------------------------------------

                        _ => {
                            skip_extension(
                                bytes,
                                &mut offset,
                            )?;
                        }
                    }
                }

                // ============================================================
                // Image Descriptor
                // ============================================================

                0x2C => {
                    let frame =
                        decode_image_block(
                            bytes,
                            &mut offset,
                            width,
                            height,
                            &global_palette,
                            &mut canvas,
                            background_pixel,
                            graphic_control,
                            frame_limit.is_none_or(|limit| frames.len() + 1 < limit),
                            None,
                            &mut lzw_scratch,
                            &mut restore_canvas,
                        )?;

                    let next_count = frames.len() + 1;
                    if max_frames.is_some_and(|limit| next_count > limit) {
                        return Err(ImageError::new("GIF animation exceeds configured frame limit"));
                    }
                    frames.push(frame);

                    frame_delays.push(
                        graphic_control.delay
                    );

                    // GCE applies only to the immediately following image.
                    graphic_control =
                        GraphicControl::default();

                    // First-frame mode deliberately stops parsing after the
                    // first complete display frame so later compressed image
                    // data is never decoded or retained.
                    if frame_limit.is_some_and(|limit| frames.len() >= limit) {
                        break;
                    }
                }

                // ============================================================
                // Invalid
                // ============================================================

                other => {
                    return Err(ImageError::new(
                        format!(
                            "Unknown GIF block marker: 0x{other:02X}",
                        )
                    ));
                }
            }
        }

        if frames.is_empty() {
            return Err(ImageError::new(
                "GIF contains no image frames",
            ));
        }

        Ok(Self {
            width,
            height,
            frames,
            frame_delays,
            loop_count,
        })
    }

    /// Frame delays converted to milliseconds.
    pub fn frame_delays_ms(&self) -> Vec<u32> {
        self.frame_delays
            .iter()
            .map(|&delay| delay as u32 * 10)
            .collect()
    }

    #[inline]
    /// Returns the number of decoded frames available from this image.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}


/// Timing information for one streamed GIF display frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GifFrameInfo {
    /// Frame delay in milliseconds.
    pub delay_ms: u32,
    /// Zero-based display-frame index.
    pub index: usize,
}

/// Low-memory sequential GIF animation decoder.
///
/// The decoder owns one compositing canvas, one optional restore canvas, and one
/// reusable LZW dictionary. `next_frame` copies the current composited canvas
/// into caller-owned storage and never retains prior display frames.
pub struct GifFrameDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
    width: u32,
    height: u32,
    global_palette: Option<GifPalette>,
    background_pixel: u32,
    canvas: Vec<u32>,
    restore_canvas: Vec<u32>,
    graphic_control: GraphicControl,
    lzw_scratch: GifLzwScratch,
    loop_count: Option<u16>,
    frame_index: usize,
    finished: bool,
}

impl<'a> GifFrameDecoder<'a> {
    /// Creates a sequential decoder without decoding any image frame.
    pub fn new(bytes: &'a [u8]) -> Result<Self, ImageError> {
        if bytes.len() < 13 {
            return Err(ImageError::new("File is too small to contain a GIF header"));
        }
        if &bytes[..6] != b"GIF87a" && &bytes[..6] != b"GIF89a" {
            return Err(ImageError::new("Invalid GIF signature"));
        }
        let mut offset = 6usize;
        let width = read_u16_le(bytes, &mut offset)? as u32;
        let height = read_u16_le(bytes, &mut offset)? as u32;
        if width == 0 || height == 0 {
            return Err(ImageError::new("GIF dimensions cannot be zero"));
        }
        let packed = read_u8(bytes, &mut offset)?;
        let global_color_table_flag = packed & 0x80 != 0;
        let global_color_table_size = 1usize << ((packed & 0x07) + 1);
        let background_color_index = read_u8(bytes, &mut offset)?;
        let _pixel_aspect_ratio = read_u8(bytes, &mut offset)?;
        let global_palette = if global_color_table_flag {
            Some(read_color_table(bytes, &mut offset, global_color_table_size)?)
        } else {
            None
        };
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("GIF dimensions overflow"))?;
        let background_pixel = global_palette
            .as_ref()
            .and_then(|palette| palette.get(background_color_index as usize))
            .copied()
            .unwrap_or(0x000000FF);
        Ok(Self {
            bytes,
            offset,
            width,
            height,
            global_palette,
            background_pixel,
            canvas: vec![0; pixel_count],
            restore_canvas: Vec::new(),
            graphic_control: GraphicControl::default(),
            lzw_scratch: GifLzwScratch::new(),
            loop_count: None,
            frame_index: 0,
            finished: false,
        })
    }

    /// GIF logical-screen width.
    #[inline]
    pub const fn width(&self) -> u32 { self.width }

    /// GIF logical-screen height.
    #[inline]
    pub const fn height(&self) -> u32 { self.height }

    /// Number of packed pixels required by `next_frame`.
    #[inline]
    pub fn required_pixels(&self) -> usize { self.canvas.len() }

    /// Loop count once/if a NETSCAPE application extension has been encountered.
    #[inline]
    pub const fn loop_count(&self) -> Option<u16> { self.loop_count }

    /// Decodes the next composited frame directly into `output`.
    pub fn next_frame(&mut self, output: &mut [u32]) -> Result<Option<GifFrameInfo>, ImageError> {
        if self.finished { return Ok(None); }
        if output.len() != self.canvas.len() {
            return Err(ImageError::buffer_size("GIF streaming destination size does not match canvas"));
        }
        loop {
            let marker = read_u8(self.bytes, &mut self.offset)?;
            match marker {
                0x3b => {
                    self.finished = true;
                    return Ok(None);
                }
                0x21 => {
                    let label = read_u8(self.bytes, &mut self.offset)?;
                    match label {
                        0xf9 => {
                            self.graphic_control = parse_graphic_control_extension(self.bytes, &mut self.offset)?;
                        }
                        0xff => {
                            if let Some(loops) = parse_application_extension(self.bytes, &mut self.offset)? {
                                self.loop_count = Some(loops);
                            }
                        }
                        _ => skip_extension(self.bytes, &mut self.offset)?,
                    }
                }
                0x2c => {
                    let delay = self.graphic_control.delay;
                    let _unused = decode_image_block(
                        self.bytes,
                        &mut self.offset,
                        self.width,
                        self.height,
                        &self.global_palette,
                        &mut self.canvas,
                        self.background_pixel,
                        self.graphic_control,
                        true,
                        Some(output),
                        &mut self.lzw_scratch,
                        &mut self.restore_canvas,
                    )?;
                    let info = GifFrameInfo {
                        delay_ms: u32::from(delay) * 10,
                        index: self.frame_index,
                    };
                    self.frame_index += 1;
                    self.graphic_control = GraphicControl::default();
                    return Ok(Some(info));
                }
                _ => return Err(ImageError::new("Unknown GIF block marker")),
            }
        }
    }

    /// Releases unusually large restore storage while keeping the decoder usable.
    pub fn shrink_scratch(&mut self) {
        self.restore_canvas.shrink_to_fit();
    }
}

// ============================================================================
// Image block
// ============================================================================

#[allow(clippy::too_many_arguments)]
/// Decodes image block data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_image_block(
    bytes: &[u8],
    offset: &mut usize,

    logical_width: u32,
    logical_height: u32,

    global_palette: &Option<GifPalette>,

    canvas: &mut Vec<u32>,

    background_pixel: u32,

    graphic_control: GraphicControl,
    preserve_canvas_after_frame: bool,
    mut output: Option<&mut [u32]>,
    lzw_scratch: &mut GifLzwScratch,
    restore_canvas: &mut Vec<u32>,
) -> Result<Vec<u32>, ImageError> {
    // ------------------------------------------------------------------------
    // Image Descriptor
    // ------------------------------------------------------------------------

    let left =
        read_u16_le(bytes, offset)? as usize;

    let top =
        read_u16_le(bytes, offset)? as usize;

    let image_width =
        read_u16_le(bytes, offset)? as usize;

    let image_height =
        read_u16_le(bytes, offset)? as usize;

    if image_width == 0
        || image_height == 0
    {
        return Err(ImageError::new(
            "GIF image frame has zero dimensions",
        ));
    }

    let packed =
        read_u8(bytes, offset)?;

    let local_color_table_flag =
        packed & 0x80 != 0;

    let interlaced =
        packed & 0x40 != 0;

    let local_color_table_size =
        1usize << ((packed & 0x07) + 1);

    // ------------------------------------------------------------------------
    // Palette
    // ------------------------------------------------------------------------

    let local_palette =
        if local_color_table_flag {
            Some(
                read_color_table(
                    bytes,
                    offset,
                    local_color_table_size,
                )?
            )
        } else {
            None
        };

    let palette =
        local_palette
            .as_ref()
            .or(global_palette.as_ref())
            .ok_or_else(|| {
                ImageError::new(
                    "GIF frame has no color table",
                )
            })?;

    // ------------------------------------------------------------------------
    // LZW
    // ------------------------------------------------------------------------

    let lzw_min_code_size =
        read_u8(bytes, offset)?;

    if lzw_min_code_size > 8 {
        return Err(ImageError::new(
            "Invalid GIF LZW minimum code size",
        ));
    }

    let expected_pixels =
        image_width
            .checked_mul(image_height)
            .ok_or_else(|| {
                ImageError::new(
                    "GIF frame size overflow",
                )
            })?;

    let mut indices =
        decode_gif_lzw_sub_blocks(
            bytes,
            offset,
            lzw_min_code_size,
            expected_pixels,
            lzw_scratch,
        )?;

    if indices.len() < expected_pixels {
        return Err(ImageError::new(
            format!(
                "GIF LZW output too short: expected {}, got {}",
                expected_pixels,
                indices.len()
            ),
        ));
    }

    indices.truncate(expected_pixels);

    // ------------------------------------------------------------------------
    // Deinterlace
    // ------------------------------------------------------------------------

    if interlaced {
        indices =
            deinterlace(
                &indices,
                image_width,
                image_height,
            )?;
    }

    // ------------------------------------------------------------------------
    // Disposal method 3 needs the previous canvas only when another frame will
    // actually be decoded. First-frame mode avoids this full-canvas backup.
    // ------------------------------------------------------------------------

    let restore_previous =
        preserve_canvas_after_frame && graphic_control.disposal_method == 3;
    if restore_previous {
        restore_canvas.resize(canvas.len(), 0);
        restore_canvas.copy_from_slice(canvas);
    }

    // ------------------------------------------------------------------------
    // Draw frame onto canvas
    // ------------------------------------------------------------------------

    let logical_width =
        logical_width as usize;

    let logical_height =
        logical_height as usize;

    for y in 0..image_height {
        let dst_y =
            top + y;

        if dst_y >= logical_height {
            continue;
        }

        for x in 0..image_width {
            let dst_x =
                left + x;

            if dst_x >= logical_width {
                continue;
            }

            let palette_index =
                indices[
                    y * image_width + x
                ];

            if graphic_control.transparent_index
                == Some(palette_index)
            {
                continue;
            }

            let pixel =
                palette
                    .get(palette_index as usize)
                    .copied()
                    .ok_or_else(|| {
                        ImageError::new(
                            format!(
                                "GIF palette index {} is out of range",
                                palette_index
                            )
                        )
                    })?;

            canvas[
                dst_y * logical_width
                    + dst_x
            ] = pixel;
        }
    }

    // ------------------------------------------------------------------------
    // Emit the fully composited frame. Streaming callers provide a reusable
    // destination slice and avoid allocating/cloning a full canvas. Materialized
    // callers retain the historical owned-frame behavior.
    // ------------------------------------------------------------------------

    if let Some(destination) = output.as_deref_mut() {
        if destination.len() != canvas.len() {
            return Err(ImageError::buffer_size("GIF streaming destination size does not match canvas"));
        }
        destination.copy_from_slice(canvas);
    }

    if !preserve_canvas_after_frame {
        return if output.is_some() {
            Ok(Vec::new())
        } else {
            Ok(core::mem::take(canvas))
        };
    }

    let completed_frame = if output.is_some() {
        Vec::new()
    } else {
        canvas.clone()
    };

    // ------------------------------------------------------------------------
    // Disposal
    //
    // 0: unspecified
    // 1: keep
    // 2: restore frame area to background
    // 3: restore previous
    // ------------------------------------------------------------------------

    match graphic_control.disposal_method {
        0 | 1 => {}

        2 => {
            for y in 0..image_height {
                let dst_y =
                    top + y;

                if dst_y >= logical_height {
                    continue;
                }

                for x in 0..image_width {
                    let dst_x =
                        left + x;

                    if dst_x >= logical_width {
                        continue;
                    }

                    canvas[
                        dst_y * logical_width
                            + dst_x
                    ] = background_pixel;
                }
            }
        }

        3 => {
            if restore_previous {
                debug_assert_eq!(restore_canvas.len(), canvas.len());
                canvas.copy_from_slice(restore_canvas);
            }
        }

        // 4-7 are reserved.
        _ => {}
    }

    Ok(completed_frame)
}

// ============================================================================
// Graphic Control Extension
// ============================================================================

/// Parses graphic control extension from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_graphic_control_extension(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<GraphicControl, ImageError> {
    let block_size =
        read_u8(bytes, offset)?;

    if block_size != 4 {
        return Err(ImageError::new(
            "Invalid GIF Graphic Control Extension size",
        ));
    }

    let packed =
        read_u8(bytes, offset)?;

    let delay =
        read_u16_le(bytes, offset)?;

    let transparent_color_index =
        read_u8(bytes, offset)?;

    let terminator =
        read_u8(bytes, offset)?;

    if terminator != 0 {
        return Err(ImageError::new(
            "GIF Graphic Control Extension is missing terminator",
        ));
    }

    let disposal_method =
        (packed >> 2) & 0x07;

    let transparency_flag =
        packed & 0x01 != 0;

    Ok(GraphicControl {
        disposal_method,

        transparent_index:
            if transparency_flag {
                Some(
                    transparent_color_index
                )
            } else {
                None
            },

        delay,
    })
}

// ============================================================================
// Application Extension
// ============================================================================

/// Parses application extension from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_application_extension(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Option<u16>, ImageError> {
    let block_size =
        read_u8(bytes, offset)?;

    if block_size as usize
        > bytes.len().saturating_sub(*offset)
    {
        return Err(ImageError::new(
            "Truncated GIF application extension",
        ));
    }

    let end =
        *offset + block_size as usize;

    let application =
        &bytes[*offset..end];

    *offset = end;

    let data =
        read_sub_blocks(bytes, offset)?;

    // ------------------------------------------------------------------------
    // NETSCAPE2.0 / ANIMEXTS1.0 looping extension
    // ------------------------------------------------------------------------

    let is_loop_extension =
        application == b"NETSCAPE2.0"
            || application == b"ANIMEXTS1.0";

    if is_loop_extension
        && data.len() >= 3
        && data[0] == 1
    {
        let loop_count =
            u16::from_le_bytes([
                data[1],
                data[2],
            ]);

        return Ok(Some(loop_count));
    }

    Ok(None)
}

// ============================================================================
// Skip extension
// ============================================================================

/// Skips over the complete chain of GIF data sub-blocks belonging to an extension.
///
/// Returns an error when the supplied data cannot be processed as required.
fn skip_extension(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<(), ImageError> {
    // Plain Text Extension has a fixed-size header before sub-blocks.
    //
    // Comment extensions immediately start with sub-blocks.
    //
    // To handle arbitrary extensions safely, read the first block and then
    // continue until a zero-sized block.

    loop {
        let size =
            read_u8(bytes, offset)? as usize;

        if size == 0 {
            break;
        }

        let end =
            offset
                .checked_add(size)
                .ok_or_else(|| {
                    ImageError::new(
                        "GIF extension offset overflow",
                    )
                })?;

        if end > bytes.len() {
            return Err(ImageError::new(
                "Truncated GIF extension",
            ));
        }

        *offset = end;
    }

    Ok(())
}

// ============================================================================
// Color table
// ============================================================================

/// Reads color table from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_color_table(
    bytes: &[u8],
    offset: &mut usize,
    entries: usize,
) -> Result<GifPalette, ImageError> {
    let byte_count =
        entries
            .checked_mul(3)
            .ok_or_else(|| {
                ImageError::new(
                    "GIF color table size overflow",
                )
            })?;

    let end =
        offset
            .checked_add(byte_count)
            .ok_or_else(|| {
                ImageError::new(
                    "GIF color table offset overflow",
                )
            })?;

    if end > bytes.len() {
        return Err(ImageError::new(
            "GIF color table is truncated",
        ));
    }

    if entries > 256 {
        return Err(ImageError::new("GIF color table exceeds 256 entries"));
    }

    let mut palette = GifPalette {
        entries: [0; 256],
        len: entries,
    };

    for slot in palette.entries[..entries].iter_mut() {
        let r = bytes[*offset];
        let g = bytes[*offset + 1];
        let b = bytes[*offset + 2];
        *offset += 3;
        *slot = pack_rgba(r, g, b, 0xFF);
    }

    Ok(palette)
}

// ============================================================================
// Data sub-blocks
// ============================================================================

/// Reads sub blocks from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_sub_blocks(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<u8>, ImageError> {
    let mut output =
        Vec::new();

    loop {
        let size =
            read_u8(bytes, offset)? as usize;

        if size == 0 {
            break;
        }

        let end =
            offset
                .checked_add(size)
                .ok_or_else(|| {
                    ImageError::new(
                        "GIF sub-block offset overflow",
                    )
                })?;

        if end > bytes.len() {
            return Err(ImageError::new(
                "GIF data sub-block is truncated",
            ));
        }

        output.extend_from_slice(
            &bytes[*offset..end]
        );

        *offset = end;
    }

    Ok(output)
}


/// Buffered LSB-first bit reader that walks GIF data sub-blocks in-place.
///
/// GIF image data is split into chunks of at most 255 bytes. Decoding directly
/// across those chunks avoids concatenating the compressed payload into a
/// temporary `Vec<u8>` for every frame.
struct GifSubBlockBitReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    block_remaining: usize,
    ended: bool,
    buffer: u64,
    buffered_bits: u32,
}

impl<'a> GifSubBlockBitReader<'a> {
    #[inline]
    fn new(bytes: &'a [u8], offset: usize) -> Self {
        Self {
            bytes,
            offset,
            block_remaining: 0,
            ended: false,
            buffer: 0,
            buffered_bits: 0,
        }
    }

    #[inline]
    fn next_byte(&mut self) -> Result<Option<u8>, ImageError> {
        if self.ended {
            return Ok(None);
        }

        if self.block_remaining == 0 {
            let size = *self.bytes.get(self.offset).ok_or_else(|| {
                ImageError::new("GIF data sub-block length is truncated")
            })? as usize;
            self.offset += 1;
            if size == 0 {
                self.ended = true;
                return Ok(None);
            }
            let end = self.offset.checked_add(size).ok_or_else(|| {
                ImageError::new("GIF data sub-block offset overflow")
            })?;
            if end > self.bytes.len() {
                return Err(ImageError::new("GIF data sub-block is truncated"));
            }
            self.block_remaining = size;
        }

        let byte = self.bytes[self.offset];
        self.offset += 1;
        self.block_remaining -= 1;
        Ok(Some(byte))
    }

    #[inline]
    fn read_bits(&mut self, count: u32) -> Result<u32, ImageError> {
        debug_assert!(count <= 24);
        while self.buffered_bits < count {
            let byte = self.next_byte()?.ok_or_else(|| {
                ImageError::new("Unexpected end of GIF LZW data")
            })?;
            self.buffer |= (byte as u64) << self.buffered_bits;
            self.buffered_bits += 8;
        }
        let mask = if count == 0 { 0 } else { (1u64 << count) - 1 };
        let value = (self.buffer & mask) as u32;
        self.buffer >>= count;
        self.buffered_bits -= count;
        Ok(value)
    }

    /// Consumes any bytes following the LZW end code through the zero-sized
    /// terminator and returns the offset of the next GIF block.
    fn finish(mut self) -> Result<usize, ImageError> {
        if self.ended {
            return Ok(self.offset);
        }

        if self.block_remaining != 0 {
            self.offset = self.offset.checked_add(self.block_remaining).ok_or_else(|| {
                ImageError::new("GIF data sub-block offset overflow")
            })?;
            if self.offset > self.bytes.len() {
                return Err(ImageError::new("GIF data sub-block is truncated"));
            }
            self.block_remaining = 0;
        }

        loop {
            let size = *self.bytes.get(self.offset).ok_or_else(|| {
                ImageError::new("GIF data sub-block terminator is truncated")
            })? as usize;
            self.offset += 1;
            if size == 0 {
                return Ok(self.offset);
            }
            self.offset = self.offset.checked_add(size).ok_or_else(|| {
                ImageError::new("GIF data sub-block offset overflow")
            })?;
            if self.offset > self.bytes.len() {
                return Err(ImageError::new("GIF data sub-block is truncated"));
            }
        }
    }
}

// ============================================================================
// GIF LZW
// ============================================================================

/// Reusable fixed-size working storage for GIF LZW decoding.
struct GifLzwScratch {
    prefix: [u16; 4096],
    suffix: [u8; 4096],
    stack: [u8; 4096],
}

impl GifLzwScratch {
    #[inline]
    fn new() -> Self {
        Self {
            prefix: [0; 4096],
            suffix: [0; 4096],
            stack: [0; 4096],
        }
    }
}

/// Decodes gif lzw data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_gif_lzw_sub_blocks(
    input: &[u8],
    input_offset: &mut usize,
    minimum_code_size: u8,
    expected_size: usize,
    scratch: &mut GifLzwScratch,
) -> Result<Vec<u8>, ImageError> {
    let clear_code =
        1u16 << minimum_code_size;

    let end_code =
        clear_code + 1;

    let first_free_code =
        end_code + 1;

    let mut reader =
        GifSubBlockBitReader::new(input, *input_offset);

    // Maximum GIF LZW dictionary size = 4096 codes. Reuse the same fixed
    // arrays across frames; entries above next_code are never observed.
    let prefix = &mut scratch.prefix;
    let suffix = &mut scratch.suffix;
    let stack = &mut scratch.stack;

    for (i, entry) in suffix[..clear_code as usize].iter_mut().enumerate() {
        *entry = i as u8;
    }

    let mut code_size =
        minimum_code_size as usize + 1;

    let mut next_code =
        first_free_code;

    let mut previous_code:
        Option<u16> =
        None;

    let mut first_character =
        0u8;

    let mut output =
        Vec::with_capacity(expected_size);

    loop {
        let code = reader.read_bits(code_size as u32)? as u16;
        // --------------------------------------------------------------------
        // Clear dictionary
        // --------------------------------------------------------------------

        if code == clear_code {
            code_size =
                minimum_code_size as usize + 1;

            next_code =
                first_free_code;

            previous_code =
                None;

            continue;
        }

        // --------------------------------------------------------------------
        // End
        // --------------------------------------------------------------------

        if code == end_code {
            *input_offset = reader.finish()?;
            break;
        }

        if code >= 4096 {
            return Err(ImageError::new(
                "GIF LZW code exceeds dictionary limit",
            ));
        }

        let mut current_code =
            code;

        let mut stack_size =
            0usize;

        // --------------------------------------------------------------------
        // KwKwK special case
        // --------------------------------------------------------------------

        if current_code >= next_code {
            let previous =
                previous_code.ok_or_else(|| {
                    ImageError::new(
                        "Invalid GIF LZW stream",
                    )
                })?;

            stack[stack_size] =
                first_character;

            stack_size += 1;

            current_code =
                previous;
        }

        // --------------------------------------------------------------------
        // Follow dictionary prefixes
        // --------------------------------------------------------------------

        while current_code
            >= clear_code
        {
            if current_code
                as usize
                >= prefix.len()
            {
                return Err(ImageError::new(
                    "Invalid GIF LZW dictionary reference",
                ));
            }

            if stack_size
                >= stack.len()
            {
                return Err(ImageError::new(
                    "GIF LZW stack overflow",
                ));
            }

            stack[stack_size] =
                suffix[
                    current_code as usize
                ];

            stack_size += 1;

            current_code =
                prefix[
                    current_code as usize
                ];
        }

        first_character =
            suffix[
                current_code as usize
            ];

        if stack_size
            >= stack.len()
        {
            return Err(ImageError::new(
                "GIF LZW stack overflow",
            ));
        }

        stack[stack_size] =
            first_character;

        stack_size += 1;

        // --------------------------------------------------------------------
        // Stack is reversed
        // --------------------------------------------------------------------

        while stack_size > 0 {
            stack_size -= 1;

            output.push(
                stack[stack_size]
            );

            if output.len()
                >= expected_size
            {
                break;
            }
        }

        // --------------------------------------------------------------------
        // Add dictionary entry
        // --------------------------------------------------------------------

        if let Some(previous) =
            previous_code
        {
            if next_code < 4096 {
                prefix[
                    next_code as usize
                ] = previous;

                suffix[
                    next_code as usize
                ] = first_character;

                next_code += 1;

                // GIF grows the code width once the next dictionary value no
                // longer fits into the current width.
                if next_code
                    == (1u16 << code_size)
                    && code_size < 12
                {
                    code_size += 1;
                }
            }
        }

        previous_code =
            Some(code);

        if output.len()
            >= expected_size
        {
            // The caller still needs its cursor positioned after the image-data
            // terminator even when the expected pixel count is reached before an
            // explicit LZW end code.
            *input_offset = reader.finish()?;
            break;
        }
    }

    Ok(output)
}

// ============================================================================
// Interlacing
// ============================================================================

/// Reorders interlaced GIF rows into normal top-to-bottom display order.
///
/// Returns an error when the supplied data cannot be processed as required.
fn deinterlace(
    input: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<u8>, ImageError> {
    let expected =
        width
            .checked_mul(height)
            .ok_or_else(|| {
                ImageError::new(
                    "GIF interlaced frame size overflow",
                )
            })?;

    if input.len() < expected {
        return Err(ImageError::new(
            "GIF interlaced frame data is truncated",
        ));
    }

    let mut output =
        vec![0u8; expected];

    let mut src_y =
        0usize;

    // GIF interlacing passes:
    //
    // Pass 1: 0, 8, 16...
    // Pass 2: 4, 12, 20...
    // Pass 3: 2, 6, 10...
    // Pass 4: 1, 3, 5...

    const PASSES: [(usize, usize); 4] = [
        (0, 8),
        (4, 8),
        (2, 4),
        (1, 2),
    ];

    for (
        start,
        step,
    ) in PASSES {
        let mut dst_y =
            start;

        while dst_y < height {
            if src_y >= height {
                return Err(ImageError::new(
                    "Invalid GIF interlace data",
                ));
            }

            let src_start =
                src_y * width;

            let dst_start =
                dst_y * width;

            output[
                dst_start..dst_start + width
            ]
                .copy_from_slice(
                    &input[
                        src_start
                            ..src_start + width
                    ]
                );

            src_y += 1;

            dst_y += step;
        }
    }

    Ok(output)
}

// ============================================================================
// Primitive readers
// ============================================================================

/// Decodes GIF directly into the lightweight image representation.
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
    let decoded = Gif::decode_with_policy(
        bytes,
        frame_limit,
        Some(options.limits.max_frames),
    )?;
    crate::Image::from_frame_pixels(decoded.width, decoded.height, decoded.frames, options)
}

impl super::Decoder for Gif {
    fn width(&self) -> u32 {
        self.width
    }

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
