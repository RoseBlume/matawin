//! Reusable decode scratch storage for repeated-image workloads.
//!
//! Fields are intentionally private so codec-specific scratch layouts can evolve
//! without becoming part of the stable public representation.

/// Optional reusable working memory for callers decoding many images.
///
/// Today PNG uses the cross-decode DEFLATE history/chunk/previous-row storage.
/// Other codecs already keep their hottest fixed scratch on the stack or reuse
/// it across frames internally; they can adopt this object later without an API
/// change.
pub struct DecodeScratch {
    pub(crate) png_history: Box<[u8; 32_768]>,
    pub(crate) png_chunk: Vec<u8>,
    pub(crate) png_previous_row: Vec<u8>,
    max_retained_bytes: usize,
}

impl Default for DecodeScratch {
    fn default() -> Self { Self::new() }
}

impl DecodeScratch {
    /// Creates empty reusable scratch with a fixed 32 KiB DEFLATE history.
    pub fn new() -> Self {
        Self {
            png_history: Box::new([0; 32_768]),
            png_chunk: Vec::new(),
            png_previous_row: Vec::new(),
            // Keep at most 8 MiB of variable scratch by default between decodes.
            max_retained_bytes: 8 * 1024 * 1024,
        }
    }

    /// Sets the maximum variable scratch capacity retained between explicit
    /// `clear` calls. The fixed 32 KiB DEFLATE history is always retained.
    pub fn set_max_retained_bytes(&mut self, bytes: usize) {
        self.max_retained_bytes = bytes;
    }

    /// Clears logical contents while retaining bounded capacity for reuse.
    pub fn clear(&mut self) {
        self.png_chunk.clear();
        self.png_previous_row.clear();
        self.enforce_retention_limit();
    }

    /// Releases all optional variable capacity immediately.
    pub fn shrink(&mut self) {
        self.png_chunk = Vec::new();
        self.png_previous_row = Vec::new();
    }

    /// Approximate bytes of reusable heap storage currently retained.
    pub fn retained_bytes(&self) -> usize {
        32_768usize
            .saturating_add(self.png_chunk.capacity())
            .saturating_add(self.png_previous_row.capacity())
    }

    pub(crate) fn enforce_retention_limit(&mut self) {
        let variable = self.png_chunk.capacity().saturating_add(self.png_previous_row.capacity());
        if variable > self.max_retained_bytes {
            self.shrink();
        }
    }
}
