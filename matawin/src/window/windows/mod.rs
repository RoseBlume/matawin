//! Implements the `windows` module.
//!
//! This module documents both public API items and internal implementation details.

use std::{
    cell::Cell,
    collections::HashSet,
    ffi::{ c_void, OsStr },
    iter::once,
    os::windows::ffi::OsStrExt,
    sync::{ Mutex, Once },
};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{ HINSTANCE, LPARAM, LRESULT, RECT, WPARAM },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{ SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 },
            WindowsAndMessaging::{
                AdjustWindowRectEx,
                CreateWindowExW,
                DefWindowProcW,
                GetClientRect,
                LoadCursorW,
                PostQuitMessage,
                RegisterClassW,
                SetWindowTextW,
                WNDCLASSW,
                WINDOW_EX_STYLE,
                IDC_ARROW,
                WM_CLOSE,
                WM_DESTROY,
                WM_ERASEBKGND,
                WM_KEYDOWN,
                WM_KEYUP,
                WM_LBUTTONDOWN,
                WM_LBUTTONUP,
                WM_MBUTTONDOWN,
                WM_MBUTTONUP,
                WM_MOUSEMOVE,
                WM_MOUSEWHEEL,
                WM_MOUSEHWHEEL,
                WM_MOVE,
                WM_RBUTTONDOWN,
                WM_RBUTTONUP,
                WM_KILLFOCUS,
                WM_SIZE,
                WS_OVERLAPPEDWINDOW,
                WS_VISIBLE,
                SIZE_MAXIMIZED,
                SIZE_MINIMIZED,
            },
        },
    },
};

/// Contains the drop implementation.
///
/// Visibility: **private**.
mod drop;
/// Contains the events implementation.
///
/// Visibility: **private**.
mod events;
/// Contains the renderer implementation.
///
/// Visibility: **private**.
mod renderer;
/// Contains the show implementation.
///
/// Visibility: **private**.
mod show;

use crate::buffer::Framebuffer;
use crate::events::{ Key, WindowEvent, EVENT_QUEUE, KEYS_DOWN };
use renderer::Direct2DRenderer;

pub use windows::Win32::Foundation::HWND;

/// Static storage used for enable native dpi.
///
/// Visibility: **private**.
static ENABLE_NATIVE_DPI: Once = Once::new();
/// Static storage used for register window class.
///
/// Visibility: **private**.
static REGISTER_WINDOW_CLASS: Once = Once::new();

/// Opts the process into per-monitor-v2 DPI awareness before the first HWND is
/// created. Without this, Windows can DPI-virtualize the client area and the
/// DWM scales an already-rasterized software framebuffer, which makes text and
/// rounded edges visibly softer after maximize/resize on scaled displays.
#[doc = "Implements the enable native dpi operation. Visibility: private/internal."]
fn enable_native_dpi() {
    ENABLE_NATIVE_DPI.call_once(|| {
        // This can fail with ACCESS_DENIED if the executable/host already chose
        // a DPI awareness mode. In that case we keep the host's policy.
        let _ = unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
        };
    });
}

/// Implements class name.
///
/// Visibility: **private**.
#[doc = "Implements the class name operation. Visibility: private/internal."]
fn class_name() -> PCWSTR {
    windows::core::w!("winbuf_class")
}

/// Implements wide null.
///
/// Visibility: **private**.
#[doc = "Implements the wide null operation. Visibility: private/internal."]
fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

/// Returns the current native client-area size in device pixels.
///
/// This deliberately queries the HWND instead of relying only on the cached
/// `WM_SIZE` value. Resize/maximize messages can be coalesced, and the widget
/// renderer needs the authoritative drawable dimensions before rasterizing a
/// frame.
#[inline]
#[doc = "Implements the client size for hwnd operation. Visibility: private/internal."]
fn client_size_for_hwnd(hwnd: HWND) -> Option<(u32, u32)> {
    let mut rect = RECT::default();
    (unsafe { GetClientRect(hwnd, &mut rect) }).ok()?;

    Some(((rect.right - rect.left).max(0) as u32, (rect.bottom - rect.top).max(0) as u32))
}

/// Pushes event.
///
/// Visibility: **private**.
#[doc = "Pushes event. Visibility: private/internal."]
fn push_event(event: WindowEvent) {
    if let Some(queue) = EVENT_QUEUE.get() {
        queue.lock().unwrap().push(event);
    }
}

