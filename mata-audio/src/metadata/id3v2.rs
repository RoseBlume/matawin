use std::io::{Read, Seek};
use crate::AudioError;
use super::{Metadata, MetadataKind, Picture};
use super::support::{preserve_position, read_exact_vec, MAX_METADATA_BLOCK_BYTES, MAX_PICTURE_BYTES};

/// Read an ID3v2 tag while preserving the caller's stream position.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    let mut header = [0u8; 10];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    if &header[0..3] != b"ID3" { return Ok(None); }
    let major = header[3];
    if !(2..=4).contains(&major) { return Err(AudioError::InvalidFormat(format!("Unsupported ID3v2.{major}"))); }
    let flags = header[5];
    let size = synchsafe4(header[6..10].try_into().unwrap())? as usize;
    let mut data = read_exact_vec(reader, size, MAX_METADATA_BLOCK_BYTES, "ID3v2 tag")?;
    if flags & 0x80 != 0 { deunsynchronize_in_place(&mut data); }

    let mut pos = if flags & 0x40 != 0 { extended_header_len(major, &data)? } else { 0 };
    let mut out = Metadata::new(MetadataKind::Id3V2);
    while pos < data.len() {
        let (id_bytes, frame_size, header_len) = if major == 2 {
            if pos + 6 > data.len() || data[pos..pos + 3].iter().all(|&b| b == 0) { break; }
            let size = ((data[pos + 3] as usize) << 16) | ((data[pos + 4] as usize) << 8) | data[pos + 5] as usize;
            (&data[pos..pos + 3], size, 6)
        } else {
            if pos + 10 > data.len() || data[pos..pos + 4].iter().all(|&b| b == 0) { break; }
            let size = if major == 4 {
                synchsafe4(data[pos + 4..pos + 8].try_into().unwrap())? as usize
            } else {
                u32::from_be_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize
            };
            (&data[pos..pos + 4], size, 10)
        };
        pos += header_len;
        if frame_size == 0 { continue; }
        let end = pos.checked_add(frame_size).ok_or_else(|| AudioError::InvalidFormat("ID3 frame size overflow".into()))?;
        if end > data.len() { return Err(AudioError::TruncatedData); }
        let id = std::str::from_utf8(id_bytes).unwrap_or("");
        parse_frame(&mut out, id, &data[pos..end], major)?;
        pos = end;
    }
    Ok(Some(out))
}

fn parse_frame(out: &mut Metadata, id: &str, payload: &[u8], major: u8) -> Result<(), AudioError> {
    if id.starts_with('T') && id != "TXXX" && id != "TXX" {
        if let Some((&enc, body)) = payload.split_first() {
            for v in decode_text_values(enc, body) { if !v.is_empty() { out.push(id, v); } }
        }
        return Ok(());
    }
    match id {
        "TXXX" | "TXX" => {
            if payload.is_empty() { return Ok(()); }
            let enc = payload[0];
            let (desc, value) = split_encoded(enc, &payload[1..]);
            let desc = decode_text(enc, desc);
            for v in decode_text_values(enc, value) {
                out.push(if desc.is_empty() { "TXXX" } else { desc.as_str() }, v);
            }
        }
        "COMM" | "COM" => {
            if payload.len() < 4 { return Ok(()); }
            let enc = payload[0];
            let language = String::from_utf8_lossy(&payload[1..4]);
            let (desc, text) = split_encoded(enc, &payload[4..]);
            out.push(format!("COMMENT:{language}:{}", decode_text(enc, desc)), decode_text(enc, text));
        }
        "APIC" => parse_apic(out, payload)?,
        "PIC" if major == 2 => parse_pic(out, payload)?,
        _ => {}
    }
    Ok(())
}

