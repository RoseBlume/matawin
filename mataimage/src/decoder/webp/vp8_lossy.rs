//! WebP lossy VP8 bitstream decoding support.
//!
//! This module contains the internal lossy VP8 bitstream decoding routines used by the WebP decoder.

use crate::ImageError;

const DC_PRED: u8 = 0;
const V_PRED: u8 = 1;
const H_PRED: u8 = 2;
const TM_PRED: u8 = 3;
const B_PRED: u8 = 4;

const B_DC: u8 = 0;
const B_TM: u8 = 1;
const B_VE: u8 = 2;
const B_HE: u8 = 3;
const B_LD: u8 = 4;
const B_RD: u8 = 5;
const B_VR: u8 = 6;
const B_VL: u8 = 7;
const B_HD: u8 = 8;
const B_HU: u8 = 9;

const COEFF_BANDS: [usize; 16] = [0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7];
const ZIGZAG: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];

/// Contains a decoded lossy VP8 image before it is wrapped by the WebP decoder.
pub(super) struct Decoded {
    /// Stores the image, frame, or plane width in pixels.
    pub width: usize,
    /// Stores the image, frame, or plane height in pixels.
    pub height: usize,
    /// Stores the byte stride of the decoded VP8 luma plane.
    pub y_stride: usize,
    /// Stores the byte stride shared by the decoded VP8 chroma planes.
    pub uv_stride: usize,
    /// Stores the decoded VP8 luma plane.
    pub y: Vec<u8>,
    /// Stores the decoded VP8 U/Cb chroma plane.
    pub u: Vec<u8>,
    /// Stores the decoded VP8 V/Cr chroma plane.
    pub v: Vec<u8>,
}

/// Reads entropy-coded VP8 boolean values from a compressed bitstream.
struct BoolReader<'a> {
    /// Stores the encoded input slice consumed by this reader.
    data: &'a [u8],
    /// Stores the current byte position within the encoded input.
    pos: usize,
    /// Stores the current arithmetic-coder range used by the VP8 boolean decoder.
    range: u32,
    /// Stores the current arithmetic-coder value accumulator.
    value: u32,
    /// Stores the number of valid bits currently held by the VP8 boolean decoder.
    bit_count: u8,
}

impl<'a> BoolReader<'a> {
    #[inline]
    /// Constructs and initializes a new instance of this type.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn new(data: &'a [u8]) -> Result<Self, ImageError> {
        if data.len() < 2 {
            return Err(ImageError::new("Truncated VP8 boolean-coded partition"));
        }
        Ok(Self {
            data,
            pos: 2,
            range: 255,
            value: ((data[0] as u32) << 8) | data[1] as u32,
            bit_count: 0,
        })
    }

    #[inline(always)]
    /// Reads bool from the supplied input while advancing the relevant cursor or bit position.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn read_bool(&mut self, probability: u8) -> Result<u8, ImageError> {
        let split = 1 + (((self.range - 1) * probability as u32) >> 8);
        let split_value = split << 8;
        let bit;
        if self.value >= split_value {
            bit = 1;
            self.range -= split;
            self.value -= split_value;
        } else {
            bit = 0;
            self.range = split;
        }

        if self.range < 128 {
            // Normalize in one operation instead of shifting one bit at a
            // time. range is in 1..128 here, so shift is always 1..=7.
            let shift = self.range.leading_zeros() - 24;
            self.range <<= shift;
            self.value <<= shift;

            let count = self.bit_count as u32 + shift;
            if count >= 8 {
                if self.pos >= self.data.len() {
                    return Err(ImageError::new("Unexpected end of VP8 boolean-coded partition"));
                }
                let extra = count - 8;
                // SAFETY: checked immediately above.
                let byte = unsafe { *self.data.get_unchecked(self.pos) };
                self.pos += 1;
                self.value |= (byte as u32) << extra;
                self.bit_count = extra as u8;
            } else {
                self.bit_count = count as u8;
            }
        }
        Ok(bit)
    }

    #[inline(always)]
    /// Decodes one probability-coded boolean bit from the VP8 arithmetic bitstream.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn bit(&mut self) -> Result<u8, ImageError> {
        self.read_bool(128)
    }

    #[inline(always)]
    /// Decodes an unsigned VP8 literal composed of the requested number of entropy-coded bits.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn literal(&mut self, bits: usize) -> Result<u32, ImageError> {
        let mut value = 0u32;
        for _ in 0..bits {
            value = (value << 1) | self.bit()? as u32;
        }
        Ok(value)
    }

    #[inline(always)]
    /// Decodes an optional signed VP8 integer whose presence is indicated by a preceding flag bit.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn optional_signed(&mut self, bits: usize) -> Result<i32, ImageError> {
        if self.bit()? == 0 {
            return Ok(0);
        }
        let magnitude = self.literal(bits)? as i32;
        Ok(if self.bit()? != 0 { -magnitude } else { magnitude })
    }
}

#[derive(Clone)]
/// Stores VP8 segmentation configuration and per-segment adjustments.
struct Segmentation {
    /// Records whether VP8 macroblock segmentation is enabled.
    enabled: bool,
    /// Records whether the VP8 bitstream updates the macroblock segment map.
    update_map: bool,
    /// Records whether segment feature values are absolute rather than frame-relative deltas.
    absolute: bool,
    /// Stores per-segment quantizer adjustments.
    quant: [i32; 4],
    /// Stores per-segment loop-filter adjustments.
    filter: [i32; 4],
    /// Stores probabilities used to decode VP8 segment identifiers.
    probs: [u8; 3],
}

impl Default for Segmentation {
    /// Returns the default value used to initialize this decoding or rendering state.
    fn default() -> Self {
        Self {
            enabled: false,
            update_map: false,
            absolute: false,
            quant: [0; 4],
            filter: [0; 4],
            probs: [255; 3],
        }
    }
}

#[derive(Clone)]
/// Stores VP8 loop-filter configuration parsed from the frame header.
struct FilterHeader {
    /// Records whether the VP8 simple loop-filter mode is selected.
    simple: bool,
    /// Stores the base VP8 loop-filter strength.
    level: i32,
    /// Stores the VP8 loop-filter sharpness setting.
    sharpness: u8,
    /// Records whether reference- and mode-specific filter adjustments are enabled.
    adjustments: bool,
    /// Stores VP8 loop-filter deltas for reference-frame classes.
    ref_delta: [i32; 4],
    /// Stores VP8 loop-filter deltas for prediction-mode classes.
    mode_delta: [i32; 4],
}

#[derive(Clone, Copy)]
/// Stores VP8 quantizer indices and deltas parsed from the frame header.
struct QuantHeader {
    /// Stores the base VP8 luma AC quantizer index.
    y_ac: i32,
    /// Stores the VP8 luma DC quantizer delta.
    y_dc_delta: i32,
    /// Stores the VP8 Y2/WHT DC quantizer delta.
    y2_dc_delta: i32,
    /// Stores the VP8 Y2/WHT AC quantizer delta.
    y2_ac_delta: i32,
    /// Stores the VP8 chroma DC quantizer delta.
    uv_dc_delta: i32,
    /// Stores the VP8 chroma AC quantizer delta.
    uv_ac_delta: i32,
}

#[derive(Clone)]
#[repr(C)]
/// Compact decoded prediction/residual state for one VP8 macroblock.
/// Segment IDs are only 0..=3, so using `u8` avoids pointer-width padding in
/// large macroblock grids.
struct Macroblock {
    /// Stores the VP8 segmentation index assigned to this macroblock.
    segment: u8,
    /// Records whether residual coefficients are skipped for this macroblock.
    skip: bool,
    /// Stores the macroblock-level luma intra-prediction mode.
    y_mode: u8,
    /// Stores the chroma intra-prediction mode.
    uv_mode: u8,
    /// Stores the sixteen 4×4 luma prediction modes when block prediction is used.
    b_modes: [u8; 16],
    /// Stores the effective loop-filter level computed for this macroblock.
    filter_level: u8,
}

#[derive(Clone, Copy)]
/// Stores the derived VP8 quantization factors used for transform coefficients.
struct QuantFactors {
    /// Stores the dequantization factor for first-order luma DC coefficients.
    y1_dc: i32,
    /// Stores the dequantization factor for first-order luma AC coefficients.
    y1_ac: i32,
    /// Stores the dequantization factor for second-order luma DC coefficients.
    y2_dc: i32,
    /// Stores the dequantization factor for second-order luma AC coefficients.
    y2_ac: i32,
    /// Stores the dequantization factor for chroma DC coefficients.
    uv_dc: i32,
    /// Stores the dequantization factor for chroma AC coefficients.
    uv_ac: i32,
}

