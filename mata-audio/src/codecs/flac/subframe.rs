use std::io::Read;

use crate::AudioError;
use super::{bit_reader::BitReader, residual::decode_residual_into};

pub(super) fn decode_subframe_into<R: Read>(
    reader: &mut BitReader<R>,
    output: &mut [i64],
    bits_per_sample: u8,
    residual_scratch: &mut [i64],
) -> Result<(), AudioError> {
    if reader.read_bit()? != 0 {
        return Err(AudioError::InvalidFormat("FLAC subframe padding bit must be zero".into()));
    }

    let subframe_type = reader.read_bits(6)? as u8;
    let has_wasted_bits = reader.read_bit()? != 0;
    let wasted_bits = if has_wasted_bits {
        reader.read_unary_zeros()?.checked_add(1)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC wasted-bit count overflow".into()))? as u8
    } else {
        0
    };

    if wasted_bits >= bits_per_sample {
        return Err(AudioError::InvalidFormat(format!(
            "FLAC subframe has {wasted_bits} wasted bits with only {bits_per_sample} bits per sample"
        )));
    }

    let effective_bps = bits_per_sample - wasted_bits;
    match subframe_type {
        0 => decode_constant(reader, output, effective_bps)?,
        1 => decode_verbatim(reader, output, effective_bps)?,
        8..=12 => decode_fixed(reader, output, effective_bps, (subframe_type - 8) as usize, residual_scratch)?,
        32..=63 => decode_lpc(reader, output, effective_bps, (subframe_type - 31) as usize, residual_scratch)?,
        _ => return Err(AudioError::InvalidFormat(format!("Reserved FLAC subframe type {subframe_type}"))),
    }

    if wasted_bits != 0 {
        for sample in output {
            *sample = sample.checked_shl(wasted_bits as u32)
                .ok_or_else(|| AudioError::InvalidFormat("FLAC sample overflow while restoring wasted bits".into()))?;
        }
    }
    Ok(())
}

#[inline]
fn decode_constant<R: Read>(reader: &mut BitReader<R>, output: &mut [i64], bps: u8) -> Result<(), AudioError> {
    let value = reader.read_signed(bps)?;
    output.fill(value);
    Ok(())
}

#[inline]
fn decode_verbatim<R: Read>(reader: &mut BitReader<R>, output: &mut [i64], bps: u8) -> Result<(), AudioError> {
    for sample in output { *sample = reader.read_signed(bps)?; }
    Ok(())
}

fn decode_fixed<R: Read>(
    reader: &mut BitReader<R>,
    output: &mut [i64],
    bps: u8,
    order: usize,
    residual_scratch: &mut [i64],
) -> Result<(), AudioError> {
    if order > 4 || order > output.len() {
        return Err(AudioError::InvalidFormat(format!("Invalid FLAC fixed predictor order {order}")));
    }
    for sample in &mut output[..order] { *sample = reader.read_signed(bps)?; }

    if order == 0 {
        // Predictor order zero is exactly the residual stream, so decode it
        // directly into the channel and eliminate a full residual copy.
        decode_residual_into(reader, output.len(), 0, output)?;
        return Ok(());
    }

    let residual_count = output.len() - order;
    decode_residual_into(reader, output.len(), order, &mut residual_scratch[..residual_count])?;
    match order {
        1 => restore_fixed_1(output, &residual_scratch[..residual_count]),
        2 => restore_fixed_2(output, &residual_scratch[..residual_count]),
        3 => restore_fixed_3(output, &residual_scratch[..residual_count]),
        4 => restore_fixed_4(output, &residual_scratch[..residual_count]),
        _ => unreachable!(),
    }
}

#[inline]
fn restore_fixed_1(output: &mut [i64], residuals: &[i64]) -> Result<(), AudioError> {
    for (n, &residual) in residuals.iter().enumerate() {
        let i = n + 1;
        output[i] = output[i - 1].checked_add(residual)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC fixed predictor overflow".into()))?;
    }
    Ok(())
}

#[inline]
fn restore_fixed_2(output: &mut [i64], residuals: &[i64]) -> Result<(), AudioError> {
    for (n, &residual) in residuals.iter().enumerate() {
        let i = n + 2;
        let prediction = output[i - 1] * 2 - output[i - 2];
        output[i] = prediction.checked_add(residual)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC fixed predictor overflow".into()))?;
    }
    Ok(())
}

#[inline]
fn restore_fixed_3(output: &mut [i64], residuals: &[i64]) -> Result<(), AudioError> {
    for (n, &residual) in residuals.iter().enumerate() {
        let i = n + 3;
        let prediction = output[i - 1] * 3 - output[i - 2] * 3 + output[i - 3];
        output[i] = prediction.checked_add(residual)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC fixed predictor overflow".into()))?;
    }
    Ok(())
}

#[inline]
fn restore_fixed_4(output: &mut [i64], residuals: &[i64]) -> Result<(), AudioError> {
    for (n, &residual) in residuals.iter().enumerate() {
        let i = n + 4;
        let prediction = output[i - 1] * 4 - output[i - 2] * 6 + output[i - 3] * 4 - output[i - 4];
        output[i] = prediction.checked_add(residual)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC fixed predictor overflow".into()))?;
    }
    Ok(())
}

fn decode_lpc<R: Read>(
    reader: &mut BitReader<R>,
    output: &mut [i64],
    bps: u8,
    order: usize,
    residual_scratch: &mut [i64],
) -> Result<(), AudioError> {
    if order == 0 || order > 32 || order > output.len() {
        return Err(AudioError::InvalidFormat(format!("Invalid FLAC LPC predictor order {order}")));
    }

    for sample in &mut output[..order] { *sample = reader.read_signed(bps)?; }

    let precision_code = reader.read_bits(4)? as u8;
    if precision_code == 15 {
        return Err(AudioError::InvalidFormat("FLAC LPC coefficient precision 15 is forbidden".into()));
    }
    let precision = precision_code + 1;
    let shift = reader.read_signed(5)?;
    if shift < 0 { return Err(AudioError::InvalidFormat(format!("Negative FLAC LPC shift {shift}"))); }
    let shift = shift as u32;

    // FLAC LPC order is bounded at 32, so coefficients never require a heap
    // allocation.
    let mut coefficients = [0i64; 32];
    for coefficient in &mut coefficients[..order] { *coefficient = reader.read_signed(precision)?; }

    let residual_count = output.len() - order;
    decode_residual_into(reader, output.len(), order, &mut residual_scratch[..residual_count])?;

    // With <=33-bit channel samples (side channels included), <=15-bit LPC
    // coefficients and order <=32, the multiply-accumulate fits in i64.
    for (n, &residual) in residual_scratch[..residual_count].iter().enumerate() {
        let i = n + order;
        let mut sum = 0i64;
        for coefficient_index in 0..order {
            let product = output[i - coefficient_index - 1]
                .checked_mul(coefficients[coefficient_index])
                .ok_or_else(|| AudioError::InvalidFormat("FLAC LPC multiply overflow".into()))?;
            sum = sum.checked_add(product)
                .ok_or_else(|| AudioError::InvalidFormat("FLAC LPC accumulation overflow".into()))?;
        }
        let prediction = sum >> shift;
        output[i] = prediction.checked_add(residual)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC LPC reconstruction overflow".into()))?;
    }
    Ok(())
}
