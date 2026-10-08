use std::io::{Read, Seek, SeekFrom};
use crate::AudioError;
use super::{Metadata, MetadataKind};
use super::support::preserve_position;

/// Parse ID3v1 with exactly one bounded 128-byte tail read while preserving position.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    let end = reader.seek(SeekFrom::End(0))?;
    if end < 128 { return Ok(None); }
    reader.seek(SeekFrom::Start(end - 128))?;
    let mut tag = [0u8; 128];
    reader.read_exact(&mut tag)?;
    if &tag[0..3] != b"TAG" { return Ok(None); }
    let mut out = Metadata::new(MetadataKind::Id3V1);
    push_text(&mut out, "TITLE", &tag[3..33]);
    push_text(&mut out, "ARTIST", &tag[33..63]);
    push_text(&mut out, "ALBUM", &tag[63..93]);
    push_text(&mut out, "YEAR", &tag[93..97]);
    if tag[125] == 0 && tag[126] != 0 {
        push_text(&mut out, "COMMENT", &tag[97..125]);
        out.push("TRACK", tag[126].to_string());
    } else {
        push_text(&mut out, "COMMENT", &tag[97..127]);
    }
    out.push("GENRE", tag[127].to_string());
    Ok(Some(out))
}

fn push_text(out: &mut Metadata, key: &str, bytes: &[u8]) {
    let end = bytes.iter().rposition(|&b| b != 0 && b != b' ').map_or(0, |i| i + 1);
    if end == 0 { return; }
    let value: String = bytes[..end].iter().map(|&b| char::from(b)).collect();
    let value = value.trim();
    if !value.is_empty() { out.push(key, value); }
}
