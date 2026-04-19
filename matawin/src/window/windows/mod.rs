use std::ffi::{c_void, CString};
use std::ptr::{null, null_mut};
mod events;
mod show;
mod drop;


use crate::buffer::Framebuffer;
use std::sync::{Mutex};
use std::collections::HashSet;

use crate::events::{WindowEvent, Key, EVENT_QUEUE, KEYS_DOWN};
pub use super::HWND;

type HDC = *mut c_void;
type HINSTANCE = *mut c_void;
type LPCSTR = *const i8;
type UINT = u32;
type WPARAM = usize;
type LPARAM = isize;
type LRESULT = isize;

const WM_CLOSE: u32 = 0x0010;
const WM_SIZE: u32 = 0x0005;
const WM_MOVE: u32 = 0x0003;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_MBUTTONDOWN: u32 = 0x0207;


const SIZE_MINIMIZED: usize = 1;
const SIZE_MAXIMIZED: usize = 2;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const WS_VISIBLE: u32 = 0x10000000;

const WM_DESTROY: u32 = 0x0002;

const SRCCOPY: u32 = 0x00CC0020;

#[repr(C)]
struct WNDCLASSA {
    style: u32,
    lpfnWndProc: extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HINSTANCE,
    hIcon: *mut c_void,
    hCursor: *mut c_void,
    hbrBackground: *mut c_void,
    lpszMenuName: LPCSTR,
    lpszClassName: LPCSTR,
}

#[repr(C)]
pub struct MSG {
    hwnd: HWND,
    message: UINT,
    wParam: WPARAM,
    lParam: LPARAM,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[repr(C)]
struct BITMAPINFOHEADER {
    biSize: u32,
    biWidth: i32,
    biHeight: i32,
    biPlanes: u16,
    biBitCount: u16,
    biCompression: u32,
    biSizeImage: u32,
    biXPelsPerMeter: i32,
    biYPelsPerMeter: i32,
    biClrUsed: u32,
    biClrImportant: u32,
}

#[repr(C)]
struct BITMAPINFO {
    bmiHeader: BITMAPINFOHEADER,
    bmiColors: [u32; 1],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassA(lpClassName: *const WNDCLASSA) -> u16;
    fn CreateWindowExA(
        dwExStyle: u32,
        lpClassName: LPCSTR,
        lpWindowName: LPCSTR,
        dwStyle: u32,
        x: i32,
        y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: *mut c_void,
        hInstance: HINSTANCE,
        lpParam: *mut c_void,
    ) -> HWND;

    fn DefWindowProcA(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
    // fn GetMessageA(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(msg: *const MSG) -> i32;
    pub fn DispatchMessageA(msg: *const MSG) -> LRESULT;
    pub fn PeekMessageA(
        lpMsg: *mut MSG,
        hWnd: HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
        wRemoveMsg: u32,
    ) -> i32;
    fn PostQuitMessage(code: i32);
    fn GetDC(hwnd: HWND) -> HDC;
    fn SetWindowTextA(hwnd: HWND, lpString: *const i8) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn StretchDIBits(
        hdc: HDC,
        xDest: i32,
        yDest: i32,
        DestWidth: i32,
        DestHeight: i32,
        xSrc: i32,
        ySrc: i32,
        SrcWidth: i32,
        SrcHeight: i32,
        lpBits: *const c_void,
        lpbmi: *const BITMAPINFO,
        iUsage: u32,
        rop: u32,
    ) -> i32;
}

fn push_event(event: WindowEvent) {
    if let Some(queue) = EVENT_QUEUE.get() {
        queue.lock().unwrap().push(event);
    }
}

extern "system" fn wndproc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {

    match msg {
        WM_CLOSE => {
            push_event(WindowEvent::Close);
            0
        }

        WM_DESTROY => {
            push_event(WindowEvent::Close);
            unsafe { PostQuitMessage(0) };
            0
        }

        WM_SIZE => {
            let width = (lparam & 0xFFFF) as u32;
            let height = ((lparam >> 16) & 0xFFFF) as u32;

            match wparam {
                SIZE_MINIMIZED => push_event(WindowEvent::Minimized),
                SIZE_MAXIMIZED => push_event(WindowEvent::Maximized(width, height)),
                _ => push_event(WindowEvent::Resized(width, height)),
                // _ => push_event(WindowEvent::Resized(width, height)),
            }

            0
        }

        WM_MOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            push_event(WindowEvent::Repositioned(x, y));
            0
        }

        WM_KEYDOWN => {
            if let Some(key) = map_key(wparam, lparam) {
                let was_down = (lparam & (1 << 30)) != 0;

                if !was_down {
                    push_event(WindowEvent::KeyPressed(key));

                    if let Some(keys) = KEYS_DOWN.get() {
                        keys.lock().unwrap().insert(key);
                    }
                }
            }
            0
        }

        WM_KEYUP => {
            if let Some(key) = map_key(wparam, lparam) {
                push_event(WindowEvent::KeyReleased(key));

                if let Some(keys) = KEYS_DOWN.get() {
                    keys.lock().unwrap().remove(&key);
                }
            }
            0
        }
        WM_LBUTTONDOWN => {
            push_event(WindowEvent::KeyPressed(Key::MouseLeft));
            0
        }

        WM_RBUTTONDOWN => {
            push_event(WindowEvent::KeyPressed(Key::MouseRight));
            0
        }

        WM_MBUTTONDOWN => {
            push_event(WindowEvent::KeyPressed(Key::MouseMiddle));
            0
        }

        _ => unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) },
    }
}


