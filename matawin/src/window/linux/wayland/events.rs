//! Implements the `events` module.
//!
//! This module documents both public API items and internal implementation details.

impl super::PlatformWindow {
    /// Pumps all currently available Wayland events without blocking.
    ///
    /// Pending events are dispatched first. The queue then repeatedly prepares a
    /// Wayland socket read and calls `ReadEventsGuard::read()`. In the pure-Rust
    /// Wayland backend this read returns `WouldBlock` when the socket currently has
    /// no data, so the loop can drain ready events without a separate polling crate.
    #[doc = "Implements the pump events operation. Visibility: public."]
    pub fn pump_events(&self) {
        let mut queue = self.event_queue.borrow_mut();
        let mut state = self.state.borrow_mut();

        let _ = queue.dispatch_pending(&mut state);
        let _ = queue.flush();

        loop {
            let Some(guard) = queue.prepare_read() else {
                let dispatched = queue.dispatch_pending(&mut state).unwrap_or(0);
                if dispatched == 0 {
                    break;
                }
                continue;
            };

            match guard.read() {
                Ok(_) => {
                    let _ = queue.dispatch_pending(&mut state);
                }
                Err(_) => {
                    // With no immediately available socket data the Rust Wayland
                    // backend reports WouldBlock. Other read failures likewise end
                    // this pump; a later call can retry or the connection will fail
                    // through the normal Wayland error path.
                    break;
                }
            }
        }
    }
}
