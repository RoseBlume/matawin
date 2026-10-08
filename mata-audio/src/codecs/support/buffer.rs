use std::io::Read;

use crate::AudioError;
use super::memory::MAX_DECODER_SCRATCH_BYTES;

/// Codec-specific refill watermarks for compressed input.
///
/// `low_watermark` decides when a refill should begin, `high_watermark` is the
/// target amount of live data after a refill, and `read_block` controls the
/// minimum sequential read size used to amortize I/O calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RefillPolicy {
    pub(crate) low_watermark: usize,
    pub(crate) high_watermark: usize,
    pub(crate) read_block: usize,
}

impl RefillPolicy {
    pub(crate) const fn new(low_watermark: usize, high_watermark: usize, read_block: usize) -> Self {
        Self { low_watermark, high_watermark, read_block }
    }

    #[inline]
    pub(crate) fn needs_refill(self, live_bytes: usize) -> bool {
        live_bytes < self.low_watermark
    }

    #[inline]
    pub(crate) fn target(self, requested: usize) -> usize {
        if requested > self.high_watermark { requested } else { self.high_watermark }
    }
}

/// Reusable compressed-byte window with an inexpensive logical head.
///
/// Consuming bytes advances `head`; physical compaction happens only after a
/// threshold is crossed. This avoids `Vec::drain(..n)` on the steady-state
/// compressed-input path.
#[derive(Debug, Default)]
pub(crate) struct ByteWindow {
    data: Vec<u8>,
    head: usize,
    peak_capacity: usize,
}

impl ByteWindow {
    pub(crate) const DEFAULT_COMPACT_THRESHOLD: usize = 32 * 1024;

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.min(MAX_DECODER_SCRATCH_BYTES);
        let data = Vec::with_capacity(capacity);
        let peak_capacity = data.capacity();
        Self { data, head: 0, peak_capacity }
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[u8] { &self.data[self.head..] }

    #[inline]
    pub(crate) fn len(&self) -> usize { self.data.len().saturating_sub(self.head) }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool { self.len() == 0 }

    #[inline]
    pub(crate) fn capacity(&self) -> usize { self.data.capacity() }

    #[inline]
    pub(crate) fn peak_capacity(&self) -> usize { self.peak_capacity }

    #[inline]
    pub(crate) fn head(&self) -> usize { self.head }

    /// Absolute backing-vector length of the live tail. This is useful when a
    /// codec recognizes a bounded trailing tag after EOF and wants to trim it
    /// without moving the live prefix.
    #[inline]
    pub(crate) fn truncate_live(&mut self, live_len: usize) {
        let live_len = live_len.min(self.len());
        self.data.truncate(self.head.saturating_add(live_len));
        if live_len == 0 { self.clear(); }
    }

    /// Returns the live tail starting `offset` bytes after the logical head.
    #[inline]
    pub(crate) fn slice_from(&self, offset: usize) -> &[u8] {
        let offset = offset.min(self.len());
        &self.data[self.head + offset..]
    }

    pub(crate) fn clear(&mut self) {
        self.data.clear();
        self.head = 0;
    }

    pub(crate) fn consume(&mut self, bytes: usize) -> Result<(), AudioError> {
        if bytes == 0 || bytes > self.len() {
            return Err(AudioError::InvalidFormat(format!(
                "invalid compressed-byte consumption of {bytes} from {} buffered bytes",
                self.len(),
            )));
        }
        self.head += bytes;
        if self.head == self.data.len() {
            self.clear();
        } else {
            self.compact_if_needed(Self::DEFAULT_COMPACT_THRESHOLD);
        }
        Ok(())
    }

    pub(crate) fn keep_tail(&mut self, tail: usize) {
        if self.len() <= tail { return; }
        self.head = self.data.len() - tail;
        self.compact_if_needed(Self::DEFAULT_COMPACT_THRESHOLD.min(tail.max(1)));
    }

    pub(crate) fn compact_if_needed(&mut self, threshold: usize) -> bool {
        if self.head == 0 { return false; }
        if self.head < threshold && self.head * 2 < self.data.len() { return false; }
        let live = self.len();
        self.data.copy_within(self.head.., 0);
        self.data.truncate(live);
        self.head = 0;
        true
    }

    fn ensure_total_capacity(&mut self, required: usize) -> Result<(), AudioError> {
        if required > MAX_DECODER_SCRATCH_BYTES {
            return Err(AudioError::InvalidFormat(format!(
                "compressed buffer request of {required} bytes exceeds bounded limit {MAX_DECODER_SCRATCH_BYTES}"
            )));
        }
        if required <= self.data.capacity() { return Ok(()); }
        let geometric = self.data.capacity().max(4096).saturating_mul(2);
        let target = required.max(geometric).min(MAX_DECODER_SCRATCH_BYTES);
        self.data.reserve_exact(target.saturating_sub(self.data.len()));
        self.peak_capacity = self.peak_capacity.max(self.data.capacity());
        Ok(())
    }

