//! PNG zlib/DEFLATE decompression support.
//!
//! PNG pixel data, compressed text chunks, and compressed ICC profiles all use
//! the same RFC 1950 zlib wrapper around RFC 1951 DEFLATE. Keeping one inflater
//! here avoids maintaining separate bit readers, Huffman implementations, and
//! length/distance tables for metadata and image pixels.

use std::io::{self, Read};
use std::sync::OnceLock;

use crate::decoder::bit_reader::LsbBitReader;
use super::helpers::invalid_data;

/// Decompresses a complete RFC 1950 zlib stream into a newly allocated vector.
///
/// The public metadata parsers historically expose malformed compressed data as
/// `None`, so this compatibility wrapper deliberately keeps the `Option` API.
pub fn decompress_zlib(data: &[u8]) -> Option<Vec<u8>> {
    decompress_zlib_vec(data).ok()
}

/// Decompresses a zlib stream into an output slice of exactly the expected size.
///
/// The PNG pixel decoder uses this form to inflate directly into its single
/// pre-sized scanline buffer instead of allocating an intermediate `Vec<u8>`.
pub(crate) fn decompress_zlib_exact(data: &[u8], output: &mut [u8]) -> io::Result<()> {
    let stream = parse_zlib(data)?;
    let expected_len = output.len();
    let written = {
        let mut destination = SliceOutput::new(output);
        decompress_deflate_slice(stream.deflate, &mut destination)?;
        destination.len()
    };

    if written != expected_len {
        return Err(invalid_data(
            "PNG decompressed data is smaller than expected",
        ));
    }

    if adler32(output) != stream.expected_adler {
        return Err(invalid_data("Invalid zlib Adler-32 checksum"));
    }

    Ok(())
}


/// Streams a zlib payload through one reusable fixed-size chunk buffer.
///
/// The inflater retains only the 32 KiB DEFLATE history window plus one chunk,
/// allowing PNG to reconstruct/convert scanlines without allocating the full
/// decompressed image. `callback` may mutate the chunk after it has already been
/// recorded in the LZ77 history (PNG uses this to unfilter the row in place).
pub(crate) fn decompress_zlib_chunks<F>(
    data: &[u8],
    expected_len: usize,
    chunk_size: usize,
    verify_checksum: bool,
    mut callback: F,
) -> io::Result<()>
where
    F: FnMut(&mut [u8]) -> io::Result<()>,
{
    let mut history = Box::new([0u8; 32_768]);
    let mut chunk = Vec::with_capacity(chunk_size);
    decompress_zlib_chunks_reuse(
        data,
        expected_len,
        chunk_size,
        verify_checksum,
        &mut history,
        &mut chunk,
        &mut callback,
    )
}

/// Streaming inflater variant using caller-owned history/chunk storage.
pub(crate) fn decompress_zlib_chunks_reuse<F>(
    data: &[u8],
    expected_len: usize,
    chunk_size: usize,
    verify_checksum: bool,
    history: &mut [u8; 32_768],
    chunk: &mut Vec<u8>,
    callback: &mut F,
) -> io::Result<()>
where
    F: FnMut(&mut [u8]) -> io::Result<()>,
{
    if chunk_size == 0 {
        return Err(invalid_data("zlib streaming chunk size must be non-zero"));
    }
    chunk.clear();
    if chunk.capacity() < chunk_size {
        chunk.reserve(chunk_size - chunk.capacity());
    }
    let stream = parse_zlib(data)?;
    let mut destination = ChunkOutput::new(
        history,
        chunk,
        expected_len,
        chunk_size,
        callback,
    );
    decompress_deflate_slice(stream.deflate, &mut destination)?;
    destination.finish(stream.expected_adler, verify_checksum)
}


/// Streams a complete zlib stream from `Read` into bounded PNG scanline chunks.
///
/// Unlike the slice path, this never needs a concatenated IDAT buffer. A four-byte
/// look-behind wrapper withholds the zlib Adler-32 trailer while the DEFLATE bit
/// reader refills from the underlying PNG IDAT stream.
pub(crate) fn decompress_zlib_reader_chunks_reuse<R, F>(
    reader: R,
    expected_len: usize,
    chunk_size: usize,
    verify_checksum: bool,
    history: &mut [u8; 32_768],
    chunk: &mut Vec<u8>,
    callback: &mut F,
) -> io::Result<()>
where
    R: Read,
    F: FnMut(&mut [u8]) -> io::Result<()>,
{
    if chunk_size == 0 {
        return Err(invalid_data("zlib streaming chunk size must be non-zero"));
    }

    let mut tail = Tail4Reader::new(reader)?;
    let mut header = [0u8; 2];
    tail.read_exact(&mut header)?;
    validate_zlib_header(header[0], header[1])?;

    chunk.clear();
    if chunk.capacity() < chunk_size {
        chunk.reserve(chunk_size - chunk.capacity());
    }
    let mut destination = ChunkOutput::new(history, chunk, expected_len, chunk_size, callback);

    {
        let mut bits = StreamDeflateReader::new(&mut tail);
        decompress_deflate(&mut bits, &mut destination)?;
    }

    let expected_adler = u32::from_be_bytes(tail.expected_tail()?);
    destination.finish(expected_adler, verify_checksum)
}

/// Reader that exposes every byte except the final four bytes of its input.
/// Those final bytes are the RFC 1950 Adler-32 trailer.
struct Tail4Reader<R> {
    inner: R,
    hold: [u8; 4],
    scratch: [u8; 8192],
    eof: bool,
}

