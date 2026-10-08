use std::io::Read;

use crate::AudioError;
use super::bit_reader::BitReader;

/// Decode one FLAC residual coding section directly into caller-owned storage.
/// Only `dst[..block_size - predictor_order]` is overwritten; no scratch is
/// cleared outside the active range.
pub(super) fn decode_residual_into<R: Read>(
    reader: &mut BitReader<R>,
    block_size: usize,
    predictor_order: usize,
    dst: &mut [i64],
) -> Result<usize, AudioError> {
    let method = reader.read_bits(2)? as u8;
    let parameter_bits = match method {
        0 => 4u8,
        1 => 5u8,
        _ => return Err(AudioError::InvalidFormat(format!("Reserved FLAC residual coding method {method}"))),
    };

    let partition_order = reader.read_bits(4)? as usize;
    let partitions = 1usize.checked_shl(partition_order as u32)
        .ok_or_else(|| AudioError::InvalidFormat("FLAC residual partition count overflow".into()))?;
    if block_size % partitions != 0 {
        return Err(AudioError::InvalidFormat(format!(
            "FLAC block size {block_size} is not divisible by {partitions} residual partitions"
        )));
    }

    let samples_per_partition = block_size / partitions;
    if samples_per_partition <= predictor_order && partition_order != 0 {
        return Err(AudioError::InvalidFormat("Invalid FLAC residual partition order".into()));
    }
    if partition_order == 0 && block_size < predictor_order {
        return Err(AudioError::InvalidFormat("Predictor order exceeds FLAC block size".into()));
    }

    let residual_count = block_size.checked_sub(predictor_order)
        .ok_or_else(|| AudioError::InvalidFormat("Predictor order exceeds block size".into()))?;
    if dst.len() < residual_count {
        return Err(AudioError::InvalidFormat("FLAC residual scratch is too small".into()));
    }

    let escape_parameter = (1u32 << parameter_bits) - 1;
    let mut write = 0usize;

    for partition in 0..partitions {
        let parameter = reader.read_bits(parameter_bits)? as u32;
        let sample_count = if partition == 0 {
            samples_per_partition.checked_sub(predictor_order)
                .ok_or_else(|| AudioError::InvalidFormat("Invalid first FLAC residual partition".into()))?
        } else {
            samples_per_partition
        };

        if parameter == escape_parameter {
            let raw_bits = reader.read_bits(5)? as u8;
            for slot in &mut dst[write..write + sample_count] {
                *slot = reader.read_signed(raw_bits)?;
            }
            write += sample_count;
            continue;
        }

        let rice_parameter = parameter as u8;
        if rice_parameter == 0 {
            // Rice parameter zero has no remainder bits and is common for
            // low-entropy signals. Decode only the unary quotient.
            for slot in &mut dst[write..write + sample_count] {
                let folded = reader.read_unary_zeros()?;
                if folded >= u32::MAX as u64 {
                    return Err(AudioError::InvalidFormat("FLAC residual exceeds 32-bit range".into()));
                }
                *slot = unfold_signed(folded);
            }
            write += sample_count;
            continue;
        }

        for slot in &mut dst[write..write + sample_count] {
            let quotient = reader.read_unary_zeros()?;
            let remainder = reader.read_bits(rice_parameter)?;
            let folded = quotient.checked_shl(rice_parameter as u32)
                .and_then(|value| value.checked_add(remainder))
                .ok_or_else(|| AudioError::InvalidFormat("FLAC Rice residual overflow".into()))?;
            if folded >= u32::MAX as u64 {
                return Err(AudioError::InvalidFormat("FLAC residual exceeds 32-bit range".into()));
            }
            *slot = unfold_signed(folded);
        }
        write += sample_count;
    }

    if write != residual_count {
        return Err(AudioError::InvalidFormat(format!(
            "Expected {residual_count} FLAC residuals, decoded {write}"
        )));
    }
    Ok(write)
}

#[inline]
fn unfold_signed(folded: u64) -> i64 {
    if folded & 1 == 0 {
        (folded >> 1) as i64
    } else {
        -(((folded >> 1) + 1) as i64)
    }
}