    pub(crate) fn try_extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), AudioError> {
        if bytes.is_empty() { return Ok(()); }
        if self.head != 0 && self.data.capacity().saturating_sub(self.data.len()) < bytes.len() {
            self.compact_if_needed(0);
        }
        let required = self.data.len().checked_add(bytes.len())
            .ok_or_else(|| AudioError::InvalidFormat("compressed buffer size overflow".into()))?;
        self.ensure_total_capacity(required)?;
        self.data.extend_from_slice(bytes);
        self.peak_capacity = self.peak_capacity.max(self.data.capacity());
        Ok(())
    }

    /// Infallible compatibility helper for already-bounded codec input. New
    /// parsers should prefer `try_extend_from_slice` when lengths originate in
    /// untrusted stream metadata.
    pub(crate) fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.try_extend_from_slice(bytes).expect("bounded compressed buffer exceeded");
    }

    pub(crate) fn shrink_to_fit_non_realtime(&mut self) {
        if self.head != 0 { self.compact_if_needed(0); }
        self.data.shrink_to_fit();
        self.peak_capacity = self.peak_capacity.max(self.data.capacity());
    }

    /// Reads directly into spare `Vec` capacity and exposes only initialized
    /// bytes. Capacity grows geometrically and is retained across refills.
    pub(crate) fn read_from<R: Read>(
        &mut self,
        reader: &mut R,
        target_live_len: usize,
        read_block: usize,
    ) -> Result<usize, AudioError> {
        let mut total = 0usize;
        let read_block = read_block.max(1024);
        while self.len() < target_live_len {
            if self.head != 0 && self.data.capacity().saturating_sub(self.data.len()) < read_block {
                self.compact_if_needed(0);
            }
            let want = target_live_len.saturating_sub(self.len()).max(read_block);
            let required = self.data.len().checked_add(want)
                .ok_or_else(|| AudioError::InvalidFormat("compressed refill size overflow".into()))?;
            self.ensure_total_capacity(required)?;
            let spare = self.data.spare_capacity_mut();
            let take = spare.len().min(want);
            if take == 0 { break; }

            // SAFETY: `u8` has no invalid bit patterns. `Read::read` is allowed
            // to initialize at most `take` bytes. We increase the vector length
            // by exactly the number of bytes that `read` reports initialized.
            let dst = unsafe {
                std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, take)
            };
            let read = match reader.read(dst) {
                Ok(n) => n,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(AudioError::Io(error)),
            };
            if read == 0 { break; }
            let old_len = self.data.len();
            // SAFETY: the previous read initialized the first `read` spare
            // bytes and `old_len + read <= capacity` by construction.
            unsafe { self.data.set_len(old_len + read) };
            total += read;
        }
        Ok(total)
    }

    #[cfg(test)]
    pub(crate) fn backing_len(&self) -> usize { self.data.len() }
}

#[cfg(test)]
mod tests {
    use super::ByteWindow;
    use std::io::Cursor;

    #[test]
    fn consume_uses_head_then_compacts() {
        let mut window = ByteWindow::with_capacity(64);
        window.extend_from_slice(&(0u8..64).collect::<Vec<_>>());
        window.consume(16).unwrap();
        assert_eq!(window.as_slice()[0], 16);
        window.consume(32).unwrap();
        assert_eq!(window.as_slice(), &(48u8..64).collect::<Vec<_>>()[..]);
    }

    #[test]
    fn refill_reads_into_reused_capacity() {
        let mut window = ByteWindow::with_capacity(32);
        let mut input = Cursor::new(vec![7u8; 128]);
        assert!(window.read_from(&mut input, 64, 16).unwrap() >= 64);
        let capacity = window.capacity();
        window.consume(48).unwrap();
        let _ = window.read_from(&mut input, 64, 16).unwrap();
        assert!(window.capacity() >= capacity);
    }
}

#[cfg(test)]
mod hardening_mutations {
    use super::ByteWindow;
    fn next(s:&mut u64)->u64{*s^=*s<<13;*s^=*s>>7;*s^=*s<<17;*s}
    #[test]
    fn random_mpeg_window_compaction_matches_naive_vec() {
        let mut seed=0x4a53_5c11_D15C_A11Eu64;
        let mut window=ByteWindow::with_capacity(512);
        let mut reference=Vec::<u8>::new();
        for _ in 0..12000 {
            let op=next(&mut seed)%5;
            match op {
                0|1=>{let n=(next(&mut seed)%128) as usize;let bytes=(0..n).map(|_|next(&mut seed) as u8).collect::<Vec<_>>();window.try_extend_from_slice(&bytes).unwrap();reference.extend_from_slice(&bytes);},
                2 if !reference.is_empty()=>{let n=1+(next(&mut seed) as usize%reference.len());window.consume(n).unwrap();reference.drain(..n);},
                3=>{let n=(next(&mut seed) as usize)%(reference.len()+1);window.keep_tail(n);let cut=reference.len().saturating_sub(n);reference.drain(..cut);},
                4=>{let n=(next(&mut seed) as usize)%(reference.len()+1);window.truncate_live(n);reference.truncate(n);},
                _=>{}
            }
            assert_eq!(window.as_slice(),reference);
            assert!(window.capacity()<=16*1024*1024);
        }
    }
}
