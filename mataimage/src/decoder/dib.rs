//! Shared helpers for Windows DIB-based image layouts used by BMP and ICO.

use crate::ImageError;

/// Computes the byte size of a scanline padded to a 32-bit DIB boundary.
///
/// Equivalent to `((width * bits_per_pixel + 31) / 32) * 4`, with checked
/// arithmetic throughout.
#[inline]
pub(crate) fn row_stride(width: u32, bits_per_pixel: u16) -> Result<usize, ImageError> {
    let bits = (width as u64)
        .checked_mul(bits_per_pixel as u64)
        .ok_or_else(|| ImageError::new("DIB row size overflow"))?;
    let dwords = bits
        .checked_add(31)
        .ok_or_else(|| ImageError::new("DIB row size overflow"))?
        / 32;
    let bytes = dwords
        .checked_mul(4)
        .ok_or_else(|| ImageError::new("DIB row size overflow"))?;

    usize::try_from(bytes)
        .map_err(|_| ImageError::new("DIB row is too large for this platform"))
}

/// Precomputed extraction parameters for one contiguous DIB bitfield channel.
///
/// The common 4/5/6/8-bit cases avoid division entirely; unusual contiguous
/// masks pay one integer scale per pixel without recomputing shifts/widths.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChannelMask {
    mask: u32,
    shift: u32,
    maximum: u32,
}

impl ChannelMask {
    #[inline]
    pub(crate) fn new(mask: u32) -> Self {
        if mask == 0 {
            return Self { mask: 0, shift: 0, maximum: 0 };
        }
        let shift = mask.trailing_zeros();
        let bits = mask.count_ones();
        let maximum = if bits == 32 { u32::MAX } else { (1u32 << bits) - 1 };
        Self { mask, shift, maximum }
    }

    #[inline(always)]
    pub(crate) fn extract(self, value: u32) -> u8 {
        if self.mask == 0 {
            return 0;
        }
        let raw = (value & self.mask) >> self.shift;
        match self.maximum {
            0xff => raw as u8,
            0x1f => ((raw << 3) | (raw >> 2)) as u8,
            0x3f => ((raw << 2) | (raw >> 4)) as u8,
            0x0f => ((raw << 4) | raw) as u8,
            maximum => (((raw as u64 * 255) + (maximum as u64 / 2)) / maximum as u64) as u8,
        }
    }
}

/// Returns whether all set bits in a DIB channel mask form one contiguous run.
#[inline]
pub(crate) fn mask_is_contiguous(mask: u32) -> bool {
    if mask == 0 {
        return true;
    }
    let shifted = mask >> mask.trailing_zeros();
    shifted & shifted.wrapping_add(1) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_stride_uses_checked_arithmetic() {
        assert_eq!(row_stride(1, 1).unwrap(), 4);
        assert_eq!(row_stride(32, 1).unwrap(), 4);
        assert_eq!(row_stride(33, 1).unwrap(), 8);
        assert_eq!(row_stride(3, 24).unwrap(), 12);
        // This must either produce a representable stride or a clean error on
        // narrow targets; it must never wrap or panic.
        let _ = row_stride(u32::MAX, u16::MAX);
    }
}
