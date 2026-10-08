use std::io::{Read, Seek, SeekFrom};
use crate::AudioError;
use super::{Metadata, MetadataKind};
use super::support::{preserve_position, read_exact_vec, MAX_METADATA_BLOCK_BYTES};

const APE_HEADER_SIZE: u64 = 32;
const MAX_APE_ITEMS: usize = 1 << 20;

/// Read APEv1/APEv2 metadata without changing the caller-visible stream position.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    let end = reader.seek(SeekFrom::End(0))?;
    if end < APE_HEADER_SIZE { return Ok(None); }

    // Probe both legal footer locations from a single bounded tail read: the
    // physical EOF and immediately before an ID3v1 trailer.
    let tail_len = end.min(160) as usize;
    let tail_start = end - tail_len as u64;
    reader.seek(SeekFrom::Start(tail_start))?;
    let mut tail = [0u8; 160];
    reader.read_exact(&mut tail[160 - tail_len..])?;

    for trailer in [0u64, 128u64] {
        if end < trailer + APE_HEADER_SIZE { continue; }
        let footer_abs = end - trailer - APE_HEADER_SIZE;
        if footer_abs < tail_start { continue; }
        let offset = 160 - tail_len + (footer_abs - tail_start) as usize;
        let footer = &tail[offset..offset + 32];
        if &footer[0..8] != b"APETAGEX" { continue; }

        let version = u32::from_le_bytes(footer[8..12].try_into().unwrap());
        let size = u32::from_le_bytes(footer[12..16].try_into().unwrap()) as u64;
        let count = u32::from_le_bytes(footer[16..20].try_into().unwrap()) as usize;
        if version != 1000 && version != 2000 {
            return Err(AudioError::InvalidFormat(format!("Unsupported APE tag version {version}")));
        }
        if count > MAX_APE_ITEMS {
            return Err(AudioError::InvalidFormat("APE item count is unreasonable".into()));
        }
        if size < 32 || size > end - trailer || size as usize > MAX_METADATA_BLOCK_BYTES {
            return Err(AudioError::InvalidFormat("Invalid or oversized APE tag size".into()));
        }

        let start = end - trailer - size;
        reader.seek(SeekFrom::Start(start))?;
        let items_len = usize::try_from(size - 32)
            .map_err(|_| AudioError::InvalidFormat("APE tag too large".into()))?;
        let bytes = read_exact_vec(reader, items_len, MAX_METADATA_BLOCK_BYTES, "APE tag")?;
        let mut pos = if bytes.len() >= 32 && &bytes[0..8] == b"APETAGEX" { 32 } else { 0 };
        let mut out = Metadata::new(if version == 1000 { MetadataKind::ApeV1 } else { MetadataKind::ApeV2 });

        for _ in 0..count {
            if pos + 8 > bytes.len() { return Err(AudioError::TruncatedData); }
            let value_size = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            let flags = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            pos += 8;
            let key_end = bytes[pos..].iter().position(|&b| b == 0)
                .ok_or(AudioError::TruncatedData)? + pos;
            let key = String::from_utf8_lossy(&bytes[pos..key_end]);
            pos = key_end + 1;
            let end_value = pos.checked_add(value_size).ok_or_else(|| AudioError::InvalidFormat("APE value offset overflow".into()))?;
            if end_value > bytes.len() { return Err(AudioError::TruncatedData); }
            let value = &bytes[pos..end_value];
            pos = end_value;

            match (flags >> 1) & 0x3 {
                0 => {
                    for part in value.split(|&b| b == 0).filter(|p| !p.is_empty()) {
                        out.push(key.as_ref(), String::from_utf8_lossy(part).into_owned());
                    }
                }
                2 => out.push(key.as_ref(), String::from_utf8_lossy(value).into_owned()),
                _ => out.push(key.as_ref(), format!("<{} binary bytes>", value.len())),
            }
        }
        return Ok(Some(out));
    }
    Ok(None)
}
