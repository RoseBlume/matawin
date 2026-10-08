use std::io::{Read, Seek, SeekFrom};
use crate::AudioError;
use super::{Metadata, MetadataKind};
use super::support::{preserve_position, checked_len_u64, MAX_TEXT_VALUE_BYTES};

/// Reads common RIFF metadata while preserving the caller's stream position.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 12];
    if let Err(e) = reader.read_exact(&mut header) {
        if e.kind() == std::io::ErrorKind::UnexpectedEof { return Ok(None); }
        return Err(e.into());
    }
    if &header[0..4] != b"RIFF" && &header[0..4] != b"RF64" { return Ok(None); }
    let file_end = reader.seek(SeekFrom::End(0))?;
    let declared = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64;
    let riff_end = if declared == u32::MAX as u64 || &header[0..4] == b"RF64" { file_end } else { (8 + declared).min(file_end) };
    reader.seek(SeekFrom::Start(12))?;
    let mut out = Metadata::new(MetadataKind::Riff);
    let mut scratch = Vec::new();
    while reader.stream_position()? + 8 <= riff_end {
        let mut ch = [0u8; 8]; reader.read_exact(&mut ch)?;
        let id: [u8; 4] = ch[0..4].try_into().unwrap();
        let size = u32::from_le_bytes(ch[4..8].try_into().unwrap()) as u64;
        let start = reader.stream_position()?;
        let end = start.checked_add(size).ok_or_else(|| AudioError::InvalidFormat("RIFF chunk size overflow".into()))?;
        if end > riff_end { return Err(AudioError::TruncatedData); }
        match &id {
            b"LIST" if size >= 4 => {
                let mut list_type = [0u8; 4]; reader.read_exact(&mut list_type)?;
                if &list_type == b"INFO" { parse_info(reader, end, &mut out, &mut scratch)?; }
            }
            b"bext" => parse_bext(reader, size, &mut out, &mut scratch)?,
            b"iXML" => {
                let len = checked_len_u64(size, MAX_TEXT_VALUE_BYTES, "iXML chunk")?;
                scratch.resize(len, 0); reader.read_exact(&mut scratch)?;
                let value = String::from_utf8_lossy(&scratch).trim_matches('\0').to_string();
                if !value.is_empty() { out.push("IXML", value); }
            }
            _ => {}
        }
        reader.seek(SeekFrom::Start(end + (size & 1)))?;
    }
    Ok((!out.fields.is_empty()).then_some(out))
}

fn parse_info<R: Read + Seek>(reader: &mut R, list_end: u64, out: &mut Metadata, scratch: &mut Vec<u8>) -> Result<(), AudioError> {
    while reader.stream_position()? + 8 <= list_end {
        let mut h = [0u8; 8]; reader.read_exact(&mut h)?;
        let id = &h[0..4];
        let size = u32::from_le_bytes(h[4..8].try_into().unwrap()) as u64;
        let start = reader.stream_position()?;
        let end = start.checked_add(size).ok_or_else(|| AudioError::InvalidFormat("RIFF INFO size overflow".into()))?;
        if end > list_end { return Err(AudioError::TruncatedData); }
        let len = checked_len_u64(size, MAX_TEXT_VALUE_BYTES, "RIFF INFO value")?;
        scratch.resize(len, 0); reader.read_exact(scratch)?;
        let value = String::from_utf8_lossy(scratch).trim_matches('\0').trim().to_string();
        if !value.is_empty() { out.push(fourcc(id), value); }
        reader.seek(SeekFrom::Start(end + (size & 1)))?;
    }
    Ok(())
}

fn parse_bext<R: Read>(reader: &mut R, size: u64, out: &mut Metadata, scratch: &mut Vec<u8>) -> Result<(), AudioError> {
    let take = size.min(602) as usize;
    scratch.resize(take, 0); reader.read_exact(scratch)?;
    let fields = [("BEXT_DESCRIPTION", 0usize, 256usize), ("BEXT_ORIGINATOR", 256, 32), ("BEXT_ORIGINATOR_REFERENCE", 288, 32), ("BEXT_ORIGINATION_DATE", 320, 10), ("BEXT_ORIGINATION_TIME", 330, 8)];
    for (key, start, len) in fields {
        if start < scratch.len() {
            let end = (start + len).min(scratch.len());
            let s = String::from_utf8_lossy(&scratch[start..end]).trim_matches('\0').trim().to_string();
            if !s.is_empty() { out.push(key, s); }
        }
    }
    Ok(())
}

fn fourcc(id: &[u8]) -> String { id.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect() }