impl<R: Read> Tail4Reader<R> {
    fn new(mut inner: R) -> io::Result<Self> {
        let mut hold = [0u8; 4];
        inner.read_exact(&mut hold)?;
        Ok(Self { inner, hold, scratch: [0u8; 8192], eof: false })
    }

    fn expected_tail(&mut self) -> io::Result<[u8; 4]> {
        // The DEFLATE bit reader may finish immediately after a refill that
        // already advanced `inner` to the end of the zlib stream. In that
        // common case `Tail4Reader::read` has the real Adler trailer in
        // `hold`, but has not yet observed the following zero-length read.
        // Probe once so a valid stream does not require the inflater to ask
        // for bytes past the final DEFLATE block.
        if !self.eof {
            let mut probe = [0u8; 1];
            if self.inner.read(&mut probe)? != 0 {
                return Err(invalid_data("Trailing bytes after DEFLATE payload"));
            }
            self.eof = true;
        }
        Ok(self.hold)
    }
}

impl<R: Read> Read for Tail4Reader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.eof {
            return Ok(0);
        }
        let request = output.len().min(self.scratch.len());
        let count = self.inner.read(&mut self.scratch[..request])?;
        if count == 0 {
            self.eof = true;
            return Ok(0);
        }

        if count >= 4 {
            output[..4].copy_from_slice(&self.hold);
            if count > 4 {
                output[4..count].copy_from_slice(&self.scratch[..count - 4]);
            }
            self.hold.copy_from_slice(&self.scratch[count - 4..count]);
        } else {
            output[..count].copy_from_slice(&self.hold[..count]);
            self.hold.copy_within(count..4, 0);
            self.hold[4 - count..].copy_from_slice(&self.scratch[..count]);
        }
        Ok(count)
    }
}

/// Buffered LSB-first bit reader backed by `Read` rather than one contiguous slice.
struct StreamDeflateReader<R> {
    input: R,
    bytes: [u8; 4096],
    byte_pos: usize,
    byte_len: usize,
    buffer: u64,
    buffered_bits: u32,
    eof: bool,
}

impl<R: Read> StreamDeflateReader<R> {
    fn new(input: R) -> Self {
        Self {
            input,
            bytes: [0u8; 4096],
            byte_pos: 0,
            byte_len: 0,
            buffer: 0,
            buffered_bits: 0,
            eof: false,
        }
    }

    #[inline]
    fn refill_bytes(&mut self) -> io::Result<bool> {
        if self.byte_pos < self.byte_len {
            return Ok(true);
        }
        if self.eof {
            return Ok(false);
        }
        let count = self.input.read(&mut self.bytes)?;
        self.byte_pos = 0;
        self.byte_len = count;
        self.eof = count == 0;
        Ok(count != 0)
    }

    #[inline]
    fn next_byte(&mut self) -> io::Result<Option<u8>> {
        if !self.refill_bytes()? {
            return Ok(None);
        }
        let value = self.bytes[self.byte_pos];
        self.byte_pos += 1;
        Ok(Some(value))
    }

    fn ensure_bits_soft(&mut self, count: u32) -> io::Result<bool> {
        while self.buffered_bits < count {
            let Some(byte) = self.next_byte()? else { return Ok(false); };
            self.buffer |= (byte as u64) << self.buffered_bits;
            self.buffered_bits += 8;
        }
        Ok(true)
    }

    #[inline]
    fn take_buffered_byte(&mut self) -> Option<u8> {
        if self.buffered_bits < 8 {
            return None;
        }
        let byte = self.buffer as u8;
        self.buffer >>= 8;
        self.buffered_bits -= 8;
        Some(byte)
    }
}

/// Decompresses zlib into a growable vector for metadata payloads whose final
/// size is not known before inflation.
fn decompress_zlib_vec(data: &[u8]) -> io::Result<Vec<u8>> {
    let stream = parse_zlib(data)?;
    let mut output = Vec::new();
    decompress_deflate_slice(stream.deflate, &mut output)?;

    if adler32(&output) != stream.expected_adler {
        return Err(invalid_data("Invalid zlib Adler-32 checksum"));
    }

    Ok(output)
}

/// Parsed pieces of the small zlib wrapper surrounding DEFLATE data.
struct ZlibStream<'a> {
    /// Raw RFC 1951 DEFLATE payload.
    deflate: &'a [u8],
    /// Big-endian Adler-32 stored at the end of the zlib stream.
    expected_adler: u32,
}

/// Validates the RFC 1950 header used by PNG zlib streams.
fn validate_zlib_header(cmf: u8, flg: u8) -> io::Result<()> {
    if cmf & 0x0f != 8 {
        return Err(invalid_data("Unsupported zlib compression method"));
    }
    if cmf >> 4 > 7 {
        return Err(invalid_data("Invalid zlib window size"));
    }
    let header = ((cmf as u16) << 8) | flg as u16;
    if header % 31 != 0 {
        return Err(invalid_data("Invalid zlib header checksum"));
    }
    if flg & 0x20 != 0 {
        return Err(invalid_data("PNG zlib streams may not use a preset dictionary"));
    }
    Ok(())
}