/// Decodes the supplied encoded data and returns its reconstructed representation.
///
/// Returns an error when the supplied data cannot be processed as required.
pub(super) fn decode(data: &[u8]) -> Result<Decoded, ImageError> {
    if data.len() < 10 {
        return Err(ImageError::new("Truncated VP8 frame"));
    }

    let tag = data[0] as u32 | ((data[1] as u32) << 8) | ((data[2] as u32) << 16);
    if tag & 1 != 0 {
        return Err(ImageError::new("A WebP VP8 image must contain a key frame"));
    }
    let version = ((tag >> 1) & 7) as u8;
    if version > 3 {
        return Err(ImageError::new(format!("Unsupported VP8 version {version}")));
    }
    let first_partition_size = ((tag >> 5) & 0x7ffff) as usize;

    if data[3..6] != [0x9d, 0x01, 0x2a] {
        return Err(ImageError::new("Invalid VP8 key-frame start code"));
    }
    let width_code = u16::from_le_bytes([data[6], data[7]]);
    let height_code = u16::from_le_bytes([data[8], data[9]]);
    let width = (width_code & 0x3fff) as usize;
    let height = (height_code & 0x3fff) as usize;
    if width == 0 || height == 0 {
        return Err(ImageError::new("VP8 frame has zero width or height"));
    }

    let first_end = 10usize.checked_add(first_partition_size)
        .ok_or_else(|| ImageError::new("VP8 first partition size overflow"))?;
    if first_end > data.len() {
        return Err(ImageError::new("Truncated VP8 first partition"));
    }

    let mb_width = (width + 15) / 16;
    let mb_height = (height + 15) / 16;
    let mut control = BoolReader::new(&data[10..first_end])?;

    // Key-frame color_space and clamping_type. WebP uses the normal YUV color space.
    let _color_space = control.bit()?;
    let _clamping_type = control.bit()?;

    let segmentation = read_segmentation(&mut control)?;
    let filter = read_filter_header(&mut control)?;
    let log2_partitions = control.literal(2)? as usize;
    let partition_count = 1usize << log2_partitions;
    let quant = read_quant_header(&mut control)?;

    // Key-frame refresh_entropy_probs flag. It has no persistent-frame effect here.
    let _refresh_entropy_probs = control.bit()?;

    let mut coeff_probs = DEFAULT_COEFF_PROBS;
    for t in 0..4 {
        for b in 0..8 {
            for c in 0..3 {
                for p in 0..11 {
                    let i = coeff_index(t, b, c, p);
                    if control.read_bool(COEFF_UPDATE_PROBS[i])? != 0 {
                        coeff_probs[i] = control.literal(8)? as u8;
                    }
                }
            }
        }
    }

    let mb_no_skip_coeff = control.bit()? != 0;
    let prob_skip_false = if mb_no_skip_coeff {
        control.literal(8)? as u8
    } else {
        0
    };

    let macroblocks = read_macroblock_modes(
        &mut control,
        mb_width,
        mb_height,
        &segmentation,
        &filter,
        mb_no_skip_coeff,
        prob_skip_false,
    )?;

    let token_partitions = split_token_partitions(data, first_end, partition_count)?;
    let mut token_readers = Vec::with_capacity(partition_count);
    for partition in token_partitions {
        token_readers.push(BoolReader::new(partition)?);
    }

    let y_stride = mb_width * 16;
    let uv_stride = mb_width * 8;
    let y_plane_height = mb_height * 16;
    let uv_plane_height = mb_height * 8;
    let mut y_plane = vec![0u8; y_stride * y_plane_height];
    let mut u_plane = vec![0u8; uv_stride * uv_plane_height];
    let mut v_plane = vec![0u8; uv_stride * uv_plane_height];

    // Aggregate non-zero contexts: [Y2, Y0..3, U0..1, V0..1].
    let mut above_ctx = vec![[0u8; 9]; mb_width];
    let quant_by_segment: [QuantFactors; 4] =
        std::array::from_fn(|segment| quant_factors(&quant, &segmentation, segment));

    for mb_y in 0..mb_height {
        let mut left_ctx = [0u8; 9];
        let partition_index = mb_y & (partition_count - 1);
        let tokens = &mut token_readers[partition_index];

        for mb_x in 0..mb_width {
            let mb_index = mb_y * mb_width + mb_x;
            let mb = &macroblocks[mb_index];
            let q = quant_by_segment[mb.segment as usize];

            let y_x0 = mb_x * 16;
            let y_y0 = mb_y * 16;
            let uv_x0 = mb_x * 8;
            let uv_y0 = mb_y * 8;

            if mb.skip {
                if mb.y_mode != B_PRED {
                    above_ctx[mb_x][0] = 0;
                    left_ctx[0] = 0;
                }
                for i in 1..9 {
                    above_ctx[mb_x][i] = 0;
                    left_ctx[i] = 0;
                }

                // Prediction still has to be materialized for skipped blocks.
                if mb.y_mode == B_PRED {
                    for by in 0..4 {
                        for bx in 0..4 {
                            predict_4x4(
                                &mut y_plane, y_stride, mb_width, mb_x, mb_y, bx,
                                y_x0 + bx * 4, y_y0 + by * 4,
                                mb.b_modes[by * 4 + bx],
                            );
                        }
                    }
                } else {
                    predict_block(&mut y_plane, y_stride, y_x0, y_y0, 16, mb.y_mode);
                }
                predict_block(&mut u_plane, uv_stride, uv_x0, uv_y0, 8, mb.uv_mode);
                predict_block(&mut v_plane, uv_stride, uv_x0, uv_y0, 8, mb.uv_mode);
                continue;
            }

            let mut y2_dc = [0i32; 16];
            if mb.y_mode != B_PRED {
                let context = (above_ctx[mb_x][0] + left_ctx[0]) as usize;
                let (y2, y2_eob) = decode_coeff_block(
                    tokens, &coeff_probs, 1, 0, context, q.y2_dc, q.y2_ac,
                )?;
                let y2_nonzero = u8::from(y2_eob != 0);
                above_ctx[mb_x][0] = y2_nonzero;
                left_ctx[0] = y2_nonzero;
                y2_dc = inverse_wht(&y2, y2_eob);
                predict_block(&mut y_plane, y_stride, y_x0, y_y0, 16, mb.y_mode);
            }

            // Decode/reconstruct luma immediately. For B_PRED this is also
            // required by intra prediction because each 4x4 block becomes a
            // predictor for blocks to its right and below.
            for by in 0..4 {
                for bx in 0..4 {
                    let block = by * 4 + bx;
                    let top_slot = 1 + bx;
                    let left_slot = 1 + by;
                    let context = (above_ctx[mb_x][top_slot] + left_ctx[left_slot]) as usize;
                    let (decoded, entropy_eob) = if mb.y_mode == B_PRED {
                        predict_4x4(
                            &mut y_plane, y_stride, mb_width, mb_x, mb_y, bx,
                            y_x0 + bx * 4, y_y0 + by * 4, mb.b_modes[block],
                        );
                        decode_coeff_block(
                            tokens, &coeff_probs, 3, 0, context, q.y1_dc, q.y1_ac,
                        )?
                    } else {
                        let (mut block_coeffs, eob) = decode_coeff_block(
                            tokens, &coeff_probs, 0, 1, context, q.y1_dc, q.y1_ac,
                        )?;
                        block_coeffs[0] = y2_dc[block];
                        (block_coeffs, eob)
                    };
                    let nonzero = u8::from(entropy_eob != 0);
                    above_ctx[mb_x][top_slot] = nonzero;
                    left_ctx[left_slot] = nonzero;
                    let transform_eob = if mb.y_mode == B_PRED {
                        entropy_eob
                    } else if entropy_eob != 0 {
                        entropy_eob.max(1)
                    } else {
                        u8::from(decoded[0] != 0)
                    };
                    add_residue(
                        &mut y_plane, y_stride,
                        y_x0 + bx * 4, y_y0 + by * 4,
                        &decoded, transform_eob,
                    );
                }
            }

            predict_block(&mut u_plane, uv_stride, uv_x0, uv_y0, 8, mb.uv_mode);
            for by in 0..2 {
                for bx in 0..2 {
                    let top_slot = 5 + bx;
                    let left_slot = 5 + by;
                    let context = (above_ctx[mb_x][top_slot] + left_ctx[left_slot]) as usize;
                    let (decoded, eob) = decode_coeff_block(
                        tokens, &coeff_probs, 2, 0, context, q.uv_dc, q.uv_ac,
                    )?;
                    let nonzero = u8::from(eob != 0);
                    above_ctx[mb_x][top_slot] = nonzero;
                    left_ctx[left_slot] = nonzero;
                    add_residue(
                        &mut u_plane, uv_stride,
                        uv_x0 + bx * 4, uv_y0 + by * 4,
                        &decoded, eob,
                    );
                }
            }

            predict_block(&mut v_plane, uv_stride, uv_x0, uv_y0, 8, mb.uv_mode);
            for by in 0..2 {
                for bx in 0..2 {
                    let top_slot = 7 + bx;
                    let left_slot = 7 + by;
                    let context = (above_ctx[mb_x][top_slot] + left_ctx[left_slot]) as usize;
                    let (decoded, eob) = decode_coeff_block(
                        tokens, &coeff_probs, 2, 0, context, q.uv_dc, q.uv_ac,
                    )?;
                    let nonzero = u8::from(eob != 0);
                    above_ctx[mb_x][top_slot] = nonzero;
                    left_ctx[left_slot] = nonzero;
                    add_residue(
                        &mut v_plane, uv_stride,
                        uv_x0 + bx * 4, uv_y0 + by * 4,
                        &decoded, eob,
                    );
                }
            }
        }
    }

    if filter.level != 0 {
        apply_loop_filter(
            &mut y_plane,
            &mut u_plane,
            &mut v_plane,
            y_stride,
            uv_stride,
            mb_width,
            mb_height,
            &macroblocks,
            filter.simple,
            filter.sharpness,
        );
    }

    Ok(Decoded {
        width,
        height,
        y_stride,
        uv_stride,
        y: y_plane,
        u: u_plane,
        v: v_plane,
    })
}

