use std::ffi::CString;
use std::ptr::{null, null_mut};

use super::{Display, WindowHandle};

type Atom = u64;

#[repr(C)]
struct XEvent {
    pad: [u64; 24],
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XMapWindow(display: Display, window: WindowHandle);
    fn XUnmapWindow(display: Display, window: WindowHandle);

    fn XMoveWindow(display: Display, window: WindowHandle, x: i32, y: i32);
    fn XResizeWindow(display: Display, window: WindowHandle, width: u32, height: u32);

    fn XInternAtom(display: Display, name: *const i8, only_if_exists: i32) -> Atom;

    fn XSendEvent(
        display: Display,
        window: WindowHandle,
        propagate: i32,
        event_mask: i64,
        event_send: *mut XEvent,
    ) -> i32;

    fn XFlush(display: Display);
}

const SUBSTRUCTURE_REDIRECT_MASK: i64 = 0x02000000;
const SUBSTRUCTURE_NOTIFY_MASK: i64 = 0x04000000;

impl super::PlatformWindow {
    // ------------------------
    // Visibility
    // ------------------------

    pub fn show(&self) {
        unsafe {
            XMapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    pub fn hide(&self) {
        unsafe {
            XUnmapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    // ------------------------
    // Position / Size
    // ------------------------

    pub fn reposition(&self, x: i32, y: i32) {
        unsafe {
            XMoveWindow(self.display, self.window, x, y);
            XFlush(self.display);
        }
    }

    pub fn resize(&self, width: u32, height: u32) {
        unsafe {
            XResizeWindow(self.display, self.window, width, height);
            XFlush(self.display);
        }
    }

    // ------------------------
    // Minimize / Restore
    // ------------------------

    pub fn minimize(&self) {
        // Equivalent: unmap (basic fallback)
        self.hide();
    }

    pub fn restore(&self) {
        self.show();
    }

    // ------------------------
    // Maximize (EWMH hint)
    // ------------------------

    pub fn maximize(&self) {
        unsafe {
            let wm_state = XInternAtom(self.display, CString::new("_NET_WM_STATE").unwrap().as_ptr(), 0);
            let max_horz = XInternAtom(self.display, CString::new("_NET_WM_STATE_MAXIMIZED_HORZ").unwrap().as_ptr(), 0);
            let max_vert = XInternAtom(self.display, CString::new("_NET_WM_STATE_MAXIMIZED_VERT").unwrap().as_ptr(), 0);

            let mut event = XEvent { pad: [0; 24] };

            let data = &mut event.pad;

            data[0] = 33; // ClientMessage
            data[1] = self.window;
            data[2] = wm_state;
            data[3] = 1; // _NET_WM_STATE_ADD
            data[4] = max_horz;
            data[5] = max_vert;

            XSendEvent(
                self.display,
                self.window,
                0,
                SUBSTRUCTURE_REDIRECT_MASK | SUBSTRUCTURE_NOTIFY_MASK,
                &mut event,
            );

            XFlush(self.display);
        }
    }

    // ------------------------
    // Decorations (WM hint)
    // ------------------------

    pub fn set_decorations(&self, enabled: bool) {
        // This uses Motif hints (widely supported but not guaranteed)
        #[repr(C)]
        struct MotifHints {
            flags: u64,
            functions: u64,
            decorations: u64,
            input_mode: i64,
            status: u64,
        }

        const MWM_HINTS_DECORATIONS: u64 = 1 << 1;

        unsafe {
            let hints = MotifHints {
                flags: MWM_HINTS_DECORATIONS,
                functions: 0,
                decorations: if enabled { 1 } else { 0 },
                input_mode: 0,
                status: 0,
            };

            let atom = XInternAtom(
                self.display,
                CString::new("_MOTIF_WM_HINTS").unwrap().as_ptr(),
                0,
            );

            // XChangeProperty not declared earlier → inline declare
            unsafe extern "C" {
                fn XChangeProperty(
                    display: Display,
                    window: WindowHandle,
                    property: Atom,
                    type_: Atom,
                    format: i32,
                    mode: i32,
                    data: *const u8,
                    nelements: i32,
                );
            }

            XChangeProperty(
                self.display,
                self.window,
                atom,
                atom,
                32,
                0,
                &hints as *const _ as *const u8,
                5,
            );

            XFlush(self.display);
        }
    }

    // ------------------------
    // Unsupported (No-ops)
    // ------------------------

    pub fn set_maximizable(&self, _enabled: bool) {
        // Not controllable in X11
    }

    pub fn set_minimizable(&self, _enabled: bool) {
        // Not controllable in X11
    }

    pub fn set_resizable(&self, enabled: bool) {
        unsafe {
            let hints = XAllocSizeHints();
            if hints.is_null() {
                return;
            }

            if enabled {
                // Allow resizing → clear min/max constraints
                (*hints).flags = 0;
            } else {
                // Lock size → min == max
                (*hints).flags = (1 << 4) | (1 << 5); // PMinSize | PMaxSize

                (*hints).min_width = self.size.0 as i32;
                (*hints).min_height = self.size.1 as i32;
                (*hints).max_width = self.size.0 as i32;
                (*hints).max_height = self.size.1 as i32;
            }

            XSetWMNormalHints(self.display, self.window, hints);
            XFree(hints as *mut _);
        }
    }
}

#[repr(C)]
struct XSizeHints {
    flags: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    min_width: i32,
    min_height: i32,
    max_width: i32,
    max_height: i32,
    width_inc: i32,
    height_inc: i32,
    min_aspect_num: i32,
    min_aspect_den: i32,
    max_aspect_num: i32,
    max_aspect_den: i32,
    base_width: i32,
    base_height: i32,
    win_gravity: i32,
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XAllocSizeHints() -> *mut XSizeHints;
    fn XSetWMNormalHints(display: super::Display, window: super::WindowHandle, hints: *mut XSizeHints);
    fn XFree(data: *mut std::ffi::c_void);
}