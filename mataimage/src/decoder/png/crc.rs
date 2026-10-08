//! PNG CRC validation support.
//!
//! PNG uses the reflected IEEE CRC-32 polynomial (`0xEDB88320`). The scalar
//! implementation is slicing-by-8. On AArch64, when the optional SIMD feature
//! is enabled and the CPU exposes the architectural `crc` extension, the
//! matching IEEE CRC32 instructions are used. x86 SSE4.2 CRC instructions are
//! deliberately not used because they implement CRC32C (Castagnoli), which is a
//! different polynomial and therefore cannot validate PNG chunks.

// ============================================================================
// CRC32
// ============================================================================

/// Slicing-by-8 lookup tables for reflected IEEE CRC-32.
const CRC32_TABLES: [[u32; 256]; 8] = generate_crc32_tables();

/// Starts the running CRC state for one PNG chunk, including the four-byte type.
#[inline]
pub(crate) fn crc32_begin_chunk(chunk_type: &[u8; 4]) -> u32 {
    crc32_update_state(0xFFFF_FFFF, chunk_type)
}

/// Extends an un-finalized PNG CRC state with another payload fragment.
///
/// This is used by the streaming IDAT reader so checksum work happens while the
/// compressed bytes are already being copied into the inflater's refill buffer.
#[inline]
pub(crate) fn crc32_update_state(crc: u32, data: &[u8]) -> u32 {
    #[cfg(all(feature = "simd", target_arch = "aarch64"))]
    {
        if std::arch::is_aarch64_feature_detected!("crc") {
            // SAFETY: runtime feature detection above guarantees the instruction
            // set required by this function. ARMv8 CRC32 (without the `c`
            // suffix) uses the IEEE polynomial required by PNG.
            return unsafe { crc32_update_aarch64(crc, data) };
        }
    }

    crc32_update_slicing8(crc, data)
}

/// Finalizes an uncomplemented running PNG CRC state.
#[inline(always)]
pub(crate) const fn crc32_finish(state: u32) -> u32 {
    !state
}

/// Calculates the PNG CRC-32 for a chunk type and complete payload.
#[inline]
pub fn crc32_chunk(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    crc32_finish(crc32_update_state(crc32_begin_chunk(chunk_type), data))
}

/// Portable slicing-by-8 update used on every target without a matching hardware
/// IEEE CRC instruction.
#[inline]
fn crc32_update_slicing8(mut crc: u32, data: &[u8]) -> u32 {
    let mut i = 0usize;

    while i + 8 <= data.len() {
        let first = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        crc ^= first;
        crc = CRC32_TABLES[7][(crc & 0xFF) as usize]
            ^ CRC32_TABLES[6][((crc >> 8) & 0xFF) as usize]
            ^ CRC32_TABLES[5][((crc >> 16) & 0xFF) as usize]
            ^ CRC32_TABLES[4][((crc >> 24) & 0xFF) as usize]
            ^ CRC32_TABLES[3][data[i + 4] as usize]
            ^ CRC32_TABLES[2][data[i + 5] as usize]
            ^ CRC32_TABLES[1][data[i + 6] as usize]
            ^ CRC32_TABLES[0][data[i + 7] as usize];
        i += 8;
    }

    while i < data.len() {
        let index = ((crc ^ data[i] as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLES[0][index];
        i += 1;
    }

    crc
}

#[cfg(all(feature = "simd", target_arch = "aarch64"))]
#[target_feature(enable = "crc")]
unsafe fn crc32_update_aarch64(mut crc: u32, mut data: &[u8]) -> u32 {
    use core::arch::aarch64::{__crc32b, __crc32d};

    while data.len() >= 8 {
        // SAFETY: the loop condition proves eight readable bytes and
        // `read_unaligned` has no alignment requirement.
        let word = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const u64) };
        crc = __crc32d(crc, u64::from_le(word));
        data = &data[8..];
    }
    for &byte in data {
        crc = __crc32b(crc, byte);
    }
    crc
}

// ============================================================================
// TABLE GENERATION
// ============================================================================

/// Generates lookup tables used by the slicing-by-8 implementation.
const fn generate_crc32_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut i = 0usize;

    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0usize;
        while bit < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB_8320;
            } else {
                crc >>= 1;
            }
            bit += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }

    let mut table = 1usize;
    while table < 8 {
        let mut i = 0usize;
        while i < 256 {
            let previous = tables[table - 1][i];
            tables[table][i] = (previous >> 8) ^ tables[0][(previous & 0xFF) as usize];
            i += 1;
        }
        table += 1;
    }

    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_crc_matches_whole_chunk() {
        let kind = *b"IDAT";
        let data = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let expected = crc32_chunk(&kind, data);
        let mut state = crc32_begin_chunk(&kind);
        for chunk in data.chunks(3) {
            state = crc32_update_state(state, chunk);
        }
        assert_eq!(crc32_finish(state), expected);
    }

    #[test]
    fn ieee_crc_known_vector() {
        // Standard IEEE CRC-32 check value, represented as a synthetic chunk by
        // starting from the normal initial state without a PNG type prefix.
        let state = crc32_update_state(0xFFFF_FFFF, b"123456789");
        assert_eq!(crc32_finish(state), 0xCBF4_3926);
    }
}