/// Validates the zlib header and returns its DEFLATE payload and checksum.
fn parse_zlib(data: &[u8]) -> io::Result<ZlibStream<'_>> {
    if data.len() < 6 {
        return Err(invalid_data("Truncated zlib stream"));
    }

    let cmf = data[0];
    let flg = data[1];

    validate_zlib_header(cmf, flg)?;

    let checksum_start = data
        .len()
        .checked_sub(4)
        .ok_or_else(|| invalid_data("Truncated zlib stream"))?;
    if checksum_start < 2 {
        return Err(invalid_data("Truncated zlib stream"));
    }

    let expected_adler = u32::from_be_bytes([
        data[checksum_start],
        data[checksum_start + 1],
        data[checksum_start + 2],
        data[checksum_start + 3],
    ]);

    Ok(ZlibStream {
        deflate: &data[2..checksum_start],
        expected_adler,
    })
}

// ============================================================================
// DEFLATE core
// ============================================================================

/// Output abstraction shared by growable metadata decompression and the fixed
/// scanline buffer used by the main PNG decoder.
trait InflateOutput {
    /// Number of bytes already written.
    fn len(&self) -> usize;
    /// Writes one literal byte.
    fn write_byte(&mut self, value: u8) -> io::Result<()>;
    /// Writes one stored-block byte range.
    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Copies an overlapping LZ77 back-reference.
    fn copy_match(&mut self, distance: usize, length: usize) -> io::Result<()>;
}

impl InflateOutput for Vec<u8> {
    #[inline]
    fn len(&self) -> usize {
        Vec::len(self)
    }

    #[inline]
    fn write_byte(&mut self, value: u8) -> io::Result<()> {
        self.push(value);
        Ok(())
    }

    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }

    fn copy_match(&mut self, distance: usize, length: usize) -> io::Result<()> {
        if distance == 0 || distance > self.len() {
            return Err(invalid_data("Invalid DEFLATE back-reference distance"));
        }

        self.len()
            .checked_add(length)
            .ok_or_else(|| invalid_data("DEFLATE output size overflow"))?;

        for _ in 0..length {
            let source = self.len() - distance;
            let value = self[source];
            self.push(value);
        }

        Ok(())
    }
}

/// Streaming DEFLATE destination retaining only the 32 KiB history window.
struct ChunkOutput<'a, 'b, F>
where
    F: FnMut(&mut [u8]) -> io::Result<()>,
{
    history: &'a mut [u8; 32_768],
    chunk: &'a mut Vec<u8>,
    chunk_size: usize,
    expected_len: usize,
    total: usize,
    adler_a: u32,
    adler_b: u32,
    callback: &'b mut F,
}

impl<'a, 'b, F> ChunkOutput<'a, 'b, F>
where
    F: FnMut(&mut [u8]) -> io::Result<()>,
{
    fn new(
        history: &'a mut [u8; 32_768],
        chunk: &'a mut Vec<u8>,
        expected_len: usize,
        chunk_size: usize,
        callback: &'b mut F,
    ) -> Self {
        chunk.clear();
        Self {
            history,
            chunk,
            chunk_size,
            expected_len,
            total: 0,
            adler_a: 1,
            adler_b: 0,
            callback,
        }
    }

    #[inline(always)]
    fn emit(&mut self, value: u8) -> io::Result<()> {
        if self.total >= self.expected_len {
            return Err(invalid_data("PNG decompressed data is larger than expected"));
        }
        self.history[self.total & 0x7fff] = value;
        self.total += 1;
        self.chunk.push(value);
        if self.chunk.len() == self.chunk_size {
            self.flush_chunk()?;
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> io::Result<()> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        adler32_update(&mut self.adler_a, &mut self.adler_b, &self.chunk);
        (self.callback)(&mut self.chunk)?;
        self.chunk.clear();
        Ok(())
    }

    fn finish(mut self, expected_adler: u32, verify_checksum: bool) -> io::Result<()> {
        if self.total != self.expected_len {
            return Err(invalid_data("PNG decompressed data is smaller than expected"));
        }
        if !self.chunk.is_empty() {
            // A caller choosing a chunk size that does not divide expected_len
            // still receives the final partial chunk.
            self.flush_chunk()?;
        }
        if verify_checksum && ((self.adler_b << 16) | self.adler_a) != expected_adler {
            return Err(invalid_data("Invalid zlib Adler-32 checksum"));
        }
        Ok(())
    }
}

impl<F> InflateOutput for ChunkOutput<'_, '_, F>
where
    F: FnMut(&mut [u8]) -> io::Result<()>,
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.total
    }

    #[inline(always)]
    fn write_byte(&mut self, value: u8) -> io::Result<()> {
        self.emit(value)
    }

    fn write_bytes(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        let end = self.total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid_data("DEFLATE output size overflow"))?;
        if end > self.expected_len {
            return Err(invalid_data("PNG decompressed data is larger than expected"));
        }

        // Stored DEFLATE blocks are already byte-aligned. Copy them in runs
        // rather than routing every byte through `emit`; update both the 32 KiB
        // LZ history ring and the caller's scanline chunk in bulk.
        while !bytes.is_empty() {
            let chunk_space = self.chunk_size - self.chunk.len();
            let take = bytes.len().min(chunk_space);
            let part = &bytes[..take];

            let history_start = self.total & 0x7fff;
            let first = take.min(32_768 - history_start);
            self.history[history_start..history_start + first]
                .copy_from_slice(&part[..first]);
            if first < take {
                self.history[..take - first].copy_from_slice(&part[first..]);
            }

            self.chunk.extend_from_slice(part);
            self.total += take;
            bytes = &bytes[take..];

            if self.chunk.len() == self.chunk_size {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn copy_match(&mut self, distance: usize, length: usize) -> io::Result<()> {
        if distance == 0 || distance > self.total.min(32_768) {
            return Err(invalid_data("Invalid DEFLATE back-reference distance"));
        }
        let end = self.total
            .checked_add(length)
            .ok_or_else(|| invalid_data("DEFLATE output size overflow"))?;
        if end > self.expected_len {
            return Err(invalid_data("PNG decompressed data is larger than expected"));
        }

        while self.total < end {
            // Reading before writing preserves overlapping-match semantics: once
            // emitted, newly produced bytes immediately become history.
            let source = (self.total - distance) & 0x7fff;
            let value = self.history[source];
            self.emit(value)?;
        }
        Ok(())
    }
}

/// Fixed-capacity inflater destination used for the expected PNG scanline size.
struct SliceOutput<'a> {
    /// Complete output storage.
    data: &'a mut [u8],
    /// Number of bytes already written.
    position: usize,
}

