//! Widget identifiers, public events, errors, and the cross-widget event queue.

use std::sync::{ Mutex, OnceLock };
use std::sync::atomic::{ AtomicU64, Ordering };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Stores the state and data for widget id.
///
/// Visibility: **public**.
pub struct WidgetId(
    /// Stores the numeric widget identifier.
    ///
    /// Visibility: **crate-visible field of a public tuple struct**.
    pub(crate) u64,
);
impl WidgetId {
    #[inline]
    #[doc = "Implements the get operation. Visibility: public."]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Defines the available widget event choices.
///
/// Visibility: **public**.
pub enum WidgetEvent {
    /// Represents the close variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Close,
    /// Represents the resized variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Resized(u32, u32),
    /// Represents the button clicked variant.
    ///
    /// Visibility: inherited from the **public** enum.
    ButtonClicked(WidgetId),
    /// Represents the text changed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    TextChanged(WidgetId),
    /// Represents the text field activated variant.
    ///
    /// Visibility: inherited from the **public** enum.
    TextFieldActivated(WidgetId),
    #[cfg(feature = "webview")]
    /// Represents the web view ready variant.
    ///
    /// Visibility: inherited from the **public** enum.
    WebViewReady(WidgetId),
    #[cfg(feature = "webview")]
    /// Represents the web view error variant.
    ///
    /// Visibility: inherited from the **public** enum.
    WebViewError(WidgetId, String),
}

#[derive(Debug, Clone)]
/// Stores the state and data for widget error.
///
/// Visibility: **public**.
pub struct WidgetError(
    /// Stores the human-readable widget error message.
    ///
    /// Visibility: **public**.
    pub String,
);
impl std::fmt::Display for WidgetError {
    #[doc = "Implements the fmt operation. Visibility: private/internal."]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for WidgetError {}

#[derive(Debug, Clone)]
/// Stores the state and data for queued event.
///
/// Visibility: **crate-visible**.
pub(crate) struct QueuedEvent {
    /// Stores host id.
    ///
    /// Visibility: **public**.
    pub host_id: u64,
    /// Stores event.
    ///
    /// Visibility: **public**.
    pub event: WidgetEvent,
}
/// Static storage used for next id.
///
/// Visibility: **private**.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// Static storage used for event queue.
///
/// Visibility: **private**.
static EVENT_QUEUE: OnceLock<Mutex<Vec<QueuedEvent>>> = OnceLock::new();
#[inline]
#[doc = "Implements the next id operation. Visibility: crate-visible."]
pub(crate) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
#[inline]
#[doc = "Pushes event. Visibility: crate-visible."]
pub(crate) fn push_event(host_id: u64, event: WidgetEvent) {
    EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::with_capacity(32)))
        .lock()
        .unwrap()
        .push(QueuedEvent { host_id, event });
}
/// Implements drain events.
///
/// Visibility: **crate-visible**.
#[doc = "Implements the drain events operation. Visibility: crate-visible."]
pub(crate) fn drain_events(host_id: u64, out: &mut Vec<WidgetEvent>) {
    let mut q = EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::with_capacity(32)))
        .lock()
        .unwrap();
    let mut i = 0;
    while i < q.len() {
        if q[i].host_id == host_id {
            out.push(q.swap_remove(i).event);
        } else {
            i += 1;
        }
    }
}
