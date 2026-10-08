use core::{mem::{align_of, size_of}, ops::{Deref, DerefMut}};

use crate::AudioError;

/// Hard ceiling used by decoder-controlled scratch allocations. The limit is
/// deliberately generous for valid high-channel-count streams while preventing
/// corrupt headers from requesting multi-gigabyte temporary buffers.
pub(crate) const MAX_DECODER_SCRATCH_BYTES: usize = 256 * 1024 * 1024;
/// Maximum single metadata/setup allocation accepted by codec hot/cold state.
pub(crate) const MAX_SETUP_ALLOCATION_BYTES: usize = 128 * 1024 * 1024;

#[inline]
pub(crate) fn checked_allocation_len<T>(elements: usize, context: &'static str) -> Result<usize, AudioError> {
    let bytes = elements.checked_mul(size_of::<T>())
        .ok_or_else(|| AudioError::InvalidFormat(format!("{context} allocation size overflow")))?;
    if bytes > MAX_DECODER_SCRATCH_BYTES {
        return Err(AudioError::InvalidFormat(format!(
            "{context} allocation of {bytes} bytes exceeds bounded scratch limit {MAX_DECODER_SCRATCH_BYTES}"
        )));
    }
    Ok(elements)
}

#[inline]
pub(crate) fn checked_setup_len<T>(elements: usize, context: &'static str) -> Result<usize, AudioError> {
    let bytes = elements.checked_mul(size_of::<T>())
        .ok_or_else(|| AudioError::InvalidFormat(format!("{context} allocation size overflow")))?;
    if bytes > MAX_SETUP_ALLOCATION_BYTES {
        return Err(AudioError::InvalidFormat(format!(
            "{context} allocation of {bytes} bytes exceeds bounded setup limit {MAX_SETUP_ALLOCATION_BYTES}"
        )));
    }
    Ok(elements)
}

/// Reusable 64-byte-aligned scratch backed entirely by safe `Vec` storage.
///
/// The vector is over-allocated by at most one cache line and the exposed
/// logical slice begins at the first naturally aligned element. This avoids a
/// custom allocator while guaranteeing AVX/NEON-friendly alignment for f32 and
/// i64 hot buffers. `clear()` only resets logical length; capacity is retained.
#[derive(Clone)]
pub(crate) struct AlignedScratch<T: Clone + Default> {
    storage: Vec<T>,
    offset: usize,
    len: usize,
    peak_capacity: usize,
}

impl<T: Clone + Default> core::fmt::Debug for AlignedScratch<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AlignedScratch")
            .field("len", &self.len)
            .field("capacity", &self.capacity())
            .field("alignment", &64usize)
            .finish()
    }
}

impl<T: Clone + Default> Default for AlignedScratch<T> {
    fn default() -> Self { Self { storage: Vec::new(), offset: 0, len: 0, peak_capacity: 0 } }
}

impl<T: Clone + Default> AlignedScratch<T> {
    const ALIGNMENT: usize = 64;

    pub(crate) fn with_len(len: usize, value: T) -> Result<Self, AudioError> {
        checked_allocation_len::<T>(len, "aligned scratch")?;
        let mut out = Self::default();
        out.reallocate(len, value);
        out.len = len;
        Ok(out)
    }

    pub(crate) fn with_capacity(capacity: usize) -> Result<Self, AudioError> {
        checked_allocation_len::<T>(capacity, "aligned scratch")?;
        let mut out = Self::default();
        if capacity != 0 { out.reallocate(capacity, T::default()); out.len = 0; }
        Ok(out)
    }

    fn pad_elements() -> usize {
        let size = size_of::<T>().max(1);
        // All hot users are f32/i64, for which 64 is an exact multiple. Keep
        // this generic helper correct for any smaller power-of-two alignment.
        (Self::ALIGNMENT / size).max(1) + 1
    }