#[inline]
/// Implements mouse position.
///
/// Visibility: **private**.
#[doc = "Implements the mouse position operation. Visibility: private/internal."]
fn mouse_position(lparam: LPARAM) -> (i32, i32) {
    let raw = lparam.0 as usize;
    ((raw & 0xffff) as i16 as i32, ((raw >> 16) & 0xffff) as i16 as i32)
}

#[inline]
/// Pushes mouse position.
///
/// Visibility: **private**.
#[doc = "Pushes mouse position. Visibility: private/internal."]
fn push_mouse_position(lparam: LPARAM) {
    let (x, y) = mouse_position(lparam);
    push_event(WindowEvent::CursorMoved(x, y));
}

#[inline]
/// Sets key down.
///
/// Visibility: **private**.
#[doc = "Sets key down. Visibility: private/internal."]
fn set_key_down(key: Key, down: bool) -> bool {
    let Some(keys) = KEYS_DOWN.get() else {
        return false;
    };
    let mut keys = keys.lock().unwrap();
    if down {
        keys.insert(key)
    } else {
        keys.remove(&key)
    }
}

/// Implements wndproc.
///
/// Visibility: **private**.
#[doc = "Implements the wndproc operation. Visibility: private/internal."]
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            push_event(WindowEvent::Close);
            LRESULT(0)
        }

        WM_DESTROY => {
            push_event(WindowEvent::Close);
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }

        // Direct2D paints the complete client area. Reporting the background as
        // already erased avoids unnecessary GDI work and reduces resize flicker.
        WM_ERASEBKGND => LRESULT(1),

        WM_SIZE => {
            let raw = lparam.0 as usize;
            let width = (raw & 0xffff) as u32;
            let height = ((raw >> 16) & 0xffff) as u32;

            match wparam.0 {
                x if x == (SIZE_MINIMIZED as usize) => push_event(WindowEvent::Minimized),
                x if x == (SIZE_MAXIMIZED as usize) => {
                    push_event(WindowEvent::Maximized(width, height));
                }
                _ => push_event(WindowEvent::Resized(width, height)),
            }

            LRESULT(0)
        }

        WM_MOVE => {
            let raw = lparam.0 as usize;
            let x = (raw & 0xffff) as i16 as i32;
            let y = ((raw >> 16) & 0xffff) as i16 as i32;

            push_event(WindowEvent::Repositioned(x, y));
            LRESULT(0)
        }

        WM_KEYDOWN => {
            if let Some(key) = map_key(wparam.0, lparam.0) {
                let was_down = (lparam.0 & (1 << 30)) != 0;

                if !was_down && set_key_down(key, true) {
                    push_event(WindowEvent::KeyPressed(key));
                }
            }

            LRESULT(0)
        }

        WM_KEYUP => {
            if let Some(key) = map_key(wparam.0, lparam.0) {
                if set_key_down(key, false) {
                    push_event(WindowEvent::KeyReleased(key));
                }
            }

            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            push_mouse_position(lparam);
            LRESULT(0)
        }

        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            push_event(WindowEvent::Scroll(0, delta));
            LRESULT(0)
        }

        WM_MOUSEHWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            push_event(WindowEvent::Scroll(delta, 0));
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            // Button messages contain their own client position. Queue it
            // before the button state so widget focus/hit-testing never depends
            // on an older WM_MOUSEMOVE after resize or DPI changes.
            push_mouse_position(lparam);
            if set_key_down(Key::MouseLeft, true) {
                push_event(WindowEvent::KeyPressed(Key::MouseLeft));
            }
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            // Button messages contain their own client position. Queue it
            // before the button state so widget focus/hit-testing never depends
            // on an older WM_MOUSEMOVE after resize or DPI changes.
            push_mouse_position(lparam);
            if set_key_down(Key::MouseLeft, false) {
                push_event(WindowEvent::KeyReleased(Key::MouseLeft));
            }
            LRESULT(0)
        }

        WM_RBUTTONDOWN => {
            // Button messages contain their own client position. Queue it
            // before the button state so widget focus/hit-testing never depends
            // on an older WM_MOUSEMOVE after resize or DPI changes.
            push_mouse_position(lparam);
            if set_key_down(Key::MouseRight, true) {
                push_event(WindowEvent::KeyPressed(Key::MouseRight));
            }
            LRESULT(0)
        }

        WM_RBUTTONUP => {
            // Button messages contain their own client position. Queue it
            // before the button state so widget focus/hit-testing never depends
            // on an older WM_MOUSEMOVE after resize or DPI changes.
            push_mouse_position(lparam);
            if set_key_down(Key::MouseRight, false) {
                push_event(WindowEvent::KeyReleased(Key::MouseRight));
            }
            LRESULT(0)
        }

        WM_MBUTTONDOWN => {
            // Button messages contain their own client position. Queue it
            // before the button state so widget focus/hit-testing never depends
            // on an older WM_MOUSEMOVE after resize or DPI changes.
            push_mouse_position(lparam);
            if set_key_down(Key::MouseMiddle, true) {
                push_event(WindowEvent::KeyPressed(Key::MouseMiddle));
            }
            LRESULT(0)
        }

        WM_MBUTTONUP => {
            // Button messages contain their own client position. Queue it
            // before the button state so widget focus/hit-testing never depends
            // on an older WM_MOUSEMOVE after resize or DPI changes.
            push_mouse_position(lparam);
            if set_key_down(Key::MouseMiddle, false) {
                push_event(WindowEvent::KeyReleased(Key::MouseMiddle));
            }
            LRESULT(0)
        }

        WM_KILLFOCUS => {
            if let Some(keys) = KEYS_DOWN.get() {
                keys.lock().unwrap().clear();
            }
            LRESULT(0)
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

/// Stores the state and data for platform window.
///
/// Visibility: **public**.
pub struct PlatformWindow {
    /// Stores hwnd.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) hwnd: HWND,

    /// Size of the most recently presented software framebuffer. This follows
    /// the client area when an application redraws at native resize dimensions.
    size: Cell<(u32, u32)>,

    /// Cached native client size. Public client-size queries and presentation
    /// refresh this from GetClientRect so a redraw never depends on a stale
    /// resize notification.
    pub(super) client_size: Cell<(u32, u32)>,

    /// Stores renderer.
    ///
    /// Visibility: **private**.
    renderer: Direct2DRenderer,
    /// Stores icon.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) icon: Cell<Option<windows::Win32::UI::WindowsAndMessaging::HICON>>,
}

impl PlatformWindow {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        // DPI awareness has to be selected before the first native window is
        // created. This makes client dimensions true device pixels instead of
        // DPI-virtualized logical pixels, so software widgets can rerasterize at
        // the monitor's real resolution.
        enable_native_dpi();

        KEYS_DOWN.get_or_init(|| Mutex::new(HashSet::with_capacity(16)));
        EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::with_capacity(32)));

        let module = unsafe { GetModuleHandleW(None).expect("GetModuleHandleW failed") };
        let instance = HINSTANCE(module.0);

        REGISTER_WINDOW_CLASS.call_once(|| {
            let cursor = unsafe { LoadCursorW(None, IDC_ARROW).expect("LoadCursorW failed") };

            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance,
                lpszClassName: class_name(),
                hCursor: cursor,
                ..Default::default()
            };

            let atom = unsafe { RegisterClassW(&class) };

            if atom == 0 {
                panic!("RegisterClassW failed");
            }
        });

        let title = wide_null(title);
        let style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
        let ex_style = WINDOW_EX_STYLE(0);

        // Treat the requested size as the client/framebuffer size rather than the
        // outer window size. This also keeps the Direct2D target exactly aligned
        // with the software framebuffer on creation.
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: size.0 as i32,
            bottom: size.1 as i32,
        };

        unsafe {
            AdjustWindowRectEx(&mut outer, style, false, ex_style).expect(
                "AdjustWindowRectEx failed while creating the window"
            );
        }

        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class_name(),
                PCWSTR(title.as_ptr()),
                style,
                0,
                0,
                outer.right - outer.left,
                outer.bottom - outer.top,
                None,
                None,
                Some(instance),
                None
            ).expect("CreateWindowExW failed")
        };

        let mut rect = RECT::default();
        unsafe {
            GetClientRect(hwnd, &mut rect).expect("GetClientRect failed after window creation");
        }
        let client_size = (
            (rect.right - rect.left).max(0) as u32,
            (rect.bottom - rect.top).max(0) as u32,
        );

        let renderer = Direct2DRenderer::new(hwnd, size, client_size);

        Self {
            hwnd,
            size: Cell::new(size),
            client_size: Cell::new(client_size),
            renderer,
            icon: Cell::new(None),
        }
    }

    /// Performs the present operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the present operation. Visibility: public."]
    pub fn present(&mut self, fb: &Framebuffer) {
        // Re-query here instead of trusting the last WM_SIZE. Resize messages
        // can be coalesced, and maximize can change the drawable client size
        // before the application observes the queued event. WidgetWindow uses
        // this same authoritative size before it rasterizes the frame.
        let client_size = client_size_for_hwnd(self.hwnd).unwrap_or_else(|| self.client_size.get());
        self.client_size.set(client_size);
        self.size.set((fb.width, fb.height));
        self.renderer.resize_target(self.hwnd, client_size);
        self.renderer.present(self.hwnd, fb);
    }

    /// Sets title.
    ///
    /// Visibility: **public**.
    #[doc = "Sets title. Visibility: public."]
    pub fn set_title(&self, title: &str) {
        let title = wide_null(title);

        unsafe {
            SetWindowTextW(self.hwnd, PCWSTR(title.as_ptr())).expect("SetWindowTextW failed");
        }
    }

    /// Returns the native HWND value as a raw pointer for compatibility with the
    /// previous API. Prefer using `hwnd()` when working with the `windows` crate.
    #[doc = "Implements the handle operation. Visibility: public."]
    pub fn handle(&self) -> *mut c_void {
        self.hwnd.0
    }

    /// Typed HWND access for code that already uses the `windows` crate.
    #[doc = "Implements the hwnd operation. Visibility: public."]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Returns the logical software framebuffer dimensions.
    #[doc = "Implements the framebuffer size operation. Visibility: public."]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        self.size.get()
    }

    /// Returns the current native client-area dimensions.
    #[doc = "Implements the client size operation. Visibility: public."]
    pub fn client_size(&self) -> (u32, u32) {
        // Querying GetClientRect here keeps callers in sync even when they
        // redraw without polling events first. Under per-monitor DPI awareness
        // this is the physical drawable pixel size.
        if let Some(size) = client_size_for_hwnd(self.hwnd) {
            self.client_size.set(size);
            size
        } else {
            self.client_size.get()
        }
    }

    /// Performs the refresh client size operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Refreshes client size. Visibility: parent-module-visible."]
    pub(super) fn refresh_client_size(&self) {
        let mut rect = RECT::default();

        if (unsafe { GetClientRect(self.hwnd, &mut rect) }).is_ok() {
            self.client_size.set((
                (rect.right - rect.left).max(0) as u32,
                (rect.bottom - rect.top).max(0) as u32,
            ));
        }
    }
}

