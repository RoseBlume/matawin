//! Android native window backend.
//!
//! Android owns the application's top-level surface through its Activity. MataWin
//! therefore does not create a second native window. Instead, an [`AndroidApp`]
//! supplied to [`crate::Window::from_android_app`] is retained by this backend and
//! its current `ANativeWindow` is used for software framebuffer presentation.
//!
//! Lifecycle and input are pumped through `android-activity`. Touch input is
//! exposed through MataWin's touch queue and the primary pointer is mirrored to
//! the existing mouse-style events so the software widget layer works without a
//! separate Android-only input path.

use std::{
    cell::Cell,
    ffi::c_void,
    ptr,
    time::Duration,
};

use android_activity::{
    input::{Axis, InputEvent, KeyAction, Keycode, MotionAction, ToolType},
    AndroidApp,
    InputStatus,
    MainEvent,
    PollEvent,
};
use ndk::hardware_buffer_format::HardwareBufferFormat;

use crate::{
    buffer::Framebuffer,
    events::{
        Key,
        Touch,
        TouchEvent,
        WindowEvent,
        EVENT_QUEUE,
        KEYS_DOWN,
        TOUCH_QUEUE,
    },
};

/// Android platform state for a MataWin window.
///
/// `AndroidApp` is cheaply clonable and is kept here instead of in global state,
/// matching Android's Activity lifecycle. Applications should drop the `Window`
/// and return from `android_main` after receiving [`WindowEvent::Close`].
pub struct PlatformWindow {
    /// Activity/lifecycle handle supplied by `android_main`.
    app: AndroidApp,
    /// Most recently presented logical framebuffer dimensions.
    framebuffer_size: Cell<(u32, u32)>,
    /// Last known physical Activity surface dimensions.
    client_size: Cell<(u32, u32)>,
    /// Whether MataWin presentation is locally suspended.
    hidden: Cell<bool>,
    /// Whether Android delivered the terminal Activity destroy event.
    destroyed: Cell<bool>,
}

impl PlatformWindow {
    /// Android cannot create an Activity from a title and size alone.
    ///
    /// This method exists so the cross-platform implementation remains complete,
    /// but callers on Android must use `Window::from_android_app` instead.
    pub fn new(_title: &str, _size: (u32, u32)) -> Self {
        panic!(
            "Window::new cannot create an Android Activity; use \
             Window::from_android_app(app, title, size) from android_main"
        );
    }

    /// Creates a MataWin wrapper around the Activity represented by `app`.
    pub fn from_android_app(app: AndroidApp, _title: &str, size: (u32, u32)) -> Self {
        // Scroll axes are disabled by default in android-activity to keep motion
        // event copying inexpensive. Enable the two axes MataWin understands.
        app.enable_motion_axis(Axis::Vscroll);
        app.enable_motion_axis(Axis::Hscroll);

        let client_size = app
            .native_window()
            .map(|window| native_size(&window))
            .unwrap_or(size);

        Self {
            app,
            framebuffer_size: Cell::new(size),
            client_size: Cell::new(client_size),
            hidden: Cell::new(false),
            destroyed: Cell::new(false),
        }
    }