    fn calculate_offset(storage: &[T]) -> usize {
        if storage.is_empty() { return 0; }
        let size = size_of::<T>().max(1);
        let address = storage.as_ptr() as usize;
        let missing = (Self::ALIGNMENT - (address % Self::ALIGNMENT)) % Self::ALIGNMENT;
        let offset = (missing + size - 1) / size;
        debug_assert_eq!((address + offset * size) % Self::ALIGNMENT, 0);
        offset
    }

    fn reallocate(&mut self, requested: usize, fill: T) {
        let total = requested.saturating_add(Self::pad_elements());
        let storage = vec![fill; total];
        let offset = Self::calculate_offset(&storage);
        debug_assert!(storage.len().saturating_sub(offset) >= requested);
        self.storage = storage;
        self.offset = offset;
        self.peak_capacity = self.peak_capacity.max(self.capacity());
    }

    pub(crate) fn try_resize(&mut self, len: usize, value: T, context: &'static str) -> Result<(), AudioError> {
        checked_allocation_len::<T>(len, context)?;
        if len > self.capacity() {
            let geometric = self.capacity().max(64).saturating_mul(2);
            self.reallocate(len.max(geometric).min(MAX_DECODER_SCRATCH_BYTES / size_of::<T>().max(1)), value.clone());
        }
        if len > self.len {
            self.storage[self.offset + self.len..self.offset + len].fill(value);
        }
        self.len = len;
        self.peak_capacity = self.peak_capacity.max(self.capacity());
        Ok(())
    }

    #[inline] pub(crate) fn len(&self) -> usize { self.len }
    #[inline] pub(crate) fn is_empty(&self) -> bool { self.len == 0 }
    #[inline] pub(crate) fn capacity(&self) -> usize { self.storage.len().saturating_sub(self.offset) }
    #[inline] pub(crate) fn peak_capacity(&self) -> usize { self.peak_capacity }
    #[inline] pub(crate) fn alignment(&self) -> usize { Self::ALIGNMENT.max(align_of::<T>()) }
    #[inline] pub(crate) fn clear(&mut self) { self.len = 0; }

    pub(crate) fn shrink_to_len(&mut self) {
        if self.len == 0 { self.storage.clear(); self.storage.shrink_to_fit(); self.offset = 0; return; }
        let current = self.to_vec();
        self.storage.clear();
        self.offset = 0;
        self.reallocate(current.len(), T::default());
        self.len = current.len();
        self.storage[self.offset..self.offset + self.len].clone_from_slice(&current);
        // Do not call `shrink_to_fit` here: a reallocation performed by Vec may
        // move the aligned logical start. `reallocate` already creates an exact
        // bounded backing allocation plus one cache-line of alignment padding.
    }

    #[inline]
    pub(crate) fn allocated_bytes(&self) -> usize { self.capacity().saturating_mul(size_of::<T>()) }
    #[inline]
    pub(crate) fn peak_allocated_bytes(&self) -> usize { self.peak_capacity.saturating_mul(size_of::<T>()) }
}

impl<T: Clone + Default> Deref for AlignedScratch<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target { &self.storage[self.offset..self.offset + self.len] }
}
impl<T: Clone + Default> DerefMut for AlignedScratch<T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.storage[self.offset..self.offset + self.len] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_float_and_integer_scratch_is_cache_line_aligned() {
        let mut f = AlignedScratch::<f32>::with_len(257, 0.0).unwrap();
        let mut i = AlignedScratch::<i64>::with_len(257, 0).unwrap();
        assert_eq!((f.as_ptr() as usize) % 64, 0);
        assert_eq!((i.as_ptr() as usize) % 64, 0);
        let fp = f.as_ptr(); let ip = i.as_ptr();
        f.clear(); i.clear();
        f.try_resize(128, 0.0, "test").unwrap(); i.try_resize(128, 0, "test").unwrap();
        assert_eq!(fp, f.as_ptr()); assert_eq!(ip, i.as_ptr());
    }

    #[test]
    fn corrupt_sized_requests_are_bounded() {
        assert!(checked_allocation_len::<f32>(usize::MAX, "test").is_err());
        assert!(checked_setup_len::<u64>(usize::MAX, "test").is_err());
    }
}
