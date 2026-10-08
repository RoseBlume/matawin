use std::{
    collections::VecDeque,
    io::{ErrorKind, Read, Seek, SeekFrom},
    ops::Range,
};

use crate::{AudioError, demuxers::{DemuxerSeek, SeekCapability}};

const MAX_OGG_PAGE_BODY: usize = 255 * 255;
const MAX_OGG_PACKET: usize = 16 * 1024 * 1024;
const MAX_PAGE_INDEX_POINTS: usize = 8192;

#[derive(Debug, Clone)]
pub struct OggPacket {
    pub data: Vec<u8>,
    pub granule_position: Option<u64>,
    pub end_of_stream: bool,
}

/// Borrowed Ogg packet view. Complete packets that live wholly on one page are
/// returned directly from the retained page body; a packet spanning pages is
/// returned from reusable continuation storage.
#[derive(Debug, Clone, Copy)]
pub struct OggPacketView<'a> {
    pub data: &'a [u8],
    pub granule_position: Option<u64>,
    pub end_of_stream: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct OggPageIndexPoint {
    pub byte_offset: u64,
    /// Granule position known before this page starts. This is the useful
    /// lower-bound timeline position when seeking to the page.
    pub start_granule: u64,
    pub end_granule: Option<u64>,
    pub sequence: u32,
    pub continuation: bool,
}

#[derive(Debug)]
enum QueuedPacket {
    Page {
        range: Range<usize>,
        granule_position: Option<u64>,
        end_of_stream: bool,
    },
    CompletedPartial {
        granule_position: Option<u64>,
        end_of_stream: bool,
    },
}

/// Streaming Ogg demuxer with retained page, lacing and continuation buffers.
pub struct OggDemuxer<R>
where
    R: Read + Seek,
{
    reader: R,
    stream_start: u64,
    serial: Option<u32>,
    sequence: Option<u32>,
    header: [u8; 27],
    lacing: Vec<u8>,
    body: Vec<u8>,
    partial: Vec<u8>,
    partial_spare: Vec<u8>,
    completed_partial: Vec<u8>,
    queue: VecDeque<QueuedPacket>,
    active: Option<QueuedPacket>,
    page_index: Vec<OggPageIndexPoint>,
    last_granule: u64,
    eos: bool,
    peak_buffer_bytes: usize,
}