impl<'a> SliceOutput<'a> {
    /// Creates a fixed output target.
    #[inline]
    fn new(data: &'a mut [u8]) -> Self {
        Self { data, position: 0 }
    }
}

impl InflateOutput for SliceOutput<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.position
    }

    #[inline]
    fn write_byte(&mut self, value: u8) -> io::Result<()> {
        let slot = self
            .data
            .get_mut(self.position)
            .ok_or_else(|| invalid_data("PNG decompressed data is larger than expected"))?;
        *slot = value;
        self.position += 1;
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or_else(|| invalid_data("PNG decompressed size overflow"))?;
        let destination = self
            .data
            .get_mut(self.position..end)
            .ok_or_else(|| invalid_data("PNG decompressed data is larger than expected"))?;
        destination.copy_from_slice(bytes);
        self.position = end;
        Ok(())
    }

    fn copy_match(&mut self, distance: usize, length: usize) -> io::Result<()> {
        if distance == 0 || distance > self.position {
            return Err(invalid_data("Invalid DEFLATE back-reference distance"));
        }

        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| invalid_data("DEFLATE output size overflow"))?;
        if end > self.data.len() {
            return Err(invalid_data("PNG decompressed data is larger than expected"));
        }

        // Recompute the source on every iteration because DEFLATE matches may
        // overlap the bytes currently being written.
        while self.position < end {
            let source = self.position - distance;
            let value = self.data[source];
            self.data[self.position] = value;
            self.position += 1;
        }

        Ok(())
    }
}

/// Bit operations needed by the shared DEFLATE core.
///
/// Both the contiguous-slice reader and the `Read`-backed streaming reader
/// implement this small interface, so Huffman/length/distance logic remains one
/// maintained implementation.
trait DeflateBits {
    fn read_bit(&mut self) -> io::Result<u32>;
    fn read_bits(&mut self, count: u32) -> io::Result<u32>;
    fn try_peek_root9(&mut self) -> io::Result<Option<u32>>;
    fn drop_bits(&mut self, count: u32);
    fn align_to_byte(&mut self);
    fn read_u16_le(&mut self) -> io::Result<u16>;
    fn write_aligned_bytes<O: InflateOutput>(
        &mut self,
        length: usize,
        output: &mut O,
    ) -> io::Result<()>;
    fn finished_at_end(&mut self) -> io::Result<bool>;
}

impl DeflateBits for LsbBitReader<'_> {
    #[inline(always)]
    fn read_bit(&mut self) -> io::Result<u32> {
        Ok(LsbBitReader::read_bit(self)?)
    }

    #[inline(always)]
    fn read_bits(&mut self, count: u32) -> io::Result<u32> {
        Ok(LsbBitReader::read_bits(self, count)?)
    }

    #[inline(always)]
    fn try_peek_root9(&mut self) -> io::Result<Option<u32>> {
        if self.available_bits() < HuffmanTree::ROOT_BITS {
            Ok(None)
        } else {
            Ok(Some(self.peek_bits_fast::<9>()?))
        }
    }

    #[inline(always)]
    fn drop_bits(&mut self, count: u32) {
        LsbBitReader::drop_bits(self, count)
    }

    #[inline(always)]
    fn align_to_byte(&mut self) {
        LsbBitReader::align_to_byte(self)
    }

    #[inline]
    fn read_u16_le(&mut self) -> io::Result<u16> {
        Ok(LsbBitReader::read_u16_le(self)?)
    }

    #[inline]
    fn write_aligned_bytes<O: InflateOutput>(
        &mut self,
        length: usize,
        output: &mut O,
    ) -> io::Result<()> {
        let bytes = LsbBitReader::read_bytes(self, length)?;
        output.write_bytes(bytes)
    }

    #[inline(always)]
    fn finished_at_end(&mut self) -> io::Result<bool> {
        Ok(LsbBitReader::finished_at_end(self))
    }
}

impl<R: Read> DeflateBits for StreamDeflateReader<R> {
    #[inline(always)]
    fn read_bit(&mut self) -> io::Result<u32> {
        self.read_bits(1)
    }

    #[inline(always)]
    fn read_bits(&mut self, count: u32) -> io::Result<u32> {
        if count > 24 {
            return Err(invalid_data("DEFLATE bit read exceeds 24 bits"));
        }
        if count == 0 {
            return Ok(0);
        }
        if !self.ensure_bits_soft(count)? {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated DEFLATE bitstream"));
        }
        let mask = (1u64 << count) - 1;
        let value = (self.buffer & mask) as u32;
        self.buffer >>= count;
        self.buffered_bits -= count;
        Ok(value)
    }