/// Reads segmentation from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_segmentation(reader: &mut BoolReader<'_>) -> Result<Segmentation, ImageError> {
    let mut seg = Segmentation::default();
    seg.enabled = reader.bit()? != 0;
    if !seg.enabled {
        return Ok(seg);
    }

    seg.update_map = reader.bit()? != 0;
    let update_data = reader.bit()? != 0;
    if update_data {
        seg.absolute = reader.bit()? != 0;
        for i in 0..4 {
            if reader.bit()? != 0 {
                let magnitude = reader.literal(7)? as i32;
                seg.quant[i] = if reader.bit()? != 0 { -magnitude } else { magnitude };
            }
        }
        for i in 0..4 {
            if reader.bit()? != 0 {
                let magnitude = reader.literal(6)? as i32;
                seg.filter[i] = if reader.bit()? != 0 { -magnitude } else { magnitude };
            }
        }
    }
    if seg.update_map {
        for p in &mut seg.probs {
            *p = if reader.bit()? != 0 { reader.literal(8)? as u8 } else { 255 };
        }
    }
    Ok(seg)
}

/// Reads filter header from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_filter_header(reader: &mut BoolReader<'_>) -> Result<FilterHeader, ImageError> {
    let simple = reader.bit()? != 0;
    let level = reader.literal(6)? as i32;
    let sharpness = reader.literal(3)? as u8;
    let adjustments = reader.bit()? != 0;
    let mut ref_delta = [0i32; 4];
    let mut mode_delta = [0i32; 4];
    if adjustments && reader.bit()? != 0 {
        for value in &mut ref_delta {
            if reader.bit()? != 0 {
                let magnitude = reader.literal(6)? as i32;
                *value = if reader.bit()? != 0 { -magnitude } else { magnitude };
            }
        }
        for value in &mut mode_delta {
            if reader.bit()? != 0 {
                let magnitude = reader.literal(6)? as i32;
                *value = if reader.bit()? != 0 { -magnitude } else { magnitude };
            }
        }
    }
    Ok(FilterHeader { simple, level, sharpness, adjustments, ref_delta, mode_delta })
}

/// Reads quant header from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_quant_header(reader: &mut BoolReader<'_>) -> Result<QuantHeader, ImageError> {
    Ok(QuantHeader {
        y_ac: reader.literal(7)? as i32,
        y_dc_delta: reader.optional_signed(4)?,
        y2_dc_delta: reader.optional_signed(4)?,
        y2_ac_delta: reader.optional_signed(4)?,
        uv_dc_delta: reader.optional_signed(4)?,
        uv_ac_delta: reader.optional_signed(4)?,
    })
}

/// Reads macroblock modes from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_macroblock_modes(
    reader: &mut BoolReader<'_>,
    mb_width: usize,
    mb_height: usize,
    segmentation: &Segmentation,
    filter: &FilterHeader,
    mb_no_skip_coeff: bool,
    prob_skip_false: u8,
) -> Result<Vec<Macroblock>, ImageError> {
    let mut result = Vec::with_capacity(mb_width * mb_height);
    let mut top_modes = vec![[B_DC; 4]; mb_width];

    for _mb_y in 0..mb_height {
        let mut left_modes = [B_DC; 4];
        for mb_x in 0..mb_width {
            let segment = if segmentation.enabled && segmentation.update_map {
                read_segment_id(reader, &segmentation.probs)?
            } else {
                0
            };
            let skip = mb_no_skip_coeff && reader.read_bool(prob_skip_false)? != 0;
            let y_mode = read_y_mode(reader)?;
            let mut b_modes = [B_DC; 16];

            if y_mode == B_PRED {
                for by in 0..4 {
                    for bx in 0..4 {
                        let i = by * 4 + bx;
                        let above = if by == 0 { top_modes[mb_x][bx] } else { b_modes[(by - 1) * 4 + bx] };
                        let left = if bx == 0 { left_modes[by] } else { b_modes[by * 4 + bx - 1] };
                        b_modes[i] = read_b_mode(reader, above, left)?;
                    }
                }
            } else {
                let b = match y_mode {
                    DC_PRED => B_DC,
                    V_PRED => B_VE,
                    H_PRED => B_HE,
                    TM_PRED => B_TM,
                    _ => return Err(ImageError::new("Invalid VP8 luma prediction mode")),
                };
                b_modes.fill(b);
            }

            top_modes[mb_x] = [b_modes[12], b_modes[13], b_modes[14], b_modes[15]];
            left_modes = [b_modes[3], b_modes[7], b_modes[11], b_modes[15]];
            let uv_mode = read_uv_mode(reader)?;
            let filter_level = macroblock_filter_level(filter, segmentation, segment as usize, y_mode);
            result.push(Macroblock { segment, skip, y_mode, uv_mode, b_modes, filter_level });
        }
    }
    Ok(result)
}

#[inline]
/// Reads segment id from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_segment_id(reader: &mut BoolReader<'_>, p: &[u8; 3]) -> Result<u8, ImageError> {
    if reader.read_bool(p[0])? == 0 {
        Ok(reader.read_bool(p[1])? as u8)
    } else {
        Ok(2 + reader.read_bool(p[2])? as u8)
    }
}

/// Reads y mode from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_y_mode(reader: &mut BoolReader<'_>) -> Result<u8, ImageError> {
    if reader.read_bool(145)? == 0 {
        return Ok(B_PRED);
    }
    if reader.read_bool(156)? == 0 {
        Ok(if reader.read_bool(163)? == 0 { DC_PRED } else { V_PRED })
    } else {
        Ok(if reader.read_bool(128)? == 0 { H_PRED } else { TM_PRED })
    }
}

/// Reads uv mode from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_uv_mode(reader: &mut BoolReader<'_>) -> Result<u8, ImageError> {
    if reader.read_bool(142)? == 0 { return Ok(DC_PRED); }
    if reader.read_bool(114)? == 0 { return Ok(V_PRED); }
    Ok(if reader.read_bool(183)? == 0 { H_PRED } else { TM_PRED })
}

/// Reads b mode from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_b_mode(reader: &mut BoolReader<'_>, above: u8, left: u8) -> Result<u8, ImageError> {
    let base = ((above as usize * 10 + left as usize) * 9) as usize;
    let p = &KF_BMODE_PROBS[base..base + 9];
    if reader.read_bool(p[0])? == 0 { return Ok(B_DC); }
    if reader.read_bool(p[1])? == 0 { return Ok(B_TM); }
    if reader.read_bool(p[2])? == 0 { return Ok(B_VE); }
    if reader.read_bool(p[3])? == 0 {
        if reader.read_bool(p[4])? == 0 { return Ok(B_HE); }
        return Ok(if reader.read_bool(p[5])? == 0 { B_RD } else { B_VR });
    }
    if reader.read_bool(p[6])? == 0 { return Ok(B_LD); }
    if reader.read_bool(p[7])? == 0 { return Ok(B_VL); }
    Ok(if reader.read_bool(p[8])? == 0 { B_HD } else { B_HU })
}

/// Splits token partitions into the component parts required by the parser.
///
/// Returns an error when the supplied data cannot be processed as required.
fn split_token_partitions<'a>(
    data: &'a [u8],
    start: usize,
    count: usize,
) -> Result<Vec<&'a [u8]>, ImageError> {
    let table_bytes = 3usize.checked_mul(count.saturating_sub(1))
        .ok_or_else(|| ImageError::new("VP8 token partition table overflow"))?;
    if start + table_bytes > data.len() {
        return Err(ImageError::new("Truncated VP8 token partition table"));
    }
    let mut sizes = Vec::with_capacity(count);
    for i in 0..count.saturating_sub(1) {
        let p = start + i * 3;
        sizes.push(data[p] as usize | ((data[p + 1] as usize) << 8) | ((data[p + 2] as usize) << 16));
    }
    let mut pos = start + table_bytes;
    let mut out = Vec::with_capacity(count);
    for size in sizes {
        let end = pos.checked_add(size)
            .ok_or_else(|| ImageError::new("VP8 token partition size overflow"))?;
        if end > data.len() {
            return Err(ImageError::new("Truncated VP8 token partition"));
        }
        out.push(&data[pos..end]);
        pos = end;
    }
    out.push(&data[pos..]);
    Ok(out)
}

