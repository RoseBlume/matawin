//! Implements the `events` module.
//!
//! This module documents both public API items and internal implementation details.

use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW,
    PeekMessageW,
    TranslateMessage,
    MSG,
    PM_REMOVE,
    WM_SIZE,
};

impl super::PlatformWindow {
    /// Performs the pump events operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the pump events operation. Visibility: public."]
    pub fn pump_events(&self) {
        let mut msg = MSG::default();

        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                // Track client-size changes without a GetClientRect call in the
                // per-frame presentation hot path.
                if msg.hwnd == self.hwnd && msg.message == WM_SIZE {
                    let raw = msg.lParam.0 as usize;
                    let width = (raw & 0xffff) as u32;
                    let height = ((raw >> 16) & 0xffff) as u32;
                    self.client_size.set((width, height));
                }

                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
