use std::io::{Read, Seek};
use crate::{demuxers::ogg::OggDemuxer, AudioError};
use super::{Metadata, MetadataKind};
use super::support::preserve_position;
use super::vorbis_common::parse_comment_block;

/// Reads the Vorbis comment header while preserving caller-visible stream position.
pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    preserve_position(reader, read_at_current_position)
}

pub(crate) fn read_at_current_position<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>, AudioError> {
    let mut ogg = OggDemuxer::new(&mut *reader)?;
    let first = match ogg.next_packet()? { Some(p) => p, None => return Ok(None) };
    if first.data.len() < 7 || first.data[0] != 1 || &first.data[1..7] != b"vorbis" { return Ok(None); }
    let second = ogg.next_packet()?.ok_or(AudioError::TruncatedData)?;
    if second.data.len() < 8 || second.data[0] != 3 || &second.data[1..7] != b"vorbis" {
        return Err(AudioError::InvalidFormat("Missing Vorbis comment header".into()));
    }
    Ok(Some(parse_comment_block(&second.data[7..], MetadataKind::VorbisOgg)?))
}

pub fn parse(data: &[u8]) -> Result<Metadata, AudioError> { parse_comment_block(data, MetadataKind::VorbisOgg) }