#[inline(always)]
/// Maps VP8 coefficient coordinates to the corresponding probability/context table index.
fn coeff_index(t: usize, b: usize, c: usize, p: usize) -> usize {
    (((t * 8 + b) * 3 + c) * 11) + p
}

#[inline]
/// Decodes coeff block data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_coeff_block(
    reader: &mut BoolReader<'_>,
    probabilities: &[u8; 1056],
    block_type: usize,
    first: usize,
    initial_context: usize,
    dc_quant: i32,
    ac_quant: i32,
) -> Result<([i32; 16], u8), ImageError> {
    let mut out = [0i32; 16];
    let mut context = initial_context.min(2);
    let mut previous_zero = false;
    let mut eob = 0u8;

    for coefficient in first..16 {
        let band = COEFF_BANDS[coefficient];
        let base = coeff_index(block_type, band, context, 0);
        // SAFETY: coeff_index always addresses an 11-byte probability node
        // inside the fixed 1056-byte table.
        let p = unsafe { probabilities.get_unchecked(base..base + 11) };

        if !previous_zero && reader.read_bool(p[0])? == 0 {
            break;
        }

        let token = if reader.read_bool(p[1])? == 0 {
            0
        } else if reader.read_bool(p[2])? == 0 {
            1
        } else if reader.read_bool(p[3])? == 0 {
            if reader.read_bool(p[4])? == 0 {
                2
            } else if reader.read_bool(p[5])? == 0 {
                3
            } else {
                4
            }
        } else if reader.read_bool(p[6])? == 0 {
            if reader.read_bool(p[7])? == 0 { 5 } else { 6 }
        } else if reader.read_bool(p[8])? == 0 {
            if reader.read_bool(p[9])? == 0 { 7 } else { 8 }
        } else if reader.read_bool(p[10])? == 0 {
            9
        } else {
            10
        };

        let magnitude = match token {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 => 5 + read_category(reader, &[159])?,
            6 => 7 + read_category(reader, &[165, 145])?,
            7 => 11 + read_category(reader, &[173, 148, 140])?,
            8 => 19 + read_category(reader, &[176, 155, 140, 135])?,
            9 => 35 + read_category(reader, &[180, 157, 141, 134, 130])?,
            10 => 67 + read_category(reader, &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129])?,
            _ => unreachable!(),
        } as i32;

        if magnitude != 0 {
            let signed = if reader.bit()? != 0 { -magnitude } else { magnitude };
            let quantizer = if coefficient == 0 { dc_quant } else { ac_quant };
            // SAFETY: ZIGZAG contains only 0..15.
            unsafe { *out.get_unchecked_mut(ZIGZAG[coefficient]) = signed * quantizer; }
            eob = (coefficient + 1) as u8;
        }
        previous_zero = magnitude == 0;
        context = if magnitude == 0 { 0 } else if magnitude == 1 { 1 } else { 2 };
    }
    Ok((out, eob))
}

#[inline(always)]
/// Reads category from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_category(reader: &mut BoolReader<'_>, probabilities: &[u8]) -> Result<u32, ImageError> {
    let mut value = 0u32;
    for &p in probabilities {
        value = (value << 1) | reader.read_bool(p)? as u32;
    }
    Ok(value)
}

/// Derives the VP8 quantization factors for a macroblock or segment from frame quantizer settings.
fn quant_factors(header: &QuantHeader, segmentation: &Segmentation, segment: usize) -> QuantFactors {
    let base = if segmentation.enabled {
        if segmentation.absolute { segmentation.quant[segment] } else { header.y_ac + segmentation.quant[segment] }
    } else {
        header.y_ac
    };
    let q = base.clamp(0, 127);
    let index = |delta: i32| -> usize { (q + delta).clamp(0, 127) as usize };
    QuantFactors {
        y1_dc: DC_QUANT[index(header.y_dc_delta)] as i32,
        y1_ac: AC_QUANT[q as usize] as i32,
        y2_dc: 2 * DC_QUANT[index(header.y2_dc_delta)] as i32,
        y2_ac: ((AC_QUANT[index(header.y2_ac_delta)] as i32 * 155) / 100).max(8),
        uv_dc: (DC_QUANT[index(header.uv_dc_delta)] as i32).min(132),
        uv_ac: AC_QUANT[index(header.uv_ac_delta)] as i32,
    }
}

/// Computes the effective VP8 loop-filter level for a macroblock after segmentation and mode adjustments.
fn macroblock_filter_level(
    filter: &FilterHeader,
    segmentation: &Segmentation,
    segment: usize,
    y_mode: u8,
) -> u8 {
    let mut level = if segmentation.enabled {
        if segmentation.absolute { segmentation.filter[segment] } else { filter.level + segmentation.filter[segment] }
    } else {
        filter.level
    };
    if filter.adjustments {
        // Key frames always use the intra/current-frame reference delta.
        level += filter.ref_delta[0];
        if y_mode == B_PRED {
            level += filter.mode_delta[0];
        }
    }
    level.clamp(0, 63) as u8
}

/// Applies the inverse Walsh-Hadamard transform used for VP8 luma DC coefficients.
fn inverse_wht(input: &[i32; 16], eob: u8) -> [i32; 16] {
    if eob == 0 {
        return [0; 16];
    }
    if eob == 1 {
        return [(input[0] + 3) >> 3; 16];
    }
    let mut tmp = [0i32; 16];
    let mut out = [0i32; 16];
    for i in 0..4 {
        let a = input[i] + input[12 + i];
        let b = input[4 + i] + input[8 + i];
        let c = input[4 + i] - input[8 + i];
        let d = input[i] - input[12 + i];
        tmp[i] = a + b;
        tmp[4 + i] = c + d;
        tmp[8 + i] = a - b;
        tmp[12 + i] = d - c;
    }
    for r in 0..4 {
        let o = r * 4;
        let a = tmp[o] + tmp[o + 3];
        let b = tmp[o + 1] + tmp[o + 2];
        let c = tmp[o + 1] - tmp[o + 2];
        let d = tmp[o] - tmp[o + 3];
        out[o] = (a + b + 3) >> 3;
        out[o + 1] = (c + d + 3) >> 3;
        out[o + 2] = (a - b + 3) >> 3;
        out[o + 3] = (d - c + 3) >> 3;
    }
    out
}

/// Applies the inverse integer transform used to reconstruct a VP8 residual block.
fn inverse_dct(input: &[i32; 16]) -> [i32; 16] {
    let mut tmp = [0i32; 16];
    let mut out = [0i32; 16];
    for i in 0..4 {
        let a = input[i] + input[8 + i];
        let b = input[i] - input[8 + i];
        let c = mul_shift(input[4 + i], 35468) - (input[12 + i] + mul_shift(input[12 + i], 20091));
        let d = input[4 + i] + mul_shift(input[4 + i], 20091) + mul_shift(input[12 + i], 35468);
        tmp[i] = a + d;
        tmp[4 + i] = b + c;
        tmp[8 + i] = b - c;
        tmp[12 + i] = a - d;
    }
    for r in 0..4 {
        let o = r * 4;
        let a = tmp[o] + tmp[o + 2];
        let b = tmp[o] - tmp[o + 2];
        let c = mul_shift(tmp[o + 1], 35468) - (tmp[o + 3] + mul_shift(tmp[o + 3], 20091));
        let d = tmp[o + 1] + mul_shift(tmp[o + 1], 20091) + mul_shift(tmp[o + 3], 35468);
        out[o] = (a + d + 4) >> 3;
        out[o + 1] = (b + c + 4) >> 3;
        out[o + 2] = (b - c + 4) >> 3;
        out[o + 3] = (a - d + 4) >> 3;
    }
    out
}

#[inline(always)]
/// Multiplies two integers and applies the fixed-point shift required by the VP8 transform.
fn mul_shift(value: i32, constant: i32) -> i32 {
    (((value as i64) * (constant as i64)) >> 16) as i32
}