/// Implements map key.
///
/// Visibility: **private**.
#[doc = "Implements the map key operation. Visibility: private/internal."]
fn map_key(wparam: usize, lparam: isize) -> Option<Key> {
    match wparam {
        0x08 => Some(Key::Backspace),
        0x0d => Some(Key::Enter),
        0x09 => Some(Key::Tab),
        0x1b => Some(Key::Escape),
        0x20 => Some(Key::Space),

        0x25 => Some(Key::Left),
        0x26 => Some(Key::Up),
        0x27 => Some(Key::Right),
        0x28 => Some(Key::Down),

        0x10 => {
            let scancode = ((lparam >> 16) & 0xff) as u32;

            match scancode {
                0x2a => Some(Key::LeftShift),
                0x36 => Some(Key::RightShift),
                _ => None,
            }
        }

        0x11 => {
            let extended = ((lparam >> 24) & 1) != 0;

            if extended {
                Some(Key::RightCtrl)
            } else {
                Some(Key::LeftCtrl)
            }
        }

        0x30..=0x39 => Some(Key::Num((wparam - 0x30) as u8)),

        0x41..=0x5a => {
            let c = (wparam as u8 as char).to_ascii_lowercase();
            Some(Key::Char(c))
        }

        // Windows OEM virtual keys. Keep these unshifted; TextField applies
        // the Shift layer centrally so Windows/X11/Wayland behave alike.
        0xba => Some(Key::Char(';')),
        0xbb => Some(Key::Char('=')),
        0xbc => Some(Key::Char(',')),
        0xbd => Some(Key::Char('-')),
        0xbe => Some(Key::Char('.')),
        0xbf => Some(Key::Char('/')),
        0xc0 => Some(Key::Char('`')),
        0xdb => Some(Key::Char('[')),
        0xdc => Some(Key::Char('\\')),
        0xdd => Some(Key::Char(']')),
        0xde => Some(Key::Char('\'')),

        0x70..=0x7b => Some(Key::F((wparam - 0x70 + 1) as u8)),

        _ => None,
    }
}