pub struct PlatformWindow {
    hwnd: HWND,
    hdc: HDC,
    size: (u32, u32),
    // position: (i32, i32),
    bmi: BITMAPINFO,
}



impl PlatformWindow {
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        let class_name = CString::new("winbuf_class").unwrap();
        let title = CString::new(title).unwrap();
        let position = (0, 0);
        KEYS_DOWN.get_or_init(|| Mutex::new(HashSet::new()));
        EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        let wc = WNDCLASSA {
            style: 0,
            lpfnWndProc: wndproc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: null_mut(),
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };

        unsafe {
            RegisterClassA(&wc);
        }

        let hwnd = unsafe {
            CreateWindowExA(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                position.0,
                position.1,
                size.0 as i32,
                size.1 as i32,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };

        let hdc = unsafe { GetDC(hwnd) };

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0 as i32,
                biHeight: -(size.1 as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [0],
        };

        Self {
            hwnd,
            hdc,
            size: size,
            // position: position,
            bmi
        }
    }


    pub fn present(&mut self, fb: &Framebuffer) {
        unsafe {
            StretchDIBits(
                self.hdc,
                0,
                0,
                self.size.0 as i32,
                self.size.1 as i32,
                0,
                0,
                self.size.0 as i32,
                self.size.1 as i32,
                fb.as_ptr() as *const _,
                &self.bmi,
                0,
                SRCCOPY,
            );
        }
    }


    pub fn set_title(&self, title: &str) {
        let c_title = CString::new(title).expect("Title contains null byte");

        unsafe {
            SetWindowTextA(self.hwnd, c_title.as_ptr());
        }
    }

    pub fn handle(&self) -> *mut c_void {
        self.hwnd
    }
}

fn map_key(wparam: usize, lparam: isize) -> Option<Key> {

    match wparam {
        0x08 => Some(Key::Backspace),
        0x0D => Some(Key::Enter),
        0x09 => Some(Key::Tab),
        0x1B => Some(Key::Escape),
        0x20 => Some(Key::Space),

        0x25 => Some(Key::Left),
        0x26 => Some(Key::Up),
        0x27 => Some(Key::Right),
        0x28 => Some(Key::Down),

        0x10 => {
            let scancode = ((lparam >> 16) & 0xFF) as u32;

            match scancode {
                0x2A => Some(Key::LeftShift),
                0x36 => Some(Key::RightShift),
                _ => None,
            }
        }

        // CTRL (extended flag)
        0x11 => {
            let extended = ((lparam >> 24) & 1) != 0;

            if extended {
                Some(Key::RightCtrl)
            } else {
                Some(Key::LeftCtrl)
            }
        }

        0x30..=0x39 => Some(Key::Num((wparam - 0x30) as u8)),

        0x41..=0x5A => {
            let c = (wparam as u8 as char).to_ascii_lowercase();
            Some(Key::Char(c))
        }

        0x70..=0x7B => Some(Key::F((wparam - 0x70 + 1) as u8)),

        _ => None,
    }
}