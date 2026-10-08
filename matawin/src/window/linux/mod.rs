//! Linux window backend selection.
//!
//! When the `wayland` feature is enabled, window creation always tries Wayland
//! first. If connecting to or initializing the Wayland compositor fails and the
//! `x11` feature is also enabled, creation transparently falls back to X11.
//! This is runtime selection: applications do not need to inspect `WAYLAND_DISPLAY`,
//! `XDG_SESSION_TYPE`, or `DISPLAY` themselves.

use std::ffi::c_void;

use crate::buffer::Framebuffer;

#[cfg(feature = "wayland")]
/// Contains the wayland implementation.
///
/// Visibility: **private**.
mod wayland;

#[cfg(feature = "x11")]
/// Contains the x11 implementation.
///
/// Visibility: **private**.
mod x11;

#[cfg(not(any(feature = "wayland", feature = "x11")))]
compile_error!("Linux window support requires at least one of the `wayland` or `x11` features");

/// Runtime-selected Linux platform window.
///
/// With both Linux backends enabled, [`PlatformWindow::new`] attempts Wayland
/// first and automatically falls back to X11 if Wayland cannot be initialized.
/// Once constructed, calls are forwarded to the selected backend with no further
/// backend detection on the hot rendering/event paths.
pub struct PlatformWindow {
    /// Stores backend.
    ///
    /// Visibility: **private**.
    backend: Backend,
}

/// Defines the available backend choices.
///
/// Visibility: **private**.
enum Backend {
    #[cfg(feature = "wayland")]
    /// Represents the wayland variant.
    ///
    /// Visibility: inherited from the **private** enum.
    Wayland(wayland::PlatformWindow),

    #[cfg(feature = "x11")]
    /// Represents the x11 variant.
    ///
    /// Visibility: inherited from the **private** enum.
    X11(x11::PlatformWindow),
}

impl PlatformWindow {
    /// Creates a Linux window, preferring Wayland and falling back to X11.
    ///
    /// The Wayland attempt is made whenever the `wayland` feature is compiled in.
    /// A failure to connect to the compositor, discover required globals, or
    /// complete initial XDG-shell configuration is treated as a recoverable
    /// backend failure. If `x11` is enabled, X11 is then attempted immediately.
    ///
    /// If every compiled-in backend fails, this function panics with the errors
    /// from each attempted backend. Neither backend requires `sudo` privileges.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        let mut errors = Vec::<String>::new();

        #[cfg(feature = "wayland")]
        match wayland::PlatformWindow::try_new(title, size) {
            Ok(window) => {
                return Self { backend: Backend::Wayland(window) };
            }
            Err(error) => errors.push(format!("Wayland: {error}")),
        }

        #[cfg(feature = "x11")]
        match x11::PlatformWindow::try_new(title, size) {
            Ok(window) => {
                return Self { backend: Backend::X11(window) };
            }
            Err(error) => errors.push(format!("X11: {error}")),
        }

        panic!("Failed to create a Linux window using any enabled backend: {}", errors.join("; "));
    }

    /// Returns the backend selected for this window.
    #[doc = "Implements the backend name operation. Visibility: public."]
    pub fn backend_name(&self) -> &'static str {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(_) => "wayland",
            #[cfg(feature = "x11")]
            Backend::X11(_) => "x11",
        }
    }

    /// Performs the present operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the present operation. Visibility: public."]
    pub fn present(&mut self, fb: &Framebuffer) {
        match &mut self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.present(fb),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.present(fb),
        }
    }

    /// Performs the pump events operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the pump events operation. Visibility: public."]
    pub fn pump_events(&self) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.pump_events(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.pump_events(),
        }
    }

    /// Sets title.
    ///
    /// Visibility: **public**.
    #[doc = "Sets title. Visibility: public."]
    pub fn set_title(&self, title: &str) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.set_title(title),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.set_title(title),
        }
    }

    /// Sets icon.
    ///
    /// Visibility: **public**.
    #[doc = "Sets icon. Visibility: public."]
    pub fn set_icon(&self, path: &str) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.set_icon(path),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.set_icon(path),
        }
    }

    /// Performs the hide operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the hide operation. Visibility: public."]
    pub fn hide(&self) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.hide(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.hide(),
        }
    }

    /// Performs the maximize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the maximize operation. Visibility: public."]
    pub fn maximize(&self) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.maximize(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.maximize(),
        }
    }

    /// Performs the minimize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the minimize operation. Visibility: public."]
    pub fn minimize(&self) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.minimize(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.minimize(),
        }
    }

    /// Performs the restore operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the restore operation. Visibility: public."]
    pub fn restore(&self) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.restore(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.restore(),
        }
    }

    /// Performs the reposition operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the reposition operation. Visibility: public."]
    pub fn reposition(&self, x: i32, y: i32) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.reposition(x, y),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.reposition(x, y),
        }
    }

    /// Performs the resize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resize operation. Visibility: public."]
    pub fn resize(&self, width: u32, height: u32) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.resize(width, height),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.resize(width, height),
        }
    }

    /// Performs the show operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the show operation. Visibility: public."]
    pub fn show(&self) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.show(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.show(),
        }
    }

    /// Sets decorations.
    ///
    /// Visibility: **public**.
    #[doc = "Sets decorations. Visibility: public."]
    pub fn set_decorations(&self, enabled: bool) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.set_decorations(enabled),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.set_decorations(enabled),
        }
    }

    /// Sets maximizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets maximizable. Visibility: public."]
    pub fn set_maximizable(&self, enabled: bool) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.set_maximizable(enabled),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.set_maximizable(enabled),
        }
    }

    /// Sets minimizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets minimizable. Visibility: public."]
    pub fn set_minimizable(&self, enabled: bool) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.set_minimizable(enabled),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.set_minimizable(enabled),
        }
    }

    /// Sets resizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resizable. Visibility: public."]
    pub fn set_resizable(&self, enabled: bool) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.set_resizable(enabled),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.set_resizable(enabled),
        }
    }

    /// Performs the handle operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the handle operation. Visibility: public."]
    pub fn handle(&self) -> *mut c_void {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.handle(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.handle(),
        }
    }

    /// Performs the framebuffer size operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the framebuffer size operation. Visibility: public."]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.framebuffer_size(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.framebuffer_size(),
        }
    }

    /// Computes client size.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the client size operation. Visibility: public."]
    pub fn client_size(&self) -> (u32, u32) {
        match &self.backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland(window) => window.client_size(),
            #[cfg(feature = "x11")]
            Backend::X11(window) => window.client_size(),
        }
    }
}