    /// Presents the software framebuffer through the current `ANativeWindow`.
    pub fn present(&mut self, fb: &Framebuffer) {
        if self.hidden.get() || self.destroyed.get() || fb.width == 0 || fb.height == 0 {
            return;
        }

        let Some(window) = self.app.native_window() else {
            return;
        };

        // Keep Android's base surface dimensions so client_size() continues to
        // report the physical Activity surface after rotations and resizes. The
        // software framebuffer is scaled during the copy when its logical size
        // differs from the native surface. Passing zero for both dimensions
        // restores the base geometry while selecting RGBA_8888.
        if window
            .set_buffers_geometry(0, 0, Some(HardwareBufferFormat::R8G8B8A8_UNORM))
            .is_err()
        {
            return;
        }

        let native = native_size(&window);
        let Ok(mut surface) = window.lock(None) else {
            return;
        };

        if surface.format() != HardwareBufferFormat::R8G8B8A8_UNORM {
            return;
        }

        let dst_width = surface.width();
        let dst_height = surface.height();
        if dst_width == 0 || dst_height == 0 {
            return;
        }

        let stride = surface.stride();
        let dst = surface.bits().cast::<u8>();
        let src = fb.as_slice();
        let src_width = fb.width as usize;
        let src_height = fb.height as usize;

        // MataWin stores pixels as 0xAARRGGBB. Android RGBA_8888 expects R, G,
        // B, A bytes. Keep the overwhelmingly common exact-size path free of
        // scaling divisions; low-level callers that intentionally submit a
        // different-sized framebuffer use nearest-neighbour sampling.
        if src_width == dst_width && src_height == dst_height {
            for y in 0..dst_height {
                let src_row = &src[y * src_width..][..src_width];
                let dst_row = unsafe { dst.add(y * stride * 4) };
                for (x, &pixel) in src_row.iter().enumerate() {
                    write_rgba8888(unsafe { dst_row.add(x * 4) }, pixel);
                }
            }
        } else {
            for y in 0..dst_height {
                let src_y = y.saturating_mul(src_height) / dst_height;
                let dst_row = unsafe { dst.add(y * stride * 4) };
                for x in 0..dst_width {
                    let src_x = x.saturating_mul(src_width) / dst_width;
                    let pixel = src[src_y * src_width + src_x];
                    write_rgba8888(unsafe { dst_row.add(x * 4) }, pixel);
                }
            }
        }

        self.framebuffer_size.set((fb.width, fb.height));
        if native.0 != 0 && native.1 != 0 {
            self.client_size.set(native);
        }
        // `surface` unlocks and posts the frame when dropped.
    }

    /// Polls Android lifecycle and input without blocking.
    pub fn pump_events(&self) {
        if self.destroyed.get() {
            return;
        }

        // poll_events processes one looper result per call. Drain a bounded
        // number so a normal MataWin frame consumes all immediately-ready
        // lifecycle messages without risking an infinite producer loop.
        for _ in 0..64 {
            let mut idle = false;
            self.app.poll_events(Some(Duration::ZERO), |event| match event {
                PollEvent::Timeout => idle = true,
                PollEvent::Main(event) => self.handle_main_event(event),
                PollEvent::Wake => {}
                _ => {}
            });

            self.drain_input_events();
            if idle || self.destroyed.get() {
                break;
            }
        }
    }

    /// Translates one Android lifecycle event into MataWin state/events.
    fn handle_main_event(&self, event: MainEvent<'_>) {
        match event {
            MainEvent::InitWindow { .. } | MainEvent::WindowResized { .. } => {
                self.refresh_client_size(true);
            }
            MainEvent::ContentRectChanged { .. } => {
                self.refresh_client_size(true);
            }
            MainEvent::TerminateWindow { .. } | MainEvent::Stop => {
                clear_keys();
                push_event(WindowEvent::Minimized);
            }
            MainEvent::Resume { .. } => {
                let size = self.current_native_size();
                if size.0 != 0 && size.1 != 0 {
                    self.client_size.set(size);
                    push_event(WindowEvent::Restored(size.0, size.1));
                }
            }
            MainEvent::Destroy => {
                self.destroyed.set(true);
                clear_keys();
                push_event(WindowEvent::Close);
            }
            MainEvent::LostFocus => clear_keys(),
            MainEvent::InputAvailable | MainEvent::RedrawNeeded { .. } => {}
            _ => {}
        }
    }

    /// Refreshes the cached native size and optionally emits `Resized`.
    fn refresh_client_size(&self, emit: bool) {
        let size = self.current_native_size();
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let previous = self.client_size.replace(size);
        if emit && previous != size {
            push_event(WindowEvent::Resized(size.0, size.1));
        }
    }

    /// Reads the current Activity surface dimensions if a surface exists.
    fn current_native_size(&self) -> (u32, u32) {
        self.app
            .native_window()
            .map(|window| native_size(&window))
            .unwrap_or((0, 0))
    }

    /// Drains Android's lending input iterator without allocating copies.
    fn drain_input_events(&self) {
        let Ok(mut iter) = self.app.input_events_iter() else {
            return;
        };

        loop {
            let read = iter.next(|event| self.handle_input_event(event));
            if !read {
                break;
            }
        }
    }

