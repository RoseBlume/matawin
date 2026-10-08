//! Implements the `show` module.
//!
//! This module documents both public API items and internal implementation details.

use crate::events::WindowEvent;

impl super::PlatformWindow {
    /// Unmaps the surface by committing a null buffer.
    #[doc = "Implements the hide operation. Visibility: public."]
    pub fn hide(&self) {
        if self.hidden.get() {
            return;
        }

        self.surface.attach(None, 0, 0);
        self.surface.commit();
        self.hidden.set(true);
        let _ = self.connection.flush();
    }

    /// Wayland does not define a broadly-supported API for loading a per-window
    /// icon from an arbitrary file path. Desktop environments normally obtain the
    /// application icon from the app-id/desktop entry, so this compatibility method
    /// intentionally leaves the compositor-managed icon unchanged.
    #[doc = "Sets icon. Visibility: public."]
    pub fn set_icon(&self, _path: &str) {}

    /// Performs the maximize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the maximize operation. Visibility: public."]
    pub fn maximize(&self) {
        self.toplevel.set_maximized();
        self.surface.commit();
        let _ = self.connection.flush();
    }

    /// Performs the minimize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the minimize operation. Visibility: public."]
    pub fn minimize(&self) {
        self.toplevel.set_minimized();
        self.surface.commit();
        super::push_event(WindowEvent::Minimized);
        let _ = self.connection.flush();
    }

    /// Performs the restore operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the restore operation. Visibility: public."]
    pub fn restore(&self) {
        self.toplevel.unset_maximized();
        self.surface.commit();
        let _ = self.connection.flush();
    }

    /// Wayland intentionally does not allow normal clients to choose absolute
    /// top-level screen coordinates. The compositor is responsible for placement,
    /// so this method is a no-op retained for API compatibility.
    #[doc = "Implements the reposition operation. Visibility: public."]
    pub fn reposition(&self, _x: i32, _y: i32) {}

    /// Requests a new preferred content size.
    ///
    /// XDG shell has no Win32-style "set the outer window to exactly W×H" request.
    /// Changing the buffer size is the client-side size hint. If resizing has been
    /// disabled, the min/max constraints are also updated to lock this size.
    #[doc = "Implements the resize operation. Visibility: public."]
    pub fn resize(&self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.state.borrow_mut().client_size = (width, height);
        if !self.resizable.get() {
            self.toplevel.set_min_size(width as i32, height as i32);
            self.toplevel.set_max_size(width as i32, height as i32);
        }

        self.surface.commit();
        super::push_event(WindowEvent::Resized(width, height));
        let _ = self.connection.flush();
    }

    /// Marks the surface visible again. Actual mapping occurs on the next
    /// `present`, when a real buffer is attached and committed.
    #[doc = "Implements the show operation. Visibility: public."]
    pub fn show(&self) {
        self.hidden.set(false);
    }

    /// Decoration policy is compositor-specific on Wayland. This low-level backend
    /// requests the default compositor behavior and therefore cannot guarantee an
    /// exact Win32-style decoration toggle without an optional decoration protocol.
    #[doc = "Sets decorations. Visibility: public."]
    pub fn set_decorations(&self, _enabled: bool) {}

    /// Standard XDG shell does not expose a client-side "disable maximize button"
    /// capability toggle. This method is retained for cross-platform API parity.
    #[doc = "Sets maximizable. Visibility: public."]
    pub fn set_maximizable(&self, _enabled: bool) {}

    /// Standard XDG shell does not expose a client-side "disable minimize button"
    /// capability toggle. This method is retained for cross-platform API parity.
    #[doc = "Sets minimizable. Visibility: public."]
    pub fn set_minimizable(&self, _enabled: bool) {}

    /// Enables or disables interactive resizing by changing XDG min/max size hints.
    #[doc = "Sets resizable. Visibility: public."]
    pub fn set_resizable(&self, enabled: bool) {
        self.resizable.set(enabled);

        if enabled {
            self.toplevel.set_min_size(0, 0);
            self.toplevel.set_max_size(0, 0);
        } else {
            let (width, height) = self.state.borrow().client_size;
            self.toplevel.set_min_size(width as i32, height as i32);
            self.toplevel.set_max_size(width as i32, height as i32);
        }

        self.surface.commit();
        let _ = self.connection.flush();
    }
}