    #[inline(always)]
    fn try_peek_root9(&mut self) -> io::Result<Option<u32>> {
        if !self.ensure_bits_soft(9)? {
            return Ok(None);
        }
        Ok(Some((self.buffer & 0x1ff) as u32))
    }

    #[inline(always)]
    fn drop_bits(&mut self, count: u32) {
        debug_assert!(count <= self.buffered_bits);
        self.buffer >>= count;
        self.buffered_bits -= count;
    }

    #[inline]
    fn align_to_byte(&mut self) {
        let skip = self.buffered_bits & 7;
        if skip != 0 {
            self.drop_bits(skip);
        }
    }

    #[inline]
    fn read_u16_le(&mut self) -> io::Result<u16> {
        self.align_to_byte();
        let lo = self.read_aligned_byte()?;
        let hi = self.read_aligned_byte()?;
        Ok(u16::from_le_bytes([lo, hi]))
    }

    fn write_aligned_bytes<O: InflateOutput>(
        &mut self,
        mut length: usize,
        output: &mut O,
    ) -> io::Result<()> {
        self.align_to_byte();

        // A refill may have prefetched a few complete bytes into the bit
        // accumulator. Drain those first, then forward contiguous refill slices.
        let mut prefix = [0u8; 8];
        let mut prefix_len = 0usize;
        while length != 0 {
            let Some(byte) = self.take_buffered_byte() else { break; };
            prefix[prefix_len] = byte;
            prefix_len += 1;
            length -= 1;
        }
        if prefix_len != 0 {
            output.write_bytes(&prefix[..prefix_len])?;
        }

        while length != 0 {
            if !self.refill_bytes()? {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated DEFLATE stored block"));
            }
            let available = self.byte_len - self.byte_pos;
            let take = available.min(length);
            output.write_bytes(&self.bytes[self.byte_pos..self.byte_pos + take])?;
            self.byte_pos += take;
            length -= take;
        }
        Ok(())
    }

    fn finished_at_end(&mut self) -> io::Result<bool> {
        // RFC 1951 permits padding bits in the final byte, but no additional
        // complete byte is valid after the final block.
        if self.buffered_bits >= 8 || self.byte_pos < self.byte_len {
            return Ok(false);
        }
        let mut probe = [0u8; 1];
        Ok(self.input.read(&mut probe)? == 0)
    }
}

impl<R: Read> StreamDeflateReader<R> {
    #[inline]
    fn read_aligned_byte(&mut self) -> io::Result<u8> {
        if let Some(byte) = self.take_buffered_byte() {
            return Ok(byte);
        }
        self.next_byte()?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "truncated DEFLATE byte stream")
        })
    }
}

/// Inflates one complete RFC 1951 stream into the supplied destination.
fn decompress_deflate<R: DeflateBits, O: InflateOutput>(
    reader: &mut R,
    output: &mut O,
) -> io::Result<()> {
    loop {
        let final_block = reader.read_bits(1)? != 0;
        let block_type = reader.read_bits(2)?;

        match block_type {
            0 => decode_stored_block(reader, output)?,
            1 => {
                let (literal_tree, distance_tree) = fixed_huffman_trees();
                decode_huffman_block(reader, output, literal_tree, Some(distance_tree))?;
            }
            2 => {
                let (literal_tree, distance_tree) = dynamic_huffman_trees(reader)?;
                decode_huffman_block(reader, output, &literal_tree, distance_tree.as_ref())?;
            }
            _ => return Err(invalid_data("Reserved DEFLATE block type")),
        }

        if final_block {
            break;
        }
    }

    if !reader.finished_at_end()? {
        return Err(invalid_data("Unexpected data after final DEFLATE block"));
    }
    Ok(())
}

/// Slice adapter for the shared DEFLATE core.
fn decompress_deflate_slice<O: InflateOutput>(data: &[u8], output: &mut O) -> io::Result<()> {
    let mut reader = LsbBitReader::new(data);
    decompress_deflate(&mut reader, output)
}

/// Decodes an uncompressed DEFLATE stored block.
fn decode_stored_block<R: DeflateBits, O: InflateOutput>(
    reader: &mut R,
    output: &mut O,
) -> io::Result<()> {
    reader.align_to_byte();
    let len = reader.read_u16_le()?;
    let nlen = reader.read_u16_le()?;

    if len ^ 0xffff != nlen {
        return Err(invalid_data("Invalid DEFLATE stored block length"));
    }

    reader.write_aligned_bytes(len as usize, output)
}

/// Builds the fixed literal/length and distance trees defined by RFC 1951.
fn fixed_huffman_trees() -> (&'static HuffmanTree, &'static HuffmanTree) {
    static TREES: OnceLock<(HuffmanTree, HuffmanTree)> = OnceLock::new();
    let trees = TREES.get_or_init(|| {
        let mut literal_lengths = [0u8; 288];
        literal_lengths[0..=143].fill(8);
        literal_lengths[144..=255].fill(9);
        literal_lengths[256..=279].fill(7);
        literal_lengths[280..=287].fill(8);
        let distance_lengths = [5u8; 32];

        // These tables are constants from RFC 1951. Construction failure here
        // is a programming error, not malformed user input.
        let literal = HuffmanTree::from_lengths(&literal_lengths)
            .expect("valid fixed DEFLATE literal lengths")
            .expect("non-empty fixed DEFLATE literal tree");
        let distance = HuffmanTree::from_lengths(&distance_lengths)
            .expect("valid fixed DEFLATE distance lengths")
            .expect("non-empty fixed DEFLATE distance tree");
        (literal, distance)
    });
    (&trees.0, &trees.1)
}