impl<R> OggDemuxer<R>
where
    R: Read + Seek,
{
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        let stream_start = reader.stream_position()?;
        let mut out = Self {
            reader,
            stream_start,
            serial: None,
            sequence: None,
            header: [0; 27],
            lacing: Vec::with_capacity(255),
            body: Vec::with_capacity(32 * 1024),
            partial: Vec::with_capacity(8 * 1024),
            partial_spare: Vec::new(),
            completed_partial: Vec::new(),
            queue: VecDeque::with_capacity(32),
            active: None,
            page_index: Vec::with_capacity(256),
            last_granule: 0,
            eos: false,
            peak_buffer_bytes: 0,
        };
        out.note_buffer_high_water();
        Ok(out)
    }

    pub fn reset(&mut self) -> Result<(), AudioError> {
        self.reader.seek(SeekFrom::Start(self.stream_start))?;
        self.serial = None;
        self.sequence = None;
        self.partial.clear();
        self.partial_spare.clear();
        self.completed_partial.clear();
        self.queue.clear();
        self.active = None;
        self.last_granule = 0;
        self.eos = false;
        // Keep the learned page index: it remains valid for the same stream.
        Ok(())
    }

    #[inline]
    pub fn page_index(&self) -> &[OggPageIndexPoint] { &self.page_index }

    #[inline]
    pub fn stream_start(&self) -> u64 { self.stream_start }

    /// Retained packet/page working-set bytes, excluding the learned page index.
    pub(crate) fn retained_buffer_bytes(&self) -> usize {
        self.lacing.capacity()
            .saturating_add(self.body.capacity())
            .saturating_add(self.partial.capacity())
            .saturating_add(self.partial_spare.capacity())
            .saturating_add(self.completed_partial.capacity())
            .saturating_add(self.queue.capacity().saturating_mul(core::mem::size_of::<QueuedPacket>()))
    }

    #[inline]
    pub(crate) fn peak_buffer_bytes(&self) -> usize { self.peak_buffer_bytes.max(self.retained_buffer_bytes()) }

    #[inline]
    pub(crate) fn index_capacity_bytes(&self) -> usize {
        self.page_index.capacity().saturating_mul(core::mem::size_of::<OggPageIndexPoint>())
    }

    /// Explicit non-real-time reclamation. Normal reset/seek paths intentionally
    /// retain capacities so steady-state packet decoding remains allocation-free.
    pub(crate) fn trim_memory_non_realtime(&mut self) {
        self.lacing.shrink_to_fit();
        self.body.shrink_to_fit();
        self.partial.shrink_to_fit();
        self.partial_spare.shrink_to_fit();
        self.completed_partial.shrink_to_fit();
        self.queue.shrink_to_fit();
        self.page_index.shrink_to_fit();
    }

    #[inline]
    fn note_buffer_high_water(&mut self) {
        self.peak_buffer_bytes = self.peak_buffer_bytes.max(self.retained_buffer_bytes());
    }

    /// Seek to a known non-continuation page at/before `granule`. The returned
    /// value is the page's known starting granule. One page of preroll is used
    /// when available so codecs with overlap state can warm up before the
    /// target page.
    pub fn seek_granule(&mut self, granule: u64) -> Result<u64, AudioError> {
        let mut candidate = None;
        for (i, point) in self.page_index.iter().enumerate() {
            if !point.continuation && point.start_granule <= granule {
                candidate = Some(i);
            } else if point.start_granule > granule {
                break;
            }
        }
        let mut index = candidate.unwrap_or(0);
        if index > 0 { index -= 1; }
        let point = self.page_index.get(index).copied().unwrap_or(OggPageIndexPoint {
            byte_offset: self.stream_start,
            start_granule: 0,
            end_granule: None,
            sequence: 0,
            continuation: false,
        });

        self.reader.seek(SeekFrom::Start(point.byte_offset))?;
        self.sequence = None;
        self.partial.clear();
        self.completed_partial.clear();
        self.queue.clear();
        self.active = None;
        self.last_granule = point.start_granule;
        self.eos = false;
        Ok(point.start_granule)
    }

    /// Borrow the next packet. This is the zero-copy path for complete
    /// single-page packets and the allocation-reuse path for continued packets.
    pub fn next_packet_view(&mut self) -> Result<Option<OggPacketView<'_>>, AudioError> {
        self.recycle_active();
        loop {
            if let Some(next) = self.queue.pop_front() {
                self.active = Some(next);
                return Ok(Some(match self.active.as_ref().unwrap() {
                    QueuedPacket::Page { range, granule_position, end_of_stream } => OggPacketView {
                        data: &self.body[range.clone()],
                        granule_position: *granule_position,
                        end_of_stream: *end_of_stream,
                    },
                    QueuedPacket::CompletedPartial { granule_position, end_of_stream } => OggPacketView {
                        data: &self.completed_partial,
                        granule_position: *granule_position,
                        end_of_stream: *end_of_stream,
                    },
                }));
            }
            if self.eos { return Ok(None); }
            if !self.read_page()? { return Ok(None); }
        }
    }

    /// Owned compatibility API. Prefer `next_packet_view` inside decoders.
    pub fn next_packet(&mut self) -> Result<Option<OggPacket>, AudioError> {
        let Some(view) = self.next_packet_view()? else { return Ok(None); };
        Ok(Some(OggPacket {
            data: view.data.to_vec(),
            granule_position: view.granule_position,
            end_of_stream: view.end_of_stream,
        }))
    }

    fn recycle_active(&mut self) {
        if matches!(self.active, Some(QueuedPacket::CompletedPartial { .. })) {
            self.completed_partial.clear();
            if self.partial_spare.capacity() < self.completed_partial.capacity() {
                std::mem::swap(&mut self.completed_partial, &mut self.partial_spare);
            }
        }
        self.active = None;
        if self.partial.is_empty() && self.partial_spare.capacity() > self.partial.capacity() {
            std::mem::swap(&mut self.partial, &mut self.partial_spare);
            self.partial.clear();
        }
    }

    fn read_page(&mut self) -> Result<bool, AudioError> {
        debug_assert!(self.queue.is_empty());
        let page_start = self.reader.stream_position()?;
        let mut read = 0usize;
        while read < self.header.len() {
            match self.reader.read(&mut self.header[read..]) {
                Ok(0) if read == 0 => {
                    if !self.partial.is_empty() { return Err(AudioError::TruncatedData); }
                    return Ok(false);
                }
                Ok(0) => return Err(AudioError::TruncatedData),
                Ok(n) => read += n,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(AudioError::Io(error)),
            }
        }

        if &self.header[0..4] != b"OggS" {
            return Err(AudioError::InvalidHeader("Missing Ogg page capture pattern".into()));
        }
        if self.header[4] != 0 {
            return Err(AudioError::InvalidFormat(format!(
                "Unsupported Ogg stream structure version {}", self.header[4]
            )));
        }

        let flags = self.header[5];
        let continuation = flags & 0x01 != 0;
        let bos = flags & 0x02 != 0;
        let eos = flags & 0x04 != 0;
        let raw_granule = u64::from_le_bytes(self.header[6..14].try_into().unwrap());
        let serial = u32::from_le_bytes(self.header[14..18].try_into().unwrap());
        let sequence = u32::from_le_bytes(self.header[18..22].try_into().unwrap());
        let expected_crc = u32::from_le_bytes(self.header[22..26].try_into().unwrap());
        let segment_count = self.header[26] as usize;

        match self.serial {
            None => { let _ = bos; self.serial = Some(serial); }
            Some(current) if current != serial => {
                return Err(AudioError::InvalidFormat(
                    "Chained or multiplexed Ogg logical streams are not supported".into(),
                ));
            }
            _ => {}
        }
        if let Some(previous) = self.sequence {
            if sequence != previous.wrapping_add(1) {
                return Err(AudioError::InvalidFormat(format!(
                    "Ogg page sequence discontinuity: expected {}, found {}",
                    previous.wrapping_add(1), sequence
                )));
            }
        }
        self.sequence = Some(sequence);
        if continuation != !self.partial.is_empty() {
            return Err(AudioError::InvalidFormat("Invalid Ogg continued-packet flag".into()));
        }

        self.lacing.resize(segment_count, 0);
        self.reader.read_exact(&mut self.lacing)?;
        let body_len = self.lacing.iter().try_fold(0usize, |sum, &x| sum.checked_add(x as usize))
            .ok_or_else(|| AudioError::InvalidFormat("Ogg page body length overflow".into()))?;
        if body_len > MAX_OGG_PAGE_BODY {
            return Err(AudioError::InvalidFormat("Ogg page exceeds maximum legal body size".into()));
        }
        self.body.resize(body_len, 0);
        self.reader.read_exact(&mut self.body)?;
        self.note_buffer_high_water();

        let mut crc_header = self.header;
        crc_header[22..26].fill(0);
        let mut actual_crc = 0u32;
        actual_crc = ogg_crc32_update(actual_crc, &crc_header);
        actual_crc = ogg_crc32_update(actual_crc, &self.lacing);
        actual_crc = ogg_crc32_update(actual_crc, &self.body);
        if actual_crc != expected_crc {
            return Err(AudioError::ChecksumMismatch {
                kind: "Ogg page CRC-32", expected: expected_crc as u64, actual: actual_crc as u64,
            });
        }

        self.record_page_index(OggPageIndexPoint {
            byte_offset: page_start,
            start_granule: self.last_granule,
            end_granule: (raw_granule != u64::MAX).then_some(raw_granule),
            sequence,
            continuation,
        });

        let mut body_offset = 0usize;
        let mut completed_this_page = 0usize;
        let lacing_len = self.lacing.len();
        for segment_index in 0..lacing_len {
            let segment = self.lacing[segment_index];
            let len = segment as usize;
            let end = body_offset.checked_add(len).ok_or_else(|| AudioError::InvalidFormat("Ogg body offset overflow".into()))?;
            if end > self.body.len() { return Err(AudioError::TruncatedData); }

            if !self.partial.is_empty() {
                let new_len = self.partial.len().checked_add(len)
                    .ok_or_else(|| AudioError::InvalidFormat("Ogg packet size overflow".into()))?;
                if new_len > MAX_OGG_PACKET { return Err(AudioError::InvalidFormat("Ogg packet exceeds bounded size".into())); }
                self.partial.extend_from_slice(&self.body[body_offset..end]);
                self.note_buffer_high_water();
                if segment < 255 {
                    self.completed_partial.clear();
                    std::mem::swap(&mut self.completed_partial, &mut self.partial);
                    self.queue.push_back(QueuedPacket::CompletedPartial { granule_position: None, end_of_stream: false });
                    completed_this_page += 1;
                }
            } else if segment < 255 {
                // A packet entirely on this page can remain a direct body slice.
                self.queue.push_back(QueuedPacket::Page {
                    range: body_offset..end,
                    granule_position: None,
                    end_of_stream: false,
                });
                completed_this_page += 1;
            } else {
                self.partial.extend_from_slice(&self.body[body_offset..end]);
                self.note_buffer_high_water();
            }
            body_offset = end;
        }

        if eos && !self.partial.is_empty() { return Err(AudioError::TruncatedData); }

        if completed_this_page != 0 {
            if let Some(last) = self.queue.back_mut() {
                match last {
                    QueuedPacket::Page { granule_position, end_of_stream, .. }
                    | QueuedPacket::CompletedPartial { granule_position, end_of_stream } => {
                        if raw_granule != u64::MAX { *granule_position = Some(raw_granule); self.last_granule = raw_granule; }
                        if eos { *end_of_stream = true; }
                    }
                }
            }
        } else if raw_granule != u64::MAX {
            self.last_granule = raw_granule;
        }
        if eos { self.eos = true; }
        Ok(true)
    }

    fn record_page_index(&mut self, point: OggPageIndexPoint) {
        if self.page_index.last().is_some_and(|p| p.byte_offset == point.byte_offset) { return; }
        if self.page_index.len() >= MAX_PAGE_INDEX_POINTS {
            let mut write = 1usize;
            for read in (2..self.page_index.len()).step_by(2) {
                self.page_index[write] = self.page_index[read];
                write += 1;
            }
            self.page_index.truncate(write);
        }
        self.page_index.push(point);
    }
}

impl<R: Read + Seek> DemuxerSeek for OggDemuxer<R> {
    fn seek_capability(&self) -> SeekCapability { SeekCapability::IndexedApproximate }

    fn seek_time_seconds(&mut self, _seconds: f64) -> Result<(), AudioError> {
        Err(AudioError::InvalidFormat(
            "Ogg's generic demuxer seeks by granule; the codec must translate seconds to granules".into(),
        ))
    }

    fn position_seconds(&self) -> Option<f64> { None }
}

#[inline]
fn ogg_crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::ogg_crc32_update;

    #[test]
    fn incremental_crc_matches_concatenated_crc() {
        let parts: [&[u8]; 3] = [b"OggS", b"header", b"payload"];
        let mut combined = Vec::new();
        let mut incremental = 0u32;
        for part in parts {
            combined.extend_from_slice(part);
            incremental = ogg_crc32_update(incremental, part);
        }
        assert_eq!(incremental, ogg_crc32_update(0, &combined));
    }
}