fn parse_apic(out: &mut Metadata, p: &[u8]) -> Result<(), AudioError> {
    if p.len() < 4 { return Ok(()); }
    let enc = p[0];
    let mime_end = p[1..].iter().position(|&b| b == 0).ok_or(AudioError::TruncatedData)? + 1;
    if mime_end + 1 >= p.len() { return Err(AudioError::TruncatedData); }
    let mime = String::from_utf8_lossy(&p[1..mime_end]).into_owned();
    let picture_type = p[mime_end + 1] as u32;
    let (desc, image) = split_encoded(enc, &p[mime_end + 2..]);
    if image.len() > MAX_PICTURE_BYTES { return Err(AudioError::InvalidFormat("ID3 picture exceeds bounded limit".into())); }
    out.pictures.push(Picture { mime_type: mime, picture_type, description: decode_text(enc, desc), data: image.to_vec() });
    Ok(())
}

fn parse_pic(out: &mut Metadata, p: &[u8]) -> Result<(), AudioError> {
    if p.len() < 5 { return Ok(()); }
    let enc = p[0];
    let mime = match &p[1..4] { b"PNG" => "image/png", b"JPG" => "image/jpeg", x => std::str::from_utf8(x).unwrap_or("application/octet-stream") }.to_string();
    let picture_type = p[4] as u32;
    let (desc, image) = split_encoded(enc, &p[5..]);
    if image.len() > MAX_PICTURE_BYTES { return Err(AudioError::InvalidFormat("ID3 picture exceeds bounded limit".into())); }
    out.pictures.push(Picture { mime_type: mime, picture_type, description: decode_text(enc, desc), data: image.to_vec() });
    Ok(())
}

fn extended_header_len(major: u8, data: &[u8]) -> Result<usize, AudioError> {
    if data.len() < 4 { return Err(AudioError::TruncatedData); }
    let n = if major == 4 {
        synchsafe4(data[..4].try_into().unwrap())? as usize
    } else {
        u32::from_be_bytes(data[..4].try_into().unwrap()) as usize + 4
    };
    if n > data.len() { return Err(AudioError::TruncatedData); }
    Ok(n)
}

#[inline]
fn synchsafe4(bytes: [u8; 4]) -> Result<u32, AudioError> {
    if (bytes[0] | bytes[1] | bytes[2] | bytes[3]) & 0x80 != 0 {
        return Err(AudioError::InvalidFormat("Invalid synchsafe integer".into()));
    }
    Ok(((bytes[0] as u32) << 21) | ((bytes[1] as u32) << 14) | ((bytes[2] as u32) << 7) | bytes[3] as u32)
}

fn deunsynchronize_in_place(data: &mut Vec<u8>) {
    let mut read = 0usize;
    let mut write = 0usize;
    while read < data.len() {
        let byte = data[read];
        data[write] = byte;
        write += 1;
        read += 1;
        if byte == 0xff && read < data.len() && data[read] == 0 { read += 1; }
    }
    data.truncate(write);
}

fn split_encoded(enc: u8, bytes: &[u8]) -> (&[u8], &[u8]) {
    if enc == 1 || enc == 2 {
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == 0 && bytes[i + 1] == 0 { return (&bytes[..i], &bytes[i + 2..]); }
            i += 2;
        }
    } else if let Some(i) = bytes.iter().position(|&b| b == 0) {
        return (&bytes[..i], &bytes[i + 1..]);
    }
    (bytes, &[])
}

fn decode_text_values(enc: u8, bytes: &[u8]) -> Vec<String> {
    let text = decode_text(enc, bytes);
    text.split('\0').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

fn decode_text(enc: u8, bytes: &[u8]) -> String {
    match enc {
        0 => bytes.iter().take_while(|&&b| b != 0).map(|&b| char::from(b)).collect::<String>().trim_matches('\0').to_string(),
        3 => String::from_utf8_lossy(bytes).trim_matches('\0').to_string(),
        1 | 2 => {
            let (le, start) = if enc == 1 && bytes.starts_with(&[0xff, 0xfe]) { (true, 2) }
                else if enc == 1 && bytes.starts_with(&[0xfe, 0xff]) { (false, 2) }
                else { (false, 0) };
            let mut units = Vec::with_capacity((bytes.len().saturating_sub(start)) / 2);
            for c in bytes[start..].chunks_exact(2) {
                units.push(if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) });
            }
            String::from_utf16_lossy(&units).trim_matches('\0').to_string()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}