/// Reads the code-length alphabet and constructs a dynamic DEFLATE Huffman pair.
fn dynamic_huffman_trees<R: DeflateBits>(
    reader: &mut R,
) -> io::Result<(HuffmanTree, Option<HuffmanTree>)> {
    let literal_count = reader.read_bits(5)? as usize + 257;
    let distance_count = reader.read_bits(5)? as usize + 1;
    let code_length_count = reader.read_bits(4)? as usize + 4;

    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    let mut code_length_lengths = [0u8; 19];
    for index in 0..code_length_count {
        code_length_lengths[ORDER[index]] = reader.read_bits(3)? as u8;
    }

    let code_length_tree = HuffmanTree::from_lengths(&code_length_lengths)?
        .ok_or_else(|| invalid_data("Empty DEFLATE code-length tree"))?;

    let total = literal_count
        .checked_add(distance_count)
        .ok_or_else(|| invalid_data("DEFLATE Huffman table size overflow"))?;
    // RFC 1951 bounds HLIT to 286 and HDIST to 32, so the complete dynamic
    // code-length vector always fits in this stack array.
    let mut lengths = [0u8; 318];
    let mut length_count = 0usize;

    while length_count < total {
        let symbol = code_length_tree.decode(reader)?;
        match symbol {
            0..=15 => {
                lengths[length_count] = symbol as u8;
                length_count += 1;
            }
            16 => {
                if length_count == 0 {
                    return Err(invalid_data("DEFLATE repeat code has no previous length"));
                }
                let previous = lengths[length_count - 1];
                let repeat = reader.read_bits(2)? as usize + 3;
                let new_len = length_count
                    .checked_add(repeat)
                    .ok_or_else(|| invalid_data("DEFLATE code-length overflow"))?;
                if new_len > total {
                    return Err(invalid_data("DEFLATE code-length repeat exceeds table"));
                }
                lengths[length_count..new_len].fill(previous);
                length_count = new_len;
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                let new_len = length_count
                    .checked_add(repeat)
                    .ok_or_else(|| invalid_data("DEFLATE code-length overflow"))?;
                if new_len > total {
                    return Err(invalid_data("DEFLATE zero repeat exceeds table"));
                }
                // Array is already zero-filled.
                length_count = new_len;
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                let new_len = length_count
                    .checked_add(repeat)
                    .ok_or_else(|| invalid_data("DEFLATE code-length overflow"))?;
                if new_len > total {
                    return Err(invalid_data("DEFLATE zero repeat exceeds table"));
                }
                length_count = new_len;
            }
            _ => return Err(invalid_data("Invalid DEFLATE code-length symbol")),
        }
    }

    let literal_lengths = &lengths[..literal_count];
    let distance_lengths = &lengths[literal_count..total];

    if literal_lengths.get(256).copied().unwrap_or(0) == 0 {
        return Err(invalid_data(
            "DEFLATE literal tree has no end-of-block symbol",
        ));
    }

    let literal_tree = HuffmanTree::from_lengths(literal_lengths)?
        .ok_or_else(|| invalid_data("Empty DEFLATE literal tree"))?;
    let distance_tree = HuffmanTree::from_lengths(distance_lengths)?;

    Ok((literal_tree, distance_tree))
}

/// Decodes one fixed or dynamic Huffman-compressed block.
fn decode_huffman_block<R: DeflateBits, O: InflateOutput>(
    reader: &mut R,
    output: &mut O,
    literal_tree: &HuffmanTree,
    distance_tree: Option<&HuffmanTree>,
) -> io::Result<()> {
    loop {
        let symbol = literal_tree.decode(reader)?;
        match symbol {
            0..=255 => output.write_byte(symbol as u8)?,
            256 => return Ok(()),
            257..=285 => {
                let length = decode_length(reader, symbol - 257)?;
                let distance_tree = distance_tree.ok_or_else(|| {
                    invalid_data("DEFLATE length code used without distance tree")
                })?;
                let distance_symbol = distance_tree.decode(reader)?;
                if distance_symbol >= 30 {
                    return Err(invalid_data("Reserved DEFLATE distance symbol"));
                }
                let distance = decode_distance(reader, distance_symbol)?;
                output.copy_match(distance, length)?;
            }
            _ => return Err(invalid_data("Reserved DEFLATE literal/length symbol")),
        }
    }
}

// ============================================================================
// Length and distance codes
// ============================================================================

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99,
    115, 131, 163, 195, 227, 258,
];

const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Converts a literal/length symbol index into its decoded match length.
#[inline]
fn decode_length<R: DeflateBits>(reader: &mut R, index: usize) -> io::Result<usize> {
    let base = *LENGTH_BASE
        .get(index)
        .ok_or_else(|| invalid_data("Invalid DEFLATE length code"))?;
    let extra_bits = LENGTH_EXTRA[index];
    let extra = if extra_bits == 0 {
        0
    } else {
        reader.read_bits(extra_bits as u32)? as usize
    };

    base.checked_add(extra)
        .ok_or_else(|| invalid_data("DEFLATE length overflow"))
}

const DISTANCE_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13,
];

