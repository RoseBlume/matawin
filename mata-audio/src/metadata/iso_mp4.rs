use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};
use crate::AudioError;
use super::{Metadata, MetadataKind, Picture};
use super::support::{preserve_position, checked_len_u64, MAX_PICTURE_BYTES, MAX_TEXT_VALUE_BYTES};

/// Reads iTunes-style metadata while preserving the caller's stream position.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    if end < 8 { return Ok(None); }
    let mut out = Metadata::new(MetadataKind::IsoMp4);
    scan_range(reader, 0, end, 0, &mut out)?;
    Ok((!out.fields.is_empty() || !out.pictures.is_empty()).then_some(out))
}

fn scan_range<R: Read + Seek>(reader: &mut R, start: u64, end: u64, depth: u8, out: &mut Metadata) -> Result<(), AudioError> {
    if depth > 12 { return Err(AudioError::InvalidFormat("ISO BMFF box nesting is too deep".into())); }
    let mut pos = start;
    while pos + 8 <= end {
        reader.seek(SeekFrom::Start(pos))?;
        let b = read_box_header(reader, end)?;
        if b.end <= pos { return Err(AudioError::InvalidFormat("ISO BMFF zero-length box loop".into())); }
        match &b.kind {
            b"moov" | b"udta" => scan_range(reader, b.data_start, b.end, depth + 1, out)?,
            b"meta" => {
                let child_start = b.data_start.checked_add(4).ok_or_else(|| AudioError::InvalidFormat("meta offset overflow".into()))?;
                if child_start <= b.end { scan_range(reader, child_start, b.end, depth + 1, out)?; }
            }
            b"ilst" => parse_ilst(reader, b.data_start, b.end, out)?,
            // `mdat`, track media and unknown atoms are skipped with one seek by
            // advancing `pos` to the already validated end; payload is never read.
            _ => {}
        }
        pos = b.end;
    }
    Ok(())
}

fn parse_ilst<R: Read + Seek>(reader: &mut R, start: u64, end: u64, out: &mut Metadata) -> Result<(), AudioError> {
    let mut pos = start;
    while pos + 8 <= end {
        reader.seek(SeekFrom::Start(pos))?;
        let item = read_box_header(reader, end)?;
        let key = metadata_key(item.kind);
        let mut child = item.data_start;
        while child + 8 <= item.end {
            reader.seek(SeekFrom::Start(child))?;
            let data_box = read_box_header(reader, item.end)?;
            if &data_box.kind == b"data" && data_box.end >= data_box.data_start + 8 {
                reader.seek(SeekFrom::Start(data_box.data_start))?;
                let mut prefix = [0u8; 8];
                reader.read_exact(&mut prefix)?;
                let data_type = u32::from_be_bytes(prefix[0..4].try_into().unwrap()) & 0x00ff_ffff;
                let payload_len_u64 = data_box.end - (data_box.data_start + 8);
                let maximum = if data_type == 13 || data_type == 14 { MAX_PICTURE_BYTES } else { MAX_TEXT_VALUE_BYTES };
                let payload_len = checked_len_u64(payload_len_u64, maximum, "MP4 metadata value")?;
                let mut payload = vec![0u8; payload_len];
                reader.read_exact(&mut payload)?;
                push_data(out, key.as_ref(), data_type, &payload);
            }
            child = data_box.end;
        }
        pos = item.end;
    }
    Ok(())
}

fn push_data(out: &mut Metadata, key: &str, data_type: u32, payload: &[u8]) {
    if key == "TRACK" || key == "DISC" {
        if payload.len() >= 6 {
            let n = u16::from_be_bytes([payload[2], payload[3]]);
            let total = u16::from_be_bytes([payload[4], payload[5]]);
            out.push(key, if total != 0 { format!("{n}/{total}") } else { n.to_string() });
        }
        return;
    }
    match data_type {
        13 => out.pictures.push(Picture { mime_type: "image/jpeg".into(), picture_type: 3, description: String::new(), data: payload.to_vec() }),
        14 => out.pictures.push(Picture { mime_type: "image/png".into(), picture_type: 3, description: String::new(), data: payload.to_vec() }),
        21 if payload.len() <= 8 => {
            let mut n = 0u64;
            for &b in payload { n = (n << 8) | b as u64; }
            out.push(key, n.to_string());
        }
        _ => {
            let text = String::from_utf8_lossy(payload);
            let text = text.trim_matches('\0');
            if !text.is_empty() { out.push(key, text); }
        }
    }
}

#[derive(Clone, Copy)]
struct BoxHeader { kind: [u8; 4], data_start: u64, end: u64 }

fn read_box_header<R: Read + Seek>(reader: &mut R, parent_end: u64) -> Result<BoxHeader, AudioError> {
    let start = reader.stream_position()?;
    let mut h = [0u8; 8];
    reader.read_exact(&mut h)?;
    let size32 = u32::from_be_bytes(h[0..4].try_into().unwrap()) as u64;
    let kind: [u8; 4] = h[4..8].try_into().unwrap();
    let (size, header_len) = if size32 == 1 {
        let mut x = [0u8; 8]; reader.read_exact(&mut x)?; (u64::from_be_bytes(x), 16u64)
    } else if size32 == 0 { (parent_end - start, 8) } else { (size32, 8) };
    if size < header_len { return Err(AudioError::InvalidFormat("ISO BMFF box is smaller than its header".into())); }
    let end = start.checked_add(size).ok_or_else(|| AudioError::InvalidFormat("ISO BMFF box size overflow".into()))?;
    if end > parent_end { return Err(AudioError::TruncatedData); }
    Ok(BoxHeader { kind, data_start: start + header_len, end })
}

fn metadata_key(kind: [u8; 4]) -> Cow<'static, str> {
    match &kind {
        b"\xA9nam" => Cow::Borrowed("TITLE"), b"\xA9ART" => Cow::Borrowed("ARTIST"), b"aART" => Cow::Borrowed("ALBUM_ARTIST"),
        b"\xA9alb" => Cow::Borrowed("ALBUM"), b"\xA9day" => Cow::Borrowed("DATE"), b"\xA9gen" => Cow::Borrowed("GENRE"),
        b"\xA9cmt" => Cow::Borrowed("COMMENT"), b"\xA9wrt" => Cow::Borrowed("COMPOSER"), b"trkn" => Cow::Borrowed("TRACK"),
        b"disk" => Cow::Borrowed("DISC"), b"cpil" => Cow::Borrowed("COMPILATION"), b"tmpo" => Cow::Borrowed("BPM"), b"covr" => Cow::Borrowed("COVER"),
        _ => Cow::Owned(kind.iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '?' }).collect()),
    }
}
