//! Shared performance policy used by codec hot paths.
//!
//! Expensive operations use centralized, documented thresholds so individual
//! decoders do not grow their own ad-hoc thread heuristics. The P1 benchmark
//! suite exercises these cutovers on 1/2/4/8 logical CPUs.

use std::sync::OnceLock;

/// Process-wide hardware parallelism cache.
static AVAILABLE_THREADS: OnceLock<usize> = OnceLock::new();

/// Work categories with independently tunable parallel cutovers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkKind {
    /// Nearest-neighbour rescaling of independent destination rows.
    Rescale,
}

impl WorkKind {
    /// Minimum amount of pixel work before OS-thread creation is considered.
    ///
    /// These constants are intentionally centralized so benchmark-driven tuning
    /// only changes one location. Small jobs always remain single-threaded.
    #[inline]
    pub(crate) const fn minimum_pixels(self) -> usize {
        match self {
            Self::Rescale => 4_000_000,
        }
    }
}

/// Returns hardware parallelism without repeatedly querying the operating system.
#[inline]
pub(crate) fn available_threads() -> usize {
    *AVAILABLE_THREADS.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .max(1)
    })
}

/// Chooses a bounded worker count for a row-oriented operation.
#[inline]
pub(crate) fn worker_count(max_threads: Option<usize>, rows: usize) -> usize {
    let requested = max_threads.unwrap_or_else(available_threads).max(1);
    requested.min(rows.max(1))
}

/// Chooses a worker count only when a measured work threshold has been reached.
#[inline]
pub(crate) fn worker_count_for(
    kind: WorkKind,
    max_threads: Option<usize>,
    pixels: usize,
    rows: usize,
) -> usize {
    if pixels < kind.minimum_pixels() {
        1
    } else {
        worker_count(max_threads, rows)
    }
}