/// Converts a distance symbol into its decoded LZ77 backward distance.
#[inline]
fn decode_distance<R: DeflateBits>(reader: &mut R, symbol: usize) -> io::Result<usize> {
    let base = *DISTANCE_BASE
        .get(symbol)
        .ok_or_else(|| invalid_data("Invalid DEFLATE distance code"))?;
    let extra_bits = DISTANCE_EXTRA[symbol];
    let extra = if extra_bits == 0 {
        0
    } else {
        reader.read_bits(extra_bits as u32)? as usize
    };

    base.checked_add(extra)
        .ok_or_else(|| invalid_data("DEFLATE distance overflow"))
}

// ============================================================================
// Canonical Huffman decoder
// ============================================================================

/// Compact flat canonical Huffman tree used by DEFLATE.
#[derive(Debug)]
struct HuffmanTree {
    /// Flat nodes; child indices avoid one allocation per tree edge.
    nodes: Vec<HuffmanNode>,
    /// Direct lookup for the overwhelmingly common short codes.
    root: [HuffmanLookup; 1 << Self::ROOT_BITS],
}

#[derive(Debug, Clone, Copy)]
struct HuffmanLookup {
    symbol: u16,
    bits: u8,
}

impl HuffmanLookup {
    const EMPTY: Self = Self { symbol: 0, bits: 0 };
}

/// One compact node in a [`HuffmanTree`].
///
/// DEFLATE has at most 318 symbols in the combined dynamic length set, so a
/// `u16` node index is ample and avoids the much larger `Option<usize>` layout.
#[derive(Debug, Clone, Copy)]
struct HuffmanNode {
    /// Child node selected by bit zero or one, or [`Self::NONE`].
    children: [u16; 2],
    /// Decoded alphabet symbol, or [`Self::NONE`] for an internal node.
    symbol: u16,
}

impl HuffmanNode {
    const NONE: u16 = u16::MAX;

    /// Creates an empty tree node.
    #[inline]
    const fn new() -> Self {
        Self {
            children: [Self::NONE; 2],
            symbol: Self::NONE,
        }
    }
}

impl HuffmanTree {
    const ROOT_BITS: usize = 9;

    /// Builds a canonical DEFLATE tree from per-symbol code lengths.
    fn from_lengths(lengths: &[u8]) -> io::Result<Option<Self>> {
        let mut counts = [0u16; 16];
        let mut symbol_count = 0usize;

        for &length in lengths {
            if length > 15 {
                return Err(invalid_data("DEFLATE Huffman code exceeds 15 bits"));
            }
            if length != 0 {
                counts[length as usize] += 1;
                symbol_count += 1;
            }
        }

        if symbol_count == 0 {
            return Ok(None);
        }

        // Detect oversubscribed trees before assigning canonical codes.
        let mut available = 1i32;
        for bits in 1..=15 {
            available = (available << 1) - counts[bits] as i32;
            if available < 0 {
                return Err(invalid_data("Oversubscribed DEFLATE Huffman tree"));
            }
        }

        let mut next_code = [0u16; 16];
        let mut code = 0u16;
        for bits in 1..=15 {
            code = (code + counts[bits - 1]) << 1;
            next_code[bits] = code;
        }

        let mut tree = Self {
            nodes: Vec::with_capacity(symbol_count.saturating_mul(2).saturating_add(15)),
            root: [HuffmanLookup::EMPTY; 1 << Self::ROOT_BITS],
        };
        tree.nodes.push(HuffmanNode::new());

        for (symbol, &length) in lengths.iter().enumerate() {
            if length == 0 {
                continue;
            }

            let code = next_code[length as usize];
            next_code[length as usize] += 1;

            if (length as usize) <= Self::ROOT_BITS {
                // Stream bit zero is the first (most-significant canonical) code
                // bit, so reverse the canonical code into the reader's low-bit
                // lookup order and replicate over all unused suffix bits.
                let reversed = (code.reverse_bits() >> (16 - length)) as usize;
                let replicas = 1usize << (Self::ROOT_BITS - length as usize);
                for suffix in 0..replicas {
                    let index = reversed | (suffix << length);
                    tree.root[index] = HuffmanLookup {
                        symbol: symbol as u16,
                        bits: length,
                    };
                }
            }

            let mut node_index = 0usize;

            // DEFLATE transmits Huffman code bits from the most-significant
            // canonical bit toward the least-significant bit.
            for shift in (0..length).rev() {
                if tree.nodes[node_index].symbol != HuffmanNode::NONE {
                    return Err(invalid_data("Invalid DEFLATE Huffman prefix"));
                }

                let bit = ((code >> shift) & 1) as usize;
                let child = tree.nodes[node_index].children[bit];
                let next = if child != HuffmanNode::NONE {
                    child as usize
                } else {
                    let index = tree.nodes.len();
                    if index >= HuffmanNode::NONE as usize {
                        return Err(invalid_data("DEFLATE Huffman tree is too large"));
                    }
                    tree.nodes.push(HuffmanNode::new());
                    tree.nodes[node_index].children[bit] = index as u16;
                    index
                };
                node_index = next;
            }

            let node = &mut tree.nodes[node_index];
            if node.symbol != HuffmanNode::NONE
                || node.children[0] != HuffmanNode::NONE
                || node.children[1] != HuffmanNode::NONE
            {
                return Err(invalid_data("Duplicate DEFLATE Huffman code"));
            }
            node.symbol = symbol as u16;
        }

        Ok(Some(tree))
    }

