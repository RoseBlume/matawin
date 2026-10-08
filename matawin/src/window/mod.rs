//! Platform-independent native window wrapper.
//!
//! On Linux the platform layer tries Wayland first and automatically falls
//! back to X11 when Wayland initialization fails, provided both features are
//! enabled.

#![allow(non_snake_case)]

use std::{ cell::Cell, ffi::c_void };

use crate::{ buffer::Framebuffer, events::{ Handlers, Key, WindowEvent, EVENT_QUEUE, KEYS_DOWN } };

#[cfg(target_os = "windows")]
/// Contains the windows implementation.
///
/// Visibility: **private**.
mod windows;
#[cfg(target_os = "windows")]
use windows::PlatformWindow;

#[cfg(target_os = "linux")]
/// Contains the linux implementation.
///
/// Visibility: **private**.
mod linux;
#[cfg(target_os = "linux")]
use linux::PlatformWindow;

#[cfg(target_os = "android")]
/// Contains the Android implementation.
///
/// Visibility: **private**.
mod android;
#[cfg(target_os = "android")]
use android::PlatformWindow;
#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "android")))]
compile_error!("matawin currently supports Windows, Linux, and Android only");

/// A native top-level application window.
///
/// This type owns only window-system state. Rendering/game state should be
/// stored by the caller (or by `mata-engine`) and submitted with [`present`].
pub struct Window {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: PlatformWindow,
    /// Stores size.
    ///
    /// Visibility: **private**.
    size: Cell<(u32, u32)>,
    /// Stores event scratch.
    ///
    /// Visibility: **private**.
    event_scratch: Vec<WindowEvent>,
    /// Stores handlers.
    ///
    /// Visibility: **public**.
    pub handlers: Handlers,
}

