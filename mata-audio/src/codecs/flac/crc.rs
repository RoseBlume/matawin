#[inline]
pub(super) fn crc8_update(mut crc: u8, byte: u8) -> u8 {
    crc ^= byte;
    for _ in 0..8 {
        crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
    }
    crc
}

#[inline]
pub(super) fn crc16_update(mut crc: u16, byte: u8) -> u16 {
    crc ^= (byte as u16) << 8;
    for _ in 0..8 {
        crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x8005 } else { crc << 1 };
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_crc_is_stable() {
        let data = b"FLAC frame header and payload";
        let c8 = data.iter().fold(0u8, |crc, &b| crc8_update(crc, b));
        let c16 = data.iter().fold(0u16, |crc, &b| crc16_update(crc, b));
        assert_eq!(c8, data.iter().fold(0u8, |crc, &b| crc8_update(crc, b)));
        assert_eq!(c16, data.iter().fold(0u16, |crc, &b| crc16_update(crc, b)));
    }
}
