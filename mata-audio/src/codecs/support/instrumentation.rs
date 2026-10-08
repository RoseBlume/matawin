//! Optional low-overhead stage timing used by the performance harness.

#[cfg(feature = "perf-instrumentation")]
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[cfg(feature = "perf-instrumentation")]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StageStats {
    pub(crate) calls: u64,
    pub(crate) total: Duration,
    pub(crate) max: Duration,
}

#[cfg(feature = "perf-instrumentation")]
static STAGES: OnceLock<Mutex<BTreeMap<&'static str, StageStats>>> = OnceLock::new();

#[cfg(feature = "perf-instrumentation")]
pub(crate) struct StageTimer {
    name: &'static str,
    start: Instant,
}

#[cfg(feature = "perf-instrumentation")]
impl StageTimer {
    #[inline]
    pub(crate) fn start(name: &'static str) -> Self {
        Self { name, start: Instant::now() }
    }
}

#[cfg(feature = "perf-instrumentation")]
impl Drop for StageTimer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        let lock = STAGES.get_or_init(|| Mutex::new(BTreeMap::new()));
        if let Ok(mut map) = lock.lock() {
            let stats = map.entry(self.name).or_default();
            stats.calls = stats.calls.saturating_add(1);
            stats.total = stats.total.saturating_add(elapsed);
            stats.max = stats.max.max(elapsed);
        }
    }
}

#[cfg(feature = "perf-instrumentation")]
pub(crate) fn snapshot() -> Vec<(&'static str, StageStats)> {
    STAGES
        .get()
        .and_then(|m| m.lock().ok())
        .map(|m| m.iter().map(|(&name, &stats)| (name, stats)).collect())
        .unwrap_or_default()
}

#[cfg(feature = "perf-instrumentation")]
pub(crate) fn clear() {
    if let Some(lock) = STAGES.get() {
        if let Ok(mut map) = lock.lock() { map.clear(); }
    }
}

#[cfg(not(feature = "perf-instrumentation"))]
pub(crate) struct StageTimer;

#[cfg(not(feature = "perf-instrumentation"))]
impl StageTimer {
    #[inline(always)]
    pub(crate) fn start(_: &'static str) -> Self { Self }
}
