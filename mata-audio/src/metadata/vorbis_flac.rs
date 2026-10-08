use std::io::{Read, Seek, SeekFrom};
use crate::AudioError;
use super::{Metadata, MetadataKind};
use super::support::{preserve_position, checked_len_u64, MAX_METADATA_BLOCK_BYTES};
use super::vorbis_common::parse_comment_block;

/// Reads the FLAC VORBIS_COMMENT metadata block (type 4), if present, while
/// preserving the caller-visible stream position. Unrelated FLAC metadata
/// blocks are seek-skipped instead of retained.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    let start = reader.stream_position()?;
    let mut signature = [0u8; 4];
    reader.read_exact(&mut signature)?;
    if &signature != b"fLaC" { reader.seek(SeekFrom::Start(start))?; return Ok(None); }
    let mut first = true;
    loop {
        let mut header = [0u8; 4]; reader.read_exact(&mut header)?;
        let is_last = header[0] & 0x80 != 0;
        let block_type = header[0] & 0x7f;
        let length = ((header[1] as u64) << 16) | ((header[2] as u64) << 8) | header[3] as u64;
        if first && block_type != 0 { return Err(AudioError::InvalidFormat("The first FLAC metadata block must be STREAMINFO".into())); }
        first = false;
        if block_type == 4 {
            let len = checked_len_u64(length, MAX_METADATA_BLOCK_BYTES, "FLAC Vorbis-comment block")?;
            let mut data = vec![0u8; len]; reader.read_exact(&mut data)?;
            return Ok(Some(parse_comment_block(&data, MetadataKind::VorbisFlac)?));
        }
        let next = reader.stream_position()?.checked_add(length).ok_or_else(|| AudioError::InvalidFormat("FLAC metadata offset overflow".into()))?;
        reader.seek(SeekFrom::Start(next))?;
        if is_last { return Ok(None); }
    }
}

pub fn parse(data: &[u8]) -> Result<Metadata, AudioError> { parse_comment_block(data, MetadataKind::VorbisFlac) }