impl Window {
    /// Creates a native window with the requested title and client size.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        Self {
            inner: PlatformWindow::new(title, size),
            size: Cell::new(size),
            event_scratch: Vec::with_capacity(16),
            handlers: Handlers::default(),
        }
    }

    #[cfg(target_os = "android")]
    /// Wraps the Android Activity and its lifecycle-owned native surface.
    ///
    /// Call this from `android_main(app: AndroidApp)` instead of [`Window::new`].
    /// The returned window keeps a cheap clone of `AndroidApp` and must be polled
    /// regularly so Android lifecycle callbacks are serviced promptly.
    #[doc = "Creates an Android window from AndroidApp. Visibility: public."]
    pub fn from_android_app(app: AndroidApp, title: &str, size: (u32, u32)) -> Self {
        let inner = PlatformWindow::from_android_app(app, title, size);
        let native_size = inner.client_size();
        Self {
            inner,
            size: Cell::new(if native_size.0 != 0 && native_size.1 != 0 { native_size } else { size }),
            event_scratch: Vec::with_capacity(16),
            handlers: Handlers::default(),
        }
    }

    /// Presents a software framebuffer to the native window.
    ///
    /// The low-level window API intentionally retains support for scaling a
    /// framebuffer whose dimensions differ from the current client area. The
    /// widget host uses `present_exact` instead so UI frames are never upscaled.
    #[inline]
    #[doc = "Implements the present operation. Visibility: public."]
    pub fn present(&mut self, framebuffer: &Framebuffer) {
        self.inner.present(framebuffer);
    }

    /// Presents only when the framebuffer exactly matches the current native
    /// client size. Returns `false` when the window changed size after the frame
    /// was rasterized, allowing WidgetWindow to rerender instead of scaling it.
    #[doc = "Implements the present exact operation. Visibility: crate-visible."]
    pub(crate) fn present_exact(&mut self, framebuffer: &Framebuffer) -> bool {
        let client = self.inner.client_size();
        if client.0 == 0 || client.1 == 0 {
            return true;
        }
        if client != (framebuffer.width, framebuffer.height) {
            return false;
        }
        self.inner.present(framebuffer);
        true
    }

    /// Pumps native events into a caller-owned buffer.
    ///
    /// This is the fastest polling API for a game/render loop because the same
    /// `Vec` capacity can be reused every frame. The buffer is cleared first.
    #[doc = "Implements the poll events into operation. Visibility: public."]
    pub fn poll_events_into(&self, events: &mut Vec<WindowEvent>) {
        self.inner.pump_events();
        events.clear();

        if let Some(queue) = EVENT_QUEUE.get() {
            let mut queue = queue.lock().unwrap();
            events.reserve(queue.len());
            events.append(&mut *queue);
        }

        if let Some(keys) = KEYS_DOWN.get() {
            let keys = keys.lock().unwrap();
            events.reserve(keys.len());
            for &key in keys.iter() {
                events.push(WindowEvent::KeyHeld(key));
            }
        }

        for event in events.iter().copied() {
            match event {
                | WindowEvent::Resized(width, height)
                | WindowEvent::Maximized(width, height)
                | WindowEvent::Restored(width, height) => self.size.set((width, height)),
                _ => {}
            }
        }
    }

    /// Pumps native events and returns an owned event vector.
    ///
    /// Prefer [`Self::poll_events_into`] in performance-sensitive loops to avoid
    /// allocating a new vector every frame.
    #[doc = "Implements the poll events operation. Visibility: public."]
    pub fn poll_events(&self) -> Vec<WindowEvent> {
        let mut events = Vec::with_capacity(16);
        self.poll_events_into(&mut events);
        events
    }

    /// Returns whether a key/button is currently held.
    #[inline]
    #[doc = "Returns whether key down. Visibility: public."]
    pub fn is_key_down(&self, key: Key) -> bool {
        KEYS_DOWN.get()
            .map(|keys| keys.lock().unwrap().contains(&key))
            .unwrap_or(false)
    }

    /// Pumps native events and drains queued touch events into a reusable buffer.
    ///
    /// The buffer is cleared before new events are appended. On Android, each
    /// touch uses a `device_id` of `"device:pointer"` so simultaneous contacts
    /// from the same touchscreen remain distinguishable.
    #[doc = "Implements the poll touches into operation. Visibility: public."]
    pub fn poll_touches_into(&self, touches: &mut Vec<crate::events::Touch>) {
        self.inner.pump_events();
        touches.clear();
        if let Some(queue) = crate::events::TOUCH_QUEUE.get() {
            let mut queue = queue.lock().unwrap();
            touches.reserve(queue.len());
            touches.append(&mut *queue);
        }
    }

    /// Pumps native events and returns queued touch events.
    #[doc = "Implements the poll touches operation. Visibility: public."]
    pub fn poll_touches(&self) -> Vec<crate::events::Touch> {
        let mut touches = Vec::with_capacity(8);
        self.poll_touches_into(&mut touches);
        touches
    }

    /// Pumps events and invokes handlers registered on this window.
    #[doc = "Implements the handle events operation. Visibility: public."]
    pub fn handle_events(&mut self) {
        let mut events = std::mem::take(&mut self.event_scratch);
        self.poll_events_into(&mut events);

        for event in events.iter().copied() {
            match event {
                WindowEvent::Close => {
                    if let Some(callback) = self.handlers.on_close.as_mut() {
                        callback();
                    }
                }
                WindowEvent::Resized(width, height) => {
                    if let Some(callback) = self.handlers.on_resized.as_mut() {
                        callback(width, height);
                    }
                }
                WindowEvent::Maximized(width, height) => {
                    if let Some(callback) = self.handlers.on_maximized.as_mut() {
                        callback(width, height);
                    }
                }
                WindowEvent::Restored(width, height) => {
                    if let Some(callback) = self.handlers.on_restored.as_mut() {
                        callback(width, height);
                    }
                }
                WindowEvent::Minimized => {
                    if let Some(callback) = self.handlers.on_minimized.as_mut() {
                        callback();
                    }
                }
                WindowEvent::Repositioned(x, y) => {
                    if let Some(callback) = self.handlers.on_repositioned.as_mut() {
                        callback(x, y);
                    }
                }
                WindowEvent::KeyPressed(key) => {
                    if let Some(callback) = self.handlers.on_key_pressed.as_mut() {
                        callback(key);
                    }
                }
                WindowEvent::KeyHeld(key) => {
                    if let Some(callback) = self.handlers.on_key_held.as_mut() {
                        callback(key);
                    }
                }
                WindowEvent::KeyReleased(key) => {
                    if let Some(callback) = self.handlers.on_key_released.as_mut() {
                        callback(key);
                    }
                }
                WindowEvent::CursorMoved(x, y) => {
                    if let Some(callback) = self.handlers.on_cursor_moved.as_mut() {
                        callback(x, y);
                    }
                }
                WindowEvent::Scroll(x, y) => {
                    if let Some(callback) = self.handlers.on_scroll.as_mut() {
                        callback(x, y);
                    }
                }
            }
        }

        events.clear();
        self.event_scratch = events;
    }

    /// Returns the value configured with close handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with close handler. Visibility: public."]
    pub fn with_close_handler<F: FnMut() + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_close = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with resize handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with resize handler. Visibility: public."]
    pub fn with_resize_handler<F: FnMut(u32, u32) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_resized = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with maximized handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with maximized handler. Visibility: public."]
    pub fn with_maximized_handler<F: FnMut(u32, u32) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_maximized = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with restored handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with restored handler. Visibility: public."]
    pub fn with_restored_handler<F: FnMut(u32, u32) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_restored = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with minimized handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with minimized handler. Visibility: public."]
    pub fn with_minimized_handler<F: FnMut() + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_minimized = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with repositioned handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with repositioned handler. Visibility: public."]
    pub fn with_repositioned_handler<F: FnMut(i32, i32) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_repositioned = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with key pressed handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with key pressed handler. Visibility: public."]
    pub fn with_key_pressed_handler<F: FnMut(Key) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_key_pressed = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with key held handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with key held handler. Visibility: public."]
    pub fn with_key_held_handler<F: FnMut(Key) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_key_held = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with key released handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with key released handler. Visibility: public."]
    pub fn with_key_released_handler<F: FnMut(Key) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_key_released = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with cursor moved handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with cursor moved handler. Visibility: public."]
    pub fn with_cursor_moved_handler<F: FnMut(i32, i32) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_cursor_moved = Some(Box::new(callback));
        self
    }

    /// Returns the value configured with scroll handler.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with scroll handler. Visibility: public."]
    pub fn with_scroll_handler<F: FnMut(i32, i32) + 'static>(mut self, callback: F) -> Self {
        self.handlers.on_scroll = Some(Box::new(callback));
        self
    }

    /// Converts native client coordinates into logical framebuffer coordinates.
    #[inline]
    #[doc = "Implements the client to framebuffer operation. Visibility: public."]
    pub fn client_to_framebuffer(&self, x: i32, y: i32) -> (i32, i32) {
        let (fw, fh) = self.inner.framebuffer_size();
        let (cw, ch) = self.inner.client_size();
        if cw == 0 || ch == 0 {
            return (x, y);
        }
        let sx = (((x as i64) * (fw as i64)) / (cw as i64)) as i32;
        let sy = (((y as i64) * (fh as i64)) / (ch as i64)) as i32;
        (sx, sy)
    }

    #[inline]
    /// Performs the get size operation.
    ///
    /// Visibility: **public**.
    #[doc = "Returns size. Visibility: public."]
    pub fn get_size(&self) -> (usize, usize) {
        let (width, height) = self.size.get();
        (width as usize, height as usize)
    }

    #[inline]
    /// Sets title.
    ///
    /// Visibility: **public**.
    #[doc = "Sets title. Visibility: public."]
    pub fn set_title(&self, title: &str) {
        self.inner.set_title(title);
    }

    #[inline]
    /// Sets icon.
    ///
    /// Visibility: **public**.
    #[doc = "Sets icon. Visibility: public."]
    pub fn set_icon(&self, path: &str) {
        self.inner.set_icon(path);
    }

    #[cfg(target_os = "linux")]
    #[inline]
    /// Performs the backend name operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the backend name operation. Visibility: public."]
    pub fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }

    #[inline]
    /// Performs the hide operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the hide operation. Visibility: public."]
    pub fn hide(&self) {
        self.inner.hide();
    }

    #[inline]
    /// Performs the show operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the show operation. Visibility: public."]
    pub fn show(&self) {
        self.inner.show();
    }

    #[inline]
    /// Performs the maximize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the maximize operation. Visibility: public."]
    pub fn maximize(&self) {
        self.inner.maximize();
    }

    #[inline]
    /// Performs the minimize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the minimize operation. Visibility: public."]
    pub fn minimize(&self) {
        self.inner.minimize();
    }

    #[inline]
    /// Performs the restore operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the restore operation. Visibility: public."]
    pub fn restore(&self) {
        self.inner.restore();
    }

    #[inline]
    /// Performs the reposition operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the reposition operation. Visibility: public."]
    pub fn reposition(&self, x: i32, y: i32) {
        self.inner.reposition(x, y);
    }

    #[inline]
    /// Performs the resize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resize operation. Visibility: public."]
    pub fn resize(&self, width: u32, height: u32) {
        self.inner.resize(width, height);
    }

    #[inline]
    /// Sets decorations.
    ///
    /// Visibility: **public**.
    #[doc = "Sets decorations. Visibility: public."]
    pub fn set_decorations(&self, enabled: bool) {
        self.inner.set_decorations(enabled);
    }

    #[inline]
    /// Sets maximizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets maximizable. Visibility: public."]
    pub fn set_maximizable(&self, enabled: bool) {
        self.inner.set_maximizable(enabled);
    }

    #[inline]
    /// Sets minimizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets minimizable. Visibility: public."]
    pub fn set_minimizable(&self, enabled: bool) {
        self.inner.set_minimizable(enabled);
    }

    #[inline]
    /// Sets resizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resizable. Visibility: public."]
    pub fn set_resizable(&self, enabled: bool) {
        self.inner.set_resizable(enabled);
    }

    /// Returns the platform-specific opaque native handle.
    #[inline]
    #[doc = "Implements the handle operation. Visibility: public."]
    pub fn handle(&self) -> *mut c_void {
        self.inner.handle()
    }

    #[inline]
    /// Performs the framebuffer size operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the framebuffer size operation. Visibility: public."]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        self.inner.framebuffer_size()
    }

    #[inline]
    /// Computes client size.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the client size operation. Visibility: public."]
    pub fn client_size(&self) -> (u32, u32) {
        self.inner.client_size()
    }
}