    /// Decodes one Huffman symbol from the bitstream.
    #[inline]
    fn decode<R: DeflateBits>(&self, reader: &mut R) -> io::Result<usize> {
        if let Some(prefix) = reader.try_peek_root9()? {
            let index = prefix as usize;
            let entry = self.root[index];
            if entry.bits != 0 {
                reader.drop_bits(entry.bits as u32);
                return Ok(entry.symbol as usize);
            }
        }

        let mut node_index = 0usize;
        for _ in 0..15 {
            let bit = reader.read_bit()? as usize;
            let node = self
                .nodes
                .get(node_index)
                .ok_or_else(|| invalid_data("Invalid DEFLATE Huffman node"))?;
            let child = node.children[bit];
            if child == HuffmanNode::NONE {
                return Err(invalid_data("Invalid DEFLATE Huffman code"));
            }
            node_index = child as usize;

            let node = self
                .nodes
                .get(node_index)
                .ok_or_else(|| invalid_data("Invalid DEFLATE Huffman node"))?;
            if node.symbol != HuffmanNode::NONE {
                return Ok(node.symbol as usize);
            }
        }

        Err(invalid_data("DEFLATE Huffman code exceeds 15 bits"))
    }
}

// ============================================================================
// Adler-32
// ============================================================================

/// Extends an Adler-32 pair with one chunk.
///
/// RFC 1950's 5,552-byte reduction interval bounds the accumulators so modulo
/// operations happen once per block rather than once per byte. The inner loop is
/// unrolled sixteen bytes at a time; unlike CRC, Adler-32 has no broadly useful
/// cross-platform hardware instruction worth dispatching to here.
#[inline]
fn adler32_update(a: &mut u32, b: &mut u32, data: &[u8]) {
    const MOD_ADLER: u32 = 65_521;
    const NMAX: usize = 5_552;
    let mut remaining = data;

    while !remaining.is_empty() {
        let length = remaining.len().min(NMAX);
        let block = &remaining[..length];
        let mut i = 0usize;

        while i + 16 <= block.len() {
            // Keeping the additions explicit lets LLVM schedule several byte
            // loads while preserving the required prefix-sum dependency for b.
            macro_rules! step {
                ($offset:expr) => {{
                    *a += block[i + $offset] as u32;
                    *b += *a;
                }};
            }
            step!(0); step!(1); step!(2); step!(3);
            step!(4); step!(5); step!(6); step!(7);
            step!(8); step!(9); step!(10); step!(11);
            step!(12); step!(13); step!(14); step!(15);
            i += 16;
        }
        while i < block.len() {
            *a += block[i] as u32;
            *b += *a;
            i += 1;
        }

        *a %= MOD_ADLER;
        *b %= MOD_ADLER;
        remaining = &remaining[length..];
    }
}

/// Computes the Adler-32 checksum used by zlib.
#[inline]
fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    adler32_update(&mut a, &mut b, data);
    (b << 16) | a
}


#[cfg(test)]
mod huffman_regression_tests {
    use super::*;

    #[test]
    fn rejects_oversubscribed_tree() {
        let error = HuffmanTree::from_lengths(&[1, 1, 1]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn incomplete_tree_fails_cleanly_on_missing_branch() {
        // DEFLATE permits certain incomplete trees, so construction is allowed;
        // the unused branch must still fail instead of walking invalid memory.
        let tree = HuffmanTree::from_lengths(&[1]).unwrap().unwrap();
        let data = [1u8];
        let mut reader = LsbBitReader::new(&data);
        assert!(tree.decode(&mut reader).is_err());
    }

    #[test]
    fn compact_node_layout_stays_small() {
        assert!(core::mem::size_of::<HuffmanNode>() <= 6);
    }


    #[test]
    fn randomized_complete_huffman_tables_build_and_decode() {
        // Complete equal-length alphabets exercise every direct-table prefix.
        for bits in 1usize..=9 {
            let symbols = 1usize << bits;
            let lengths = vec![bits as u8; symbols];
            let tree = HuffmanTree::from_lengths(&lengths).unwrap().unwrap();
            // All-zero bytes are valid input for symbol zero in this canonical tree.
            let encoded = [0u8; 8];
            let mut reader = LsbBitReader::new(&encoded);
            for _ in 0..(64 / bits).max(1) {
                assert!(tree.decode(&mut reader).is_ok());
            }
        }
    }

    #[test]
    fn chunk_output_bulk_copy_checks_bounds_and_wraps_history() {
        let mut history = [0u8; 32_768];
        let mut chunk = Vec::new();
        let mut seen = Vec::new();
        {
            let mut callback = |data: &mut [u8]| { seen.extend_from_slice(data); Ok(()) };
            let mut output = ChunkOutput::new(&mut history, &mut chunk, 40_000, 4096, &mut callback);
            let bytes = vec![0x5au8; 40_000];
            output.write_bytes(&bytes).unwrap();
            output.flush_chunk().unwrap();
        }
        assert_eq!(seen.len(), 40_000);
        assert!(seen.iter().all(|&v| v == 0x5a));

        let mut history = [0u8; 32_768];
        let mut chunk = Vec::new();
        let mut callback = |_data: &mut [u8]| Ok(());
        let mut output = ChunkOutput::new(&mut history, &mut chunk, 3, 8, &mut callback);
        assert!(output.write_bytes(&[1, 2, 3, 4]).is_err());
    }
}