    /// Translates a single Android input event into MataWin events.
    fn handle_input_event(&self, event: &InputEvent<'_>) -> InputStatus {
        match event {
            InputEvent::KeyEvent(event) => {
                let Some(key) = map_key(event.key_code()) else {
                    return InputStatus::Unhandled;
                };

                match event.action() {
                    KeyAction::Down => {
                        if set_key_down(key, true) {
                            push_event(WindowEvent::KeyPressed(key));
                        }
                        InputStatus::Handled
                    }
                    KeyAction::Up => {
                        if set_key_down(key, false) {
                            push_event(WindowEvent::KeyReleased(key));
                        }
                        InputStatus::Handled
                    }
                    _ => InputStatus::Unhandled,
                }
            }
            InputEvent::MotionEvent(event) => {
                self.handle_motion_event(event);
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }
    }

    /// Handles touch, pointer movement, and wheel-style motion events.
    fn handle_motion_event(&self, event: &android_activity::input::MotionEvent<'_>) {
        let action = event.action();

        if matches!(action, MotionAction::Scroll) && event.pointer_count() != 0 {
            let pointer = event.pointer_at_index(0);
            let horizontal = (-pointer.axis_value(Axis::Hscroll) * 120.0).round() as i32;
            let vertical = (pointer.axis_value(Axis::Vscroll) * 120.0).round() as i32;
            if horizontal != 0 || vertical != 0 {
                push_event(WindowEvent::Scroll(horizontal, vertical));
            }
            return;
        }

        let changed_index = event.pointer_index().min(event.pointer_count().saturating_sub(1));

        for pointer in event.pointers() {
            let x = pointer.axis_value(Axis::X).round() as i32;
            let y = pointer.axis_value(Axis::Y).round() as i32;
            let pointer_index = pointer.pointer_index();
            let pointer_id = pointer.pointer_id();
            let device_id = format!("{}:{}", event.device_id(), pointer_id);

            let touch_event = match action {
                MotionAction::Down if pointer_index == changed_index => Some(TouchEvent::Started(
                    x.max(0) as u64,
                    y.max(0) as u64,
                )),
                MotionAction::PointerDown if pointer_index == changed_index => {
                    Some(TouchEvent::Started(x.max(0) as u64, y.max(0) as u64))
                }
                MotionAction::Up if pointer_index == changed_index => Some(TouchEvent::Ended(
                    x.max(0) as u64,
                    y.max(0) as u64,
                )),
                MotionAction::PointerUp if pointer_index == changed_index => {
                    Some(TouchEvent::Ended(x.max(0) as u64, y.max(0) as u64))
                }
                MotionAction::Move => Some(TouchEvent::Moved(x.max(0) as u64, y.max(0) as u64)),
                MotionAction::Cancel => Some(TouchEvent::Cancelled),
                _ => None,
            };

            let tool = pointer.tool_type();
            let is_touch_tool = matches!(tool, ToolType::Finger | ToolType::Stylus | ToolType::Eraser);
            if is_touch_tool {
                if let Some(touch_event) = touch_event {
                    push_touch(Touch { device_id, event: touch_event });
                }
            }

            // Preserve the existing desktop/widget pointer contract for the
            // primary pointer. Touch and mouse motion both move the cursor; a
            // primary down/up contact maps to the existing left-button events.
            if pointer_index == 0 && !matches!(tool, ToolType::Palm) {
                push_event(WindowEvent::CursorMoved(x, y));
                match action {
                    MotionAction::Down => {
                        if set_key_down(Key::MouseLeft, true) {
                            push_event(WindowEvent::KeyPressed(Key::MouseLeft));
                        }
                    }
                    MotionAction::Up | MotionAction::PointerUp | MotionAction::Cancel => {
                        if set_key_down(Key::MouseLeft, false) {
                            push_event(WindowEvent::KeyReleased(Key::MouseLeft));
                        }
                    }
                    _ => {}
                }
            }

        }
    }

    /// Android Activity titles are normally supplied by the manifest. Keeping
    /// this as a no-op preserves the cross-platform API without forcing JNI work
    /// onto every title change.
    pub fn set_title(&self, _title: &str) {}

    /// Android launcher icons are resources packaged in the APK, not mutable
    /// top-level-window icons.
    pub fn set_icon(&self, _path: &str) {}

    /// Suspends MataWin presentation while retaining Activity ownership.
    pub fn hide(&self) {
        self.hidden.set(true);
    }

    /// Resumes MataWin presentation.
    pub fn show(&self) {
        self.hidden.set(false);
        self.refresh_client_size(false);
    }

    /// Android Activities already occupy the system-managed app window.
    pub fn maximize(&self) {
        let size = self.client_size();
        push_event(WindowEvent::Maximized(size.0, size.1));
    }

    /// Emits the portable minimized state. Android continues to own Activity
    /// task visibility; applications should rely on lifecycle events for actual
    /// backgrounding.
    pub fn minimize(&self) {
        push_event(WindowEvent::Minimized);
    }

    /// Restores the portable state to the current Activity surface size.
    pub fn restore(&self) {
        let size = self.client_size();
        push_event(WindowEvent::Restored(size.0, size.1));
    }

    /// Android controls top-level window placement, so repositioning is ignored.
    pub fn reposition(&self, _x: i32, _y: i32) {}

    /// Changes the logical software framebuffer size used by future frames.
    /// The Android compositor controls the physical Activity surface size.
    pub fn resize(&self, width: u32, height: u32) {
        if width != 0 && height != 0 {
            self.framebuffer_size.set((width, height));
        }
    }

    /// Android system decorations are managed by the Activity/window flags.
    pub fn set_decorations(&self, _enabled: bool) {}

    /// Not applicable to an Android Activity window.
    pub fn set_maximizable(&self, _enabled: bool) {}

    /// Not applicable to an Android Activity window.
    pub fn set_minimizable(&self, _enabled: bool) {}

    /// Android controls the Activity's physical bounds.
    pub fn set_resizable(&self, _enabled: bool) {}

    /// Returns the current `ANativeWindow*` as an opaque pointer, or null while
    /// Android has no active surface.
    pub fn handle(&self) -> *mut c_void {
        self.app
            .native_window()
            .map(|window| window.ptr().as_ptr().cast::<c_void>())
            .unwrap_or(ptr::null_mut())
    }

    /// Returns the logical software framebuffer dimensions.
    pub fn framebuffer_size(&self) -> (u32, u32) {
        self.framebuffer_size.get()
    }

    /// Returns the current Android native surface size in physical pixels.
    pub fn client_size(&self) -> (u32, u32) {
        let current = self.current_native_size();
        if current.0 != 0 && current.1 != 0 {
            self.client_size.set(current);
            current
        } else {
            self.client_size.get()
        }
    }
}

/// Writes one MataWin `0xAARRGGBB` pixel to an Android RGBA_8888 address.
#[inline]
fn write_rgba8888(dst: *mut u8, pixel: u32) {
    unsafe {
        ptr::write(dst, ((pixel >> 16) & 0xff) as u8);
        ptr::write(dst.add(1), ((pixel >> 8) & 0xff) as u8);
        ptr::write(dst.add(2), (pixel & 0xff) as u8);
        ptr::write(dst.add(3), ((pixel >> 24) & 0xff) as u8);
    }
}

/// Returns the current native surface dimensions, clamping invalid negatives.
#[inline]
fn native_size(window: &ndk::native_window::NativeWindow) -> (u32, u32) {
    (window.width().max(0) as u32, window.height().max(0) as u32)
}

/// Pushes a portable window event onto MataWin's shared queue.
#[inline]
fn push_event(event: WindowEvent) {
    EVENT_QUEUE
        .get_or_init(|| std::sync::Mutex::new(Vec::with_capacity(64)))
        .lock()
        .unwrap()
        .push(event);
}

/// Queues a touch event while bounding retained input if an application chooses
/// to consume only the mouse-compatible event stream.
#[inline]
fn push_touch(touch: Touch) {
    const MAX_QUEUED_TOUCHES: usize = 256;
    let mut queue = TOUCH_QUEUE
        .get_or_init(|| std::sync::Mutex::new(Vec::with_capacity(32)))
        .lock()
        .unwrap();
    if queue.len() == MAX_QUEUED_TOUCHES {
        queue.remove(0);
    }
    queue.push(touch);
}

/// Updates the held-key set and reports whether its membership changed.
#[inline]
fn set_key_down(key: Key, down: bool) -> bool {
    let mut keys = KEYS_DOWN
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap();
    if down {
        keys.insert(key)
    } else {
        keys.remove(&key)
    }
}

/// Clears held input when Android suspends or loses focus.
#[inline]
fn clear_keys() {
    if let Some(keys) = KEYS_DOWN.get() {
        keys.lock().unwrap().clear();
    }
}

/// Maps Android key codes to MataWin's portable key representation.
fn map_key(code: Keycode) -> Option<Key> {
    Some(match code {
        Keycode::Del => Key::Backspace,
        Keycode::Enter | Keycode::NumpadEnter | Keycode::DpadCenter => Key::Enter,
        Keycode::Tab => Key::Tab,
        Keycode::Escape | Keycode::Back => Key::Escape,
        Keycode::Space => Key::Space,
        Keycode::DpadUp => Key::Up,
        Keycode::DpadDown => Key::Down,
        Keycode::DpadLeft => Key::Left,
        Keycode::DpadRight => Key::Right,
        Keycode::CtrlLeft => Key::LeftCtrl,
        Keycode::CtrlRight => Key::RightCtrl,
        Keycode::ShiftLeft => Key::LeftShift,
        Keycode::ShiftRight => Key::RightShift,
        Keycode::Keycode0 | Keycode::Numpad0 => Key::Num(0),
        Keycode::Keycode1 | Keycode::Numpad1 => Key::Num(1),
        Keycode::Keycode2 | Keycode::Numpad2 => Key::Num(2),
        Keycode::Keycode3 | Keycode::Numpad3 => Key::Num(3),
        Keycode::Keycode4 | Keycode::Numpad4 => Key::Num(4),
        Keycode::Keycode5 | Keycode::Numpad5 => Key::Num(5),
        Keycode::Keycode6 | Keycode::Numpad6 => Key::Num(6),
        Keycode::Keycode7 | Keycode::Numpad7 => Key::Num(7),
        Keycode::Keycode8 | Keycode::Numpad8 => Key::Num(8),
        Keycode::Keycode9 | Keycode::Numpad9 => Key::Num(9),
        Keycode::A => Key::Char('a'),
        Keycode::B => Key::Char('b'),
        Keycode::C => Key::Char('c'),
        Keycode::D => Key::Char('d'),
        Keycode::E => Key::Char('e'),
        Keycode::F => Key::Char('f'),
        Keycode::G => Key::Char('g'),
        Keycode::H => Key::Char('h'),
        Keycode::I => Key::Char('i'),
        Keycode::J => Key::Char('j'),
        Keycode::K => Key::Char('k'),
        Keycode::L => Key::Char('l'),
        Keycode::M => Key::Char('m'),
        Keycode::N => Key::Char('n'),
        Keycode::O => Key::Char('o'),
        Keycode::P => Key::Char('p'),
        Keycode::Q => Key::Char('q'),
        Keycode::R => Key::Char('r'),
        Keycode::S => Key::Char('s'),
        Keycode::T => Key::Char('t'),
        Keycode::U => Key::Char('u'),
        Keycode::V => Key::Char('v'),
        Keycode::W => Key::Char('w'),
        Keycode::X => Key::Char('x'),
        Keycode::Y => Key::Char('y'),
        Keycode::Z => Key::Char('z'),
        Keycode::Comma | Keycode::NumpadComma => Key::Char(','),
        Keycode::Period | Keycode::NumpadDot => Key::Char('.'),
        Keycode::Grave => Key::Char('`'),
        Keycode::Minus | Keycode::NumpadSubtract => Key::Char('-'),
        Keycode::Equals | Keycode::NumpadEquals => Key::Char('='),
        Keycode::LeftBracket => Key::Char('['),
        Keycode::RightBracket => Key::Char(']'),
        Keycode::Backslash => Key::Char('\\'),
        Keycode::Semicolon => Key::Char(';'),
        Keycode::Apostrophe => Key::Char('\''),
        Keycode::Slash | Keycode::NumpadDivide => Key::Char('/'),
        Keycode::F1 => Key::F(1),
        Keycode::F2 => Key::F(2),
        Keycode::F3 => Key::F(3),
        Keycode::F4 => Key::F(4),
        Keycode::F5 => Key::F(5),
        Keycode::F6 => Key::F(6),
        Keycode::F7 => Key::F(7),
        Keycode::F8 => Key::F(8),
        Keycode::F9 => Key::F(9),
        Keycode::F10 => Key::F(10),
        Keycode::F11 => Key::F(11),
        Keycode::F12 => Key::F(12),
        _ => return None,
    })
}
