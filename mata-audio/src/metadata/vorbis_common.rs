use crate::AudioError;
use super::{Metadata, MetadataKind};

/// Allocation-free iterator over raw Vorbis comment key/value byte slices.
pub(crate) struct CommentIter<'a> {
    data: &'a [u8],
    pos: usize,
    remaining: u32,
}

impl<'a> CommentIter<'a> {
    fn new(data: &'a [u8], pos: usize, remaining: u32) -> Self { Self { data, pos, remaining } }
}

impl<'a> Iterator for CommentIter<'a> {
    type Item = Result<(&'a [u8], &'a [u8]), AudioError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        let len = match read_u32(self.data, &mut self.pos) { Ok(v) => v as usize, Err(e) => return Some(Err(e)) };
        let bytes = match take(self.data, &mut self.pos, len) { Ok(v) => v, Err(e) => return Some(Err(e)) };
        if let Some(eq) = bytes.iter().position(|&b| b == b'=') {
            Some(Ok((&bytes[..eq], &bytes[eq + 1..])))
        } else {
            Some(Ok((b"COMMENT", bytes)))
        }
    }
}

pub(crate) fn parse_comment_block(data: &[u8], kind: MetadataKind) -> Result<Metadata, AudioError> {
    let mut pos = 0usize;
    let vendor_len = read_u32(data, &mut pos)? as usize;
    let vendor = take(data, &mut pos, vendor_len)?;
    let count = read_u32(data, &mut pos)?;
    if count as usize > data.len().saturating_div(4).saturating_add(1) {
        return Err(AudioError::InvalidFormat("Vorbis comment count is unreasonable".into()));
    }

    let mut out = Metadata::new(kind);
    out.push("VENDOR", String::from_utf8_lossy(vendor).into_owned());
    for field in CommentIter::new(data, pos, count) {
        let (key, value) = field?;
        out.push(String::from_utf8_lossy(key).into_owned(), String::from_utf8_lossy(value).into_owned());
    }
    Ok(out)
}

#[inline]
fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, AudioError> {
    let bytes = take(data, pos, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

#[inline]
fn take<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], AudioError> {
    let end = pos.checked_add(len).ok_or_else(|| AudioError::InvalidFormat("Vorbis comment offset overflow".into()))?;
    if end > data.len() { return Err(AudioError::TruncatedData); }
    let out = &data[*pos..end];
    *pos = end;
    Ok(out)
}
