use crate::AudioError;

/// Decode the 80-bit IEEE extended value used by AIFF sample-rate fields.
/// Common integer sample rates avoid the generic exponent path entirely.
pub fn decode_extended_80(bytes: &[u8; 10]) -> Result<f64, AudioError> {
    match bytes {
        [0x40, 0x0B, 0xFA, 0x00, 0, 0, 0, 0, 0, 0] => return Ok(8_000.0),
        [0x40, 0x0C, 0xFA, 0x00, 0, 0, 0, 0, 0, 0] => return Ok(16_000.0),
        [0x40, 0x0D, 0xAC, 0x44, 0, 0, 0, 0, 0, 0] => return Ok(22_050.0),
        [0x40, 0x0D, 0xBB, 0x80, 0, 0, 0, 0, 0, 0] => return Ok(24_000.0),
        [0x40, 0x0D, 0xFA, 0x00, 0, 0, 0, 0, 0, 0] => return Ok(32_000.0),
        [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0] => return Ok(44_100.0),
        [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0] => return Ok(48_000.0),
        [0x40, 0x0F, 0xBB, 0x80, 0, 0, 0, 0, 0, 0] => return Ok(96_000.0),
        [0x40, 0x10, 0xBB, 0x80, 0, 0, 0, 0, 0, 0] => return Ok(192_000.0),
        _ => {}
    }

    let exponent_raw = u16::from_be_bytes([bytes[0], bytes[1]]);
    let negative = exponent_raw & 0x8000 != 0;
    let exponent = exponent_raw & 0x7FFF;
    let significand = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
        bytes[7], bytes[8], bytes[9],
    ]);

    if exponent == 0 && significand == 0 { return Ok(0.0); }
    if exponent == 0x7FFF {
        return Err(AudioError::InvalidFormat("AIFF sample rate is infinite or NaN".into()));
    }

    let unbiased = exponent as i32 - 16383;
    let power = unbiased - 63;
    let scale = pow2(power);
    let mut value = significand as f64 * scale;
    if negative { value = -value; }
    Ok(value)
}

#[inline]
fn pow2(exponent: i32) -> f64 {
    // AIFF sample rates are normally well inside the normalized f64 range.
    // Keep a correct fallback for unusual extended values.
    let biased = exponent + 1023;
    if (1..=2046).contains(&biased) {
        f64::from_bits((biased as u64) << 52)
    } else {
        2.0_f64.powi(exponent)
    }
}