#[inline(always)]
/// Clamps the supplied value to the valid numeric range used by the decoder.
fn clamp_pixel(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[inline(always)]
/// Adds a decoded residual block to a prediction block while clamping reconstructed samples to 8 bits.
fn add_residue(
    plane: &mut [u8], stride: usize, x: usize, y: usize,
    coeffs: &[i32; 16], eob: u8,
) {
    if eob == 0 {
        return;
    }

    let ptr = plane.as_mut_ptr();
    if eob == 1 {
        let dc = (coeffs[0] + 4) >> 3;
        if dc == 0 {
            return;
        }
        unsafe {
            for row in 0..4 {
                let row_ptr = ptr.add((y + row) * stride + x);
                *row_ptr.add(0) = clamp_pixel(*row_ptr.add(0) as i32 + dc);
                *row_ptr.add(1) = clamp_pixel(*row_ptr.add(1) as i32 + dc);
                *row_ptr.add(2) = clamp_pixel(*row_ptr.add(2) as i32 + dc);
                *row_ptr.add(3) = clamp_pixel(*row_ptr.add(3) as i32 + dc);
            }
        }
        return;
    }

    let residue = inverse_dct(coeffs);
    unsafe {
        for row in 0..4 {
            let row_ptr = ptr.add((y + row) * stride + x);
            let r = row * 4;
            *row_ptr.add(0) = clamp_pixel(*row_ptr.add(0) as i32 + residue[r]);
            *row_ptr.add(1) = clamp_pixel(*row_ptr.add(1) as i32 + residue[r + 1]);
            *row_ptr.add(2) = clamp_pixel(*row_ptr.add(2) as i32 + residue[r + 2]);
            *row_ptr.add(3) = clamp_pixel(*row_ptr.add(3) as i32 + residue[r + 3]);
        }
    }
}



#[inline]
/// Builds a predicted VP8 block from neighboring samples and the selected intra-prediction mode.
fn predict_block(plane: &mut [u8], stride: usize, x: usize, y: usize, size: usize, mode: u8) {
    debug_assert!(size == 8 || size == 16);
    let shift = size.trailing_zeros();

    match mode {
        DC_PRED => {
            let value = if x == 0 && y == 0 {
                128
            } else if y == 0 {
                let mut sum = 0usize;
                for r in 0..size {
                    sum += plane[(y + r) * stride + x - 1] as usize;
                }
                ((sum + size / 2) >> shift) as u8
            } else if x == 0 {
                let top = &plane[(y - 1) * stride + x..(y - 1) * stride + x + size];
                let sum = top.iter().map(|&v| v as usize).sum::<usize>();
                ((sum + size / 2) >> shift) as u8
            } else {
                let top = &plane[(y - 1) * stride + x..(y - 1) * stride + x + size];
                let mut sum = top.iter().map(|&v| v as usize).sum::<usize>();
                for r in 0..size {
                    sum += plane[(y + r) * stride + x - 1] as usize;
                }
                ((sum + size) >> (shift + 1)) as u8
            };
            for r in 0..size {
                let start = (y + r) * stride + x;
                plane[start..start + size].fill(value);
            }
        }
        V_PRED => {
            if y == 0 {
                for r in 0..size {
                    let start = (y + r) * stride + x;
                    plane[start..start + size].fill(127);
                }
            } else {
                let mut top = [0u8; 16];
                top[..size].copy_from_slice(&plane[(y - 1) * stride + x..(y - 1) * stride + x + size]);
                for r in 0..size {
                    let start = (y + r) * stride + x;
                    plane[start..start + size].copy_from_slice(&top[..size]);
                }
            }
        }
        H_PRED => {
            for r in 0..size {
                let v = if x == 0 { 129 } else { plane[(y + r) * stride + x - 1] };
                let start = (y + r) * stride + x;
                plane[start..start + size].fill(v);
            }
        }
        TM_PRED => {
            let p = if y == 0 { 127 } else if x == 0 { 129 } else { plane[(y - 1) * stride + x - 1] } as i32;
            let mut top = [127u8; 16];
            if y != 0 {
                top[..size].copy_from_slice(&plane[(y - 1) * stride + x..(y - 1) * stride + x + size]);
            }
            let ptr = plane.as_mut_ptr();
            for r in 0..size {
                let l = if x == 0 { 129 } else { plane[(y + r) * stride + x - 1] } as i32;
                unsafe {
                    let row = ptr.add((y + r) * stride + x);
                    for c in 0..size {
                        *row.add(c) = clamp_pixel(l + top[c] as i32 - p);
                    }
                }
            }
        }
        _ => {}
    }
}

#[inline]
/// Returns the rounded average of two 8-bit sample values.
fn avg2(a: u8, b: u8) -> u8 { ((a as u16 + b as u16 + 1) >> 1) as u8 }
#[inline]
/// Returns the weighted rounded average used by VP8 directional predictors.
fn avg3(a: u8, b: u8, c: u8) -> u8 { ((a as u16 + 2 * b as u16 + c as u16 + 2) >> 2) as u8 }

/// Generates one 4×4 VP8 luma prediction block for the selected intra mode.
fn predict_4x4(
    plane: &mut [u8],
    stride: usize,
    mb_width: usize,
    mb_x: usize,
    mb_y: usize,
    block_x: usize,
    x: usize,
    y: usize,
    mode: u8,
) {
    let p = if y == 0 { 127 } else if x == 0 { 129 } else { plane[(y - 1) * stride + x - 1] };
    let mut a = [127u8; 8];
    for i in 0..8 {
        if y != 0 {
            if block_x == 3 && i >= 4 {
                // For right-edge 4x4 blocks in the first macroblock row,
                // the top-right extension is the VP8 frame-edge value 127.
                // For later macroblock rows, read from the row immediately
                // above the macroblock (or replicate its rightmost sample).
                a[i] = if mb_y == 0 {
                    127
                } else if mb_x + 1 < mb_width {
                    plane[(mb_y * 16 - 1) * stride + mb_x * 16 + 16 + (i - 4)]
                } else {
                    plane[(mb_y * 16 - 1) * stride + mb_x * 16 + 15]
                };
            } else {
                a[i] = plane[(y - 1) * stride + x + i];
            }
        }
    }
    let mut l = [129u8; 4];
    if x != 0 {
        for i in 0..4 { l[i] = plane[(y + i) * stride + x - 1]; }
    }

    let mut b = [[0u8; 4]; 4];
    match mode {
        B_DC => {
            let sum = a[..4].iter().map(|&v| v as u16).sum::<u16>()
                + l.iter().map(|&v| v as u16).sum::<u16>();
            let v = ((sum + 4) >> 3) as u8;
            for row in &mut b { row.fill(v); }
        }
        B_TM => {
            for r in 0..4 { for c in 0..4 { b[r][c] = clamp_pixel(l[r] as i32 + a[c] as i32 - p as i32); } }
        }
        B_VE => {
            for c in 0..4 {
                let left = if c == 0 { p } else { a[c - 1] };
                let v = avg3(left, a[c], a[c + 1]);
                for r in 0..4 { b[r][c] = v; }
            }
        }
        B_HE => {
            for r in 0..4 {
                let up = if r == 0 { p } else { l[r - 1] };
                let down = if r == 3 { l[3] } else { l[r + 1] };
                let v = avg3(up, l[r], down);
                b[r].fill(v);
            }
        }
        B_LD => {
            let f = |i: usize| if i == 6 { avg3(a[6], a[7], a[7]) } else { avg3(a[i], a[i + 1], a[i + 2]) };
            b[0][0]=f(0); b[0][1]=f(1); b[1][0]=f(1); b[0][2]=f(2); b[1][1]=f(2); b[2][0]=f(2);
            b[0][3]=f(3); b[1][2]=f(3); b[2][1]=f(3); b[3][0]=f(3); b[1][3]=f(4); b[2][2]=f(4); b[3][1]=f(4);
            b[2][3]=f(5); b[3][2]=f(5); b[3][3]=f(6);
        }
        B_RD | B_VR | B_HD => {
            let e = [l[3], l[2], l[1], l[0], p, a[0], a[1], a[2], a[3]];
            if mode == B_RD {
                let f = |i: usize| avg3(e[i - 1], e[i], e[i + 1]);
                b[3][0]=f(1); b[3][1]=f(2); b[2][0]=f(2); b[3][2]=f(3); b[2][1]=f(3); b[1][0]=f(3);
                let v=f(4); b[3][3]=v;b[2][2]=v;b[1][1]=v;b[0][0]=v;
                let v=f(5); b[2][3]=v;b[1][2]=v;b[0][1]=v;
                let v=f(6); b[1][3]=v;b[0][2]=v; b[0][3]=f(7);
            } else if mode == B_VR {
                let f = |i: usize| avg3(e[i - 1], e[i], e[i + 1]);
                let h = |i: usize| avg2(e[i], e[i + 1]);
                b[3][0]=f(2);b[2][0]=f(3);let v=f(4);b[3][1]=v;b[1][0]=v;let v=h(4);b[2][1]=v;b[0][0]=v;
                let v=f(5);b[3][2]=v;b[1][1]=v;let v=h(5);b[2][2]=v;b[0][1]=v;
                let v=f(6);b[3][3]=v;b[1][2]=v;let v=h(6);b[2][3]=v;b[0][2]=v;b[1][3]=f(7);b[0][3]=h(7);
            } else {
                let f = |i: usize| avg3(e[i - 1], e[i], e[i + 1]);
                let h = |i: usize| avg2(e[i], e[i + 1]);
                b[3][0]=h(0);b[3][1]=f(1);let v=h(1);b[2][0]=v;b[3][2]=v;let v=f(2);b[2][1]=v;b[3][3]=v;
                let v=h(2);b[2][2]=v;b[1][0]=v;let v=f(3);b[2][3]=v;b[1][1]=v;
                let v=h(3);b[1][2]=v;b[0][0]=v;let v=f(4);b[1][3]=v;b[0][1]=v;b[0][2]=f(5);b[0][3]=f(6);
            }
        }
        B_VL => {
            b[0][0]=avg2(a[0],a[1]);b[1][0]=avg3(a[0],a[1],a[2]);let v=avg2(a[1],a[2]);b[2][0]=v;b[0][1]=v;
            let v=avg3(a[1],a[2],a[3]);b[1][1]=v;b[3][0]=v;let v=avg2(a[2],a[3]);b[2][1]=v;b[0][2]=v;
            let v=avg3(a[2],a[3],a[4]);b[3][1]=v;b[1][2]=v;let v=avg2(a[3],a[4]);b[2][2]=v;b[0][3]=v;
            let v=avg3(a[3],a[4],a[5]);b[3][2]=v;b[1][3]=v;b[2][3]=avg3(a[4],a[5],a[6]);b[3][3]=avg3(a[5],a[6],a[7]);
        }
        B_HU => {
            b[0][0]=avg2(l[0],l[1]);b[0][1]=avg3(l[0],l[1],l[2]);let v=avg2(l[1],l[2]);b[0][2]=v;b[1][0]=v;
            let v=avg3(l[1],l[2],l[3]);b[0][3]=v;b[1][1]=v;let v=avg2(l[2],l[3]);b[1][2]=v;b[2][0]=v;
            let v=avg3(l[2],l[3],l[3]);b[1][3]=v;b[2][1]=v;b[2][2]=l[3];b[2][3]=l[3];b[3].fill(l[3]);
        }
        _ => {}
    }
    for r in 0..4 {
        let dst = (y + r) * stride + x;
        plane[dst..dst + 4].copy_from_slice(&b[r]);
    }
}

// VP8 loop filtering. Filtering is performed after reconstruction so intra
// prediction always observes unfiltered neighboring samples.
/// Applies loop filter to the supplied decoding or rendering state.
fn apply_loop_filter(
    y: &mut [u8], u: &mut [u8], v: &mut [u8],
    y_stride: usize, uv_stride: usize,
    mb_width: usize, mb_height: usize,
    macroblocks: &[Macroblock], simple: bool, sharpness: u8,
) {
    for my in 0..mb_height {
        for mx in 0..mb_width {
            let mb = &macroblocks[my * mb_width + mx];
            let level = mb.filter_level as i32;
            if level == 0 { continue; }
            let mut interior = level;
            if sharpness > 0 {
                interior >>= if sharpness > 4 { 2 } else { 1 };
                interior = interior.min(9 - sharpness as i32);
            }
            interior = interior.max(1);
            let mb_limit = 2 * (level + 2) + interior;
            let sub_limit = 2 * level + interior;
            let hev = if level >= 40 { 2 } else if level >= 15 { 1 } else { 0 };

            let yx = mx * 16;
            let yy = my * 16;
            if mx > 0 { filter_vertical(y, y_stride, yx, yy, 16, mb_limit, interior, hev, true, simple); }
            if my > 0 { filter_horizontal(y, y_stride, yx, yy, 16, mb_limit, interior, hev, true, simple); }
            for off in [4usize, 8, 12] {
                filter_vertical(y, y_stride, yx + off, yy, 16, sub_limit, interior, hev, false, simple);
                filter_horizontal(y, y_stride, yx, yy + off, 16, sub_limit, interior, hev, false, simple);
            }

            if !simple {
                let ux = mx * 8;
                let uy = my * 8;
                if mx > 0 {
                    filter_vertical(u, uv_stride, ux, uy, 8, mb_limit, interior, hev, true, false);
                    filter_vertical(v, uv_stride, ux, uy, 8, mb_limit, interior, hev, true, false);
                }
                if my > 0 {
                    filter_horizontal(u, uv_stride, ux, uy, 8, mb_limit, interior, hev, true, false);
                    filter_horizontal(v, uv_stride, ux, uy, 8, mb_limit, interior, hev, true, false);
                }
                filter_vertical(u, uv_stride, ux + 4, uy, 8, sub_limit, interior, hev, false, false);
                filter_vertical(v, uv_stride, ux + 4, uy, 8, sub_limit, interior, hev, false, false);
                filter_horizontal(u, uv_stride, ux, uy + 4, 8, sub_limit, interior, hev, false, false);
                filter_horizontal(v, uv_stride, ux, uy + 4, 8, sub_limit, interior, hev, false, false);
            }
        }
    }
}

#[inline]
/// Applies the VP8 loop filter across a vertical block boundary.
fn filter_vertical(
    plane: &mut [u8], stride: usize, x: usize, y: usize, length: usize,
    edge_limit: i32, interior: i32, hev: i32, macro_edge: bool, simple: bool,
) {
    let ptr = plane.as_mut_ptr();
    for r in 0..length {
        unsafe {
            filter_one_edge_ptr(
                ptr.add((y + r) * stride + x), 1,
                edge_limit, interior, hev, macro_edge, simple,
            );
        }
    }
}

#[inline]
/// Applies the VP8 loop filter across a horizontal block boundary.
fn filter_horizontal(
    plane: &mut [u8], stride: usize, x: usize, y: usize, length: usize,
    edge_limit: i32, interior: i32, hev: i32, macro_edge: bool, simple: bool,
) {
    let ptr = plane.as_mut_ptr();
    for c in 0..length {
        unsafe {
            filter_one_edge_ptr(
                ptr.add(y * stride + x + c), stride,
                edge_limit, interior, hev, macro_edge, simple,
            );
        }
    }
}

#[inline(always)]
/// Converts a signed intermediate value to the unsigned sample representation used by the filter.
fn unsigned(v: i32) -> u8 {
    (v + 128).clamp(0, 255) as u8
}

#[inline(always)]
/// Clamps a signed intermediate value to the range accepted by the VP8 loop-filter equations.
fn sclip(v: i32) -> i32 {
    v.clamp(-128, 127)
}

/// Filter one VP8 edge. Callers only pass edges with four valid samples on
/// each side, guaranteed by the macroblock-padded reconstruction planes.
#[inline(always)]
unsafe fn filter_one_edge_ptr(
    q0_ptr: *mut u8,
    step: usize,
    edge_limit: i32,
    interior: i32,
    hev_threshold: i32,
    macro_edge: bool,
    simple: bool,
) {
    unsafe {
        let step = step as isize;
        let p3p = q0_ptr.offset(-4 * step);
        let p2p = q0_ptr.offset(-3 * step);
        let p1p = q0_ptr.offset(-2 * step);
        let p0p = q0_ptr.offset(-step);
        let q1p = q0_ptr.offset(step);
        let q2p = q0_ptr.offset(2 * step);
        let q3p = q0_ptr.offset(3 * step);

    let p3 = *p3p as i32;
    let p2 = *p2p as i32;
    let p1 = *p1p as i32;
    let p0 = *p0p as i32;
    let q0 = *q0_ptr as i32;
    let q1 = *q1p as i32;
    let q2 = *q2p as i32;
    let q3 = *q3p as i32;

        if 2 * (p0 - q0).abs() + ((p1 - q1).abs() >> 1) > edge_limit {
            return;
        }

    let p1s = p1 - 128;
    let p0s = p0 - 128;
    let q0s = q0 - 128;
    let q1s = q1 - 128;

        if simple {
        let a = sclip(3 * (q0s - p0s) + sclip(p1s - q1s));
        let a1 = sclip(a + 4) >> 3;
        let a2 = sclip(a + 3) >> 3;
        *q0_ptr = unsigned(q0s - a1);
        *p0p = unsigned(p0s + a2);
            return;
        }

        if (p3 - p2).abs() > interior
        || (p2 - p1).abs() > interior
        || (p1 - p0).abs() > interior
        || (q1 - q0).abs() > interior
        || (q2 - q1).abs() > interior
        || (q3 - q2).abs() > interior
    {
            return;
        }

    let hev = (p1 - p0).abs() > hev_threshold || (q1 - q0).abs() > hev_threshold;
    let w = sclip((if hev { sclip(p1s - q1s) } else { 0 }) + 3 * (q0s - p0s));

        if macro_edge && !hev {
        let a0 = (27 * w + 63) >> 7;
        let a1 = (18 * w + 63) >> 7;
        let a2 = (9 * w + 63) >> 7;
        *p0p = unsigned(p0s + a0);
        *q0_ptr = unsigned(q0s - a0);
        *p1p = unsigned(p1s + a1);
        *q1p = unsigned(q1s - a1);
        *p2p = unsigned((p2 - 128) + a2);
        *q2p = unsigned((q2 - 128) - a2);
        } else {
        let q_adjust = sclip(w + 4) >> 3;
        let p_adjust = sclip(w + 3) >> 3;
        *q0_ptr = unsigned(q0s - q_adjust);
        *p0p = unsigned(p0s + p_adjust);
        if !hev {
            let a = (q_adjust + 1) >> 1;
            *q1p = unsigned(q1s - a);
            *p1p = unsigned(p1s + a);
        }
        }
    }
}

// Generated below from the fixed tables in the VP8 specification.
const DC_QUANT: [u16; 128] = [
    4, 5, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15, 16, 17, 17,
    18, 19, 20, 20, 21, 21, 22, 22, 23, 23, 24, 25, 25, 26, 27, 28,
    29, 30, 31, 32, 33, 34, 35, 36, 37, 37, 38, 39, 40, 41, 42, 43,
    44, 45, 46, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58,
    59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74,
    75, 76, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89,
    91, 93, 95, 96, 98, 100, 101, 102, 104, 106, 108, 110, 112, 114, 116, 118,
    122, 124, 126, 128, 130, 132, 134, 136, 138, 140, 143, 145, 148, 151, 154, 157,
];

const AC_QUANT: [u16; 128] = [
    4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51,
    52, 53, 54, 55, 56, 57, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76,
    78, 80, 82, 84, 86, 88, 90, 92, 94, 96, 98, 100, 102, 104, 106, 108,
    110, 112, 114, 116, 119, 122, 125, 128, 131, 134, 137, 140, 143, 146, 149, 152,
    155, 158, 161, 164, 167, 170, 173, 177, 181, 185, 189, 193, 197, 201, 205, 209,
    213, 217, 221, 225, 229, 234, 239, 245, 249, 254, 259, 264, 269, 274, 279, 284,
];

const DEFAULT_COEFF_PROBS: [u8; 1056] = [
    128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
    128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 253, 136, 254, 255, 228, 219, 128, 128, 128, 128, 128,
    189, 129, 242, 255, 227, 213, 255, 219, 128, 128, 128, 106, 126, 227, 252, 214, 209, 255, 255, 128, 128, 128,
    1, 98, 248, 255, 236, 226, 255, 255, 128, 128, 128, 181, 133, 238, 254, 221, 234, 255, 154, 128, 128, 128,
    78, 134, 202, 247, 198, 180, 255, 219, 128, 128, 128, 1, 185, 249, 255, 243, 255, 128, 128, 128, 128, 128,
    184, 150, 247, 255, 236, 224, 128, 128, 128, 128, 128, 77, 110, 216, 255, 236, 230, 128, 128, 128, 128, 128,
    1, 101, 251, 255, 241, 255, 128, 128, 128, 128, 128, 170, 139, 241, 252, 236, 209, 255, 255, 128, 128, 128,
    37, 116, 196, 243, 228, 255, 255, 255, 128, 128, 128, 1, 204, 254, 255, 245, 255, 128, 128, 128, 128, 128,
    207, 160, 250, 255, 238, 128, 128, 128, 128, 128, 128, 102, 103, 231, 255, 211, 171, 128, 128, 128, 128, 128,
    1, 152, 252, 255, 240, 255, 128, 128, 128, 128, 128, 177, 135, 243, 255, 234, 225, 128, 128, 128, 128, 128,
    80, 129, 211, 255, 194, 224, 128, 128, 128, 128, 128, 1, 1, 255, 128, 128, 128, 128, 128, 128, 128, 128,
    246, 1, 255, 128, 128, 128, 128, 128, 128, 128, 128, 255, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
    198, 35, 237, 223, 193, 187, 162, 160, 145, 155, 62, 131, 45, 198, 221, 172, 176, 220, 157, 252, 221, 1,
    68, 47, 146, 208, 149, 167, 221, 162, 255, 223, 128, 1, 149, 241, 255, 221, 224, 255, 255, 128, 128, 128,
    184, 141, 234, 253, 222, 220, 255, 199, 128, 128, 128, 81, 99, 181, 242, 176, 190, 249, 202, 255, 255, 128,
    1, 129, 232, 253, 214, 197, 242, 196, 255, 255, 128, 99, 121, 210, 250, 201, 198, 255, 202, 128, 128, 128,
    23, 91, 163, 242, 170, 187, 247, 210, 255, 255, 128, 1, 200, 246, 255, 234, 255, 128, 128, 128, 128, 128,
    109, 178, 241, 255, 231, 245, 255, 255, 128, 128, 128, 44, 130, 201, 253, 205, 192, 255, 255, 128, 128, 128,
    1, 132, 239, 251, 219, 209, 255, 165, 128, 128, 128, 94, 136, 225, 251, 218, 190, 255, 255, 128, 128, 128,
    22, 100, 174, 245, 186, 161, 255, 199, 128, 128, 128, 1, 182, 249, 255, 232, 235, 128, 128, 128, 128, 128,
    124, 143, 241, 255, 227, 234, 128, 128, 128, 128, 128, 35, 77, 181, 251, 193, 211, 255, 205, 128, 128, 128,
    1, 157, 247, 255, 236, 231, 255, 255, 128, 128, 128, 121, 141, 235, 255, 225, 227, 255, 255, 128, 128, 128,
    45, 99, 188, 251, 195, 217, 255, 224, 128, 128, 128, 1, 1, 251, 255, 213, 255, 128, 128, 128, 128, 128,
    203, 1, 248, 255, 255, 128, 128, 128, 128, 128, 128, 137, 1, 177, 255, 224, 255, 128, 128, 128, 128, 128,
    253, 9, 248, 251, 207, 208, 255, 192, 128, 128, 128, 175, 13, 224, 243, 193, 185, 249, 198, 255, 255, 128,
    73, 17, 171, 221, 161, 179, 236, 167, 255, 234, 128, 1, 95, 247, 253, 212, 183, 255, 255, 128, 128, 128,
    239, 90, 244, 250, 211, 209, 255, 255, 128, 128, 128, 155, 77, 195, 248, 188, 195, 255, 255, 128, 128, 128,
    1, 24, 239, 251, 218, 219, 255, 205, 128, 128, 128, 201, 51, 219, 255, 196, 186, 128, 128, 128, 128, 128,
    69, 46, 190, 239, 201, 218, 255, 228, 128, 128, 128, 1, 191, 251, 255, 255, 128, 128, 128, 128, 128, 128,
    223, 165, 249, 255, 213, 255, 128, 128, 128, 128, 128, 141, 124, 248, 255, 255, 128, 128, 128, 128, 128, 128,
    1, 16, 248, 255, 255, 128, 128, 128, 128, 128, 128, 190, 36, 230, 255, 236, 255, 128, 128, 128, 128, 128,
    149, 1, 255, 128, 128, 128, 128, 128, 128, 128, 128, 1, 226, 255, 128, 128, 128, 128, 128, 128, 128, 128,
    247, 192, 255, 128, 128, 128, 128, 128, 128, 128, 128, 240, 128, 255, 128, 128, 128, 128, 128, 128, 128, 128,
    1, 134, 252, 255, 255, 128, 128, 128, 128, 128, 128, 213, 62, 250, 255, 255, 128, 128, 128, 128, 128, 128,
    55, 93, 255, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
    128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
    202, 24, 213, 235, 186, 191, 220, 160, 240, 175, 255, 126, 38, 182, 232, 169, 184, 228, 174, 255, 187, 128,
    61, 46, 138, 219, 151, 178, 240, 170, 255, 216, 128, 1, 112, 230, 250, 199, 191, 247, 159, 255, 255, 128,
    166, 109, 228, 252, 211, 215, 255, 174, 128, 128, 128, 39, 77, 162, 232, 172, 180, 245, 178, 255, 255, 128,
    1, 52, 220, 246, 198, 199, 249, 220, 255, 255, 128, 124, 74, 191, 243, 183, 193, 250, 221, 255, 255, 128,
    24, 71, 130, 219, 154, 170, 243, 182, 255, 255, 128, 1, 182, 225, 249, 219, 240, 255, 224, 128, 128, 128,
    149, 150, 226, 252, 216, 205, 255, 171, 128, 128, 128, 28, 108, 170, 242, 183, 194, 254, 223, 255, 255, 128,
    1, 81, 230, 252, 204, 203, 255, 192, 128, 128, 128, 123, 102, 209, 247, 188, 196, 255, 233, 128, 128, 128,
    20, 95, 153, 243, 164, 173, 255, 203, 128, 128, 128, 1, 222, 248, 255, 216, 213, 128, 128, 128, 128, 128,
    168, 175, 246, 252, 235, 205, 255, 255, 128, 128, 128, 47, 116, 215, 255, 211, 212, 255, 255, 128, 128, 128,
    1, 121, 236, 253, 212, 214, 255, 255, 128, 128, 128, 141, 84, 213, 252, 201, 202, 255, 219, 128, 128, 128,
    42, 80, 160, 240, 162, 185, 255, 205, 128, 128, 128, 1, 1, 255, 128, 128, 128, 128, 128, 128, 128, 128,
    244, 1, 255, 128, 128, 128, 128, 128, 128, 128, 128, 238, 1, 255, 128, 128, 128, 128, 128, 128, 128, 128,
];

const COEFF_UPDATE_PROBS: [u8; 1056] = [
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 176, 246, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    223, 241, 252, 255, 255, 255, 255, 255, 255, 255, 255, 249, 253, 253, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 244, 252, 255, 255, 255, 255, 255, 255, 255, 255, 234, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    253, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 246, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    239, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255, 254, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 248, 254, 255, 255, 255, 255, 255, 255, 255, 255, 251, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    251, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255, 254, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 254, 253, 255, 254, 255, 255, 255, 255, 255, 255, 250, 255, 254, 255, 254, 255, 255, 255, 255, 255, 255,
    254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    217, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 225, 252, 241, 253, 255, 255, 254, 255, 255, 255, 255,
    234, 250, 241, 250, 253, 255, 253, 254, 255, 255, 255, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    223, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255, 238, 253, 254, 254, 255, 255, 255, 255, 255, 255, 255,
    255, 248, 254, 255, 255, 255, 255, 255, 255, 255, 255, 249, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    247, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255, 252, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    253, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 254, 253, 255, 255, 255, 255, 255, 255, 255, 255, 250, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    186, 251, 250, 255, 255, 255, 255, 255, 255, 255, 255, 234, 251, 244, 254, 255, 255, 255, 255, 255, 255, 255,
    251, 251, 243, 253, 254, 255, 254, 255, 255, 255, 255, 255, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    236, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255, 251, 253, 253, 254, 254, 255, 255, 255, 255, 255, 255,
    255, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255, 254, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    254, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    248, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 250, 254, 252, 254, 255, 255, 255, 255, 255, 255, 255,
    248, 254, 249, 253, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 255, 255, 255, 255, 255, 255, 255, 255,
    246, 253, 253, 255, 255, 255, 255, 255, 255, 255, 255, 252, 254, 251, 254, 254, 255, 255, 255, 255, 255, 255,
    255, 254, 252, 255, 255, 255, 255, 255, 255, 255, 255, 248, 254, 253, 255, 255, 255, 255, 255, 255, 255, 255,
    253, 255, 254, 254, 255, 255, 255, 255, 255, 255, 255, 255, 251, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    245, 251, 254, 255, 255, 255, 255, 255, 255, 255, 255, 253, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 251, 253, 255, 255, 255, 255, 255, 255, 255, 255, 252, 253, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 252, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    249, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 253, 255, 255, 255, 255, 255, 255, 255, 255, 250, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
];
const KF_BMODE_PROBS: [u8; 900] = [
    231, 120, 48, 89, 115, 113, 120, 152, 112, 152, 179, 64, 126, 170, 118, 46, 70, 95,
    175, 69, 143, 80, 85, 82, 72, 155, 103, 56, 58, 10, 171, 218, 189, 17, 13, 152,
    144, 71, 10, 38, 171, 213, 144, 34, 26, 114, 26, 17, 163, 44, 195, 21, 10, 173,
    121, 24, 80, 195, 26, 62, 44, 64, 85, 170, 46, 55, 19, 136, 160, 33, 206, 71,
    63, 20, 8, 114, 114, 208, 12, 9, 226, 81, 40, 11, 96, 182, 84, 29, 16, 36,
    134, 183, 89, 137, 98, 101, 106, 165, 148, 72, 187, 100, 130, 157, 111, 32, 75, 80,
    66, 102, 167, 99, 74, 62, 40, 234, 128, 41, 53, 9, 178, 241, 141, 26, 8, 107,
    104, 79, 12, 27, 217, 255, 87, 17, 7, 74, 43, 26, 146, 73, 166, 49, 23, 157,
    65, 38, 105, 160, 51, 52, 31, 115, 128, 87, 68, 71, 44, 114, 51, 15, 186, 23,
    47, 41, 14, 110, 182, 183, 21, 17, 194, 66, 45, 25, 102, 197, 189, 23, 18, 22,
    88, 88, 147, 150, 42, 46, 45, 196, 205, 43, 97, 183, 117, 85, 38, 35, 179, 61,
    39, 53, 200, 87, 26, 21, 43, 232, 171, 56, 34, 51, 104, 114, 102, 29, 93, 77,
    107, 54, 32, 26, 51, 1, 81, 43, 31, 39, 28, 85, 171, 58, 165, 90, 98, 64,
    34, 22, 116, 206, 23, 34, 43, 166, 73, 68, 25, 106, 22, 64, 171, 36, 225, 114,
    34, 19, 21, 102, 132, 188, 16, 76, 124, 62, 18, 78, 95, 85, 57, 50, 48, 51,
    193, 101, 35, 159, 215, 111, 89, 46, 111, 60, 148, 31, 172, 219, 228, 21, 18, 111,
    112, 113, 77, 85, 179, 255, 38, 120, 114, 40, 42, 1, 196, 245, 209, 10, 25, 109,
    100, 80, 8, 43, 154, 1, 51, 26, 71, 88, 43, 29, 140, 166, 213, 37, 43, 154,
    61, 63, 30, 155, 67, 45, 68, 1, 209, 142, 78, 78, 16, 255, 128, 34, 197, 171,
    41, 40, 5, 102, 211, 183, 4, 1, 221, 51, 50, 17, 168, 209, 192, 23, 25, 82,
    125, 98, 42, 88, 104, 85, 117, 175, 82, 95, 84, 53, 89, 128, 100, 113, 101, 45,
    75, 79, 123, 47, 51, 128, 81, 171, 1, 57, 17, 5, 71, 102, 57, 53, 41, 49,
    115, 21, 2, 10, 102, 255, 166, 23, 6, 38, 33, 13, 121, 57, 73, 26, 1, 85,
    41, 10, 67, 138, 77, 110, 90, 47, 114, 101, 29, 16, 10, 85, 128, 101, 196, 26,
    57, 18, 10, 102, 102, 213, 34, 20, 43, 117, 20, 15, 36, 163, 128, 68, 1, 26,
    138, 31, 36, 171, 27, 166, 38, 44, 229, 67, 87, 58, 169, 82, 115, 26, 59, 179,
    63, 59, 90, 180, 59, 166, 93, 73, 154, 40, 40, 21, 116, 143, 209, 34, 39, 175,
    57, 46, 22, 24, 128, 1, 54, 17, 37, 47, 15, 16, 183, 34, 223, 49, 45, 183,
    46, 17, 33, 183, 6, 98, 15, 32, 183, 65, 32, 73, 115, 28, 128, 23, 128, 205,
    40, 3, 9, 115, 51, 192, 18, 6, 223, 87, 37, 9, 115, 59, 77, 64, 21, 47,
    104, 55, 44, 218, 9, 54, 53, 130, 226, 64, 90, 70, 205, 40, 41, 23, 26, 57,
    54, 57, 112, 184, 5, 41, 38, 166, 213, 30, 34, 26, 133, 152, 116, 10, 32, 134,
    75, 32, 12, 51, 192, 255, 160, 43, 51, 39, 19, 53, 221, 26, 114, 32, 73, 255,
    31, 9, 65, 234, 2, 15, 1, 118, 73, 88, 31, 35, 67, 102, 85, 55, 186, 85,
    56, 21, 23, 111, 59, 205, 45, 37, 192, 55, 38, 70, 124, 73, 102, 1, 34, 98,
    102, 61, 71, 37, 34, 53, 31, 243, 192, 69, 60, 71, 38, 73, 119, 28, 222, 37,
    68, 45, 128, 34, 1, 47, 11, 245, 171, 62, 17, 19, 70, 146, 85, 55, 62, 70,
    75, 15, 9, 9, 64, 255, 184, 119, 16, 37, 43, 37, 154, 100, 163, 85, 160, 1,
    63, 9, 92, 136, 28, 64, 32, 201, 85, 86, 6, 28, 5, 64, 255, 25, 248, 1,
    56, 8, 17, 132, 137, 255, 55, 116, 128, 58, 15, 20, 82, 135, 57, 26, 121, 40,
    164, 50, 31, 137, 154, 133, 25, 35, 218, 51, 103, 44, 131, 131, 123, 31, 6, 158,
    86, 40, 64, 135, 148, 224, 45, 183, 128, 22, 26, 17, 131, 240, 154, 14, 1, 209,
    83, 12, 13, 54, 192, 255, 68, 47, 28, 45, 16, 21, 91, 64, 222, 7, 1, 197,
    56, 21, 39, 155, 60, 138, 23, 102, 213, 85, 26, 85, 85, 128, 128, 32, 146, 171,
    18, 11, 7, 63, 144, 171, 4, 4, 246, 35, 27, 10, 146, 174, 171, 12, 26, 128,
    190, 80, 35, 99, 180, 80, 126, 54, 45, 85, 126, 47, 87, 176, 51, 41, 20, 32,
    101, 75, 128, 139, 118, 146, 116, 128, 85, 56, 41, 15, 176, 236, 85, 37, 9, 62,
    146, 36, 19, 30, 171, 255, 97, 27, 20, 71, 30, 17, 119, 118, 255, 17, 18, 138,
    101, 38, 60, 138, 55, 70, 43, 26, 142, 138, 45, 61, 62, 219, 1, 81, 188, 64,
    32, 41, 20, 117, 151, 142, 20, 21, 163, 112, 19, 12, 61, 195, 128, 48, 4, 24,
];

#[cfg(test)]
mod layout_tests {
    use super::Macroblock;

    #[test]
    fn macroblock_state_stays_compact() {
        assert!(core::mem::size_of::<Macroblock>() <= 24);
    }
}
