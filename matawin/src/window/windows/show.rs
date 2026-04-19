

use super::HWND;

use std::ffi::CString;
use std::ptr::null_mut;

// Hides the window and activates another window.
const SW_HIDE: i32 = 0;
// Activates and displays a window. If the window is minimized or maximized, the system restores it to its original size and position. An application should specify this flag when displaying the window for the first time.
// const SW_NORMAL: i32 = 1;
// // Activates the window and displays it as a minimized window.
// const SW_SHOWMINIMIZED: i32 = 2;
// // Activates the window and displays it as a maximized window.
// const SW_SHOWMAXIMIZED: i32 = 3;
// Displays a window in its most recent size and position. This value is similar to SW_SHOWNORMAL, except that the window is not activated.
const SW_MAXIMIZE: i32 = 3;
// // Activates the window and displays it in its current size and position. The window is not activated.
// const SW_SHOWNOACTIVATE: i32 = 4;
// Activates the window and displays it in its current size and position.
const SW_SHOW: i32 = 5;
// Minimizes the specified window and activates the next top-level window in the Z order.
const SW_MINIMIZE: i32 = 6;
// Displays the window as a minimized window. This value is similar to SW_SHOWMINIMIZED, except the window is not activated.
// const SW_SHOWMINNOACTIVE: i32 = 7;
// // Displays the window in its current size and position. This value is similar to SW_SHOW, except that the window is not activated.
// const SW_SHOWNA: i32 = 8;
// Activates and displays the window. If the window is minimized or maximized, the system restores it to its original size and position. An application should specify this flag when restoring a minimized window.
const SW_RESTORE: i32 = 9;
// // Activates the window and displays it in its current size and position. If the window is minimized or maximized, the system restores it to its original size and position. An application should specify this flag when displaying the window for the first time.
// const SW_SHOWDEFAULT: i32 = 10;
// // Minimizes a window, even if the thread that owns the window is not responding. This flag should only be used when minimizing windows from a different thread.
// const SW_FORCEMINIMIZE: i32 = 11;

const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOSIZE: u32 = 0x0001;

#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

unsafe extern "system" {
    fn ShowWindow(hwnd: HWND, nCmdShow: i32) -> i32;
    fn AdjustWindowRect(rect: *mut RECT, style: u32, menu: i32) -> i32;
    fn GetWindowLongA(hwnd: HWND, index: i32) -> i32;
    fn SetWindowLongA(hwnd: HWND, index: i32, value: i32) -> i32;
    fn SetWindowPos(
        hWnd: HWND,
        hWndInsertAfter: HWND,
        X: i32,
        Y: i32,
        cx: i32,
        cy: i32,
        uFlags: u32,
    ) -> i32;
    fn SendMessageA(
        hwnd: HWND,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize;

    fn LoadImageA(
        hInst: *mut std::ffi::c_void,
        name: *const i8,
        type_: u32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> *mut std::ffi::c_void;

}

const IMAGE_ICON: u32 = 1;
const LR_LOADFROMFILE: u32 = 0x00000010;
const GWL_STYLE: i32 = -16;
const SWP_FRAMECHANGED: u32 = 0x0020;
const WS_MAXIMIZEBOX: u32 = 0x00010000;
const WS_MINIMIZEBOX: u32 = 0x00020000;
const WS_THICKFRAME: u32 = 0x00040000; // resizable border
const WS_CAPTION: u32 = 0x00C00000;    // required for buttons


const WM_SETICON: u32 = 0x0080;

const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;

// const IMAGE_ICON: u32 = 1;

// const LR_LOADFROMFILE: u32 = 0x00000010;
const LR_DEFAULTSIZE: u32 = 0x00000040;

impl super::PlatformWindow {
    pub fn hide(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn set_icon(&self, path: &str) {

        let path = CString::new(path).unwrap();

        unsafe {
            let hicon = LoadImageA(
                null_mut(),
                path.as_ptr(),
                IMAGE_ICON,
                0,
                0,
                LR_LOADFROMFILE | LR_DEFAULTSIZE,
            );

            if !hicon.is_null() {
                // Set big icon (taskbar / alt-tab)
                SendMessageA(self.hwnd, WM_SETICON, ICON_BIG as usize, hicon as isize);

                // Set small icon (title bar)
                SendMessageA(self.hwnd, WM_SETICON, ICON_SMALL as usize, hicon as isize);
            }
        }
    }

    pub fn maximize(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_MAXIMIZE);
        }
    }

    pub fn minimize(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_MINIMIZE);
        }
    }
    pub fn restore(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_RESTORE);
        }
    }

    pub fn reposition(&self, x: i32, y: i32) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOZORDER | SWP_NOSIZE,
            );
        }
    }

    pub fn resize(&self, width: u32, height: u32) {
        unsafe {
            let style = GetWindowLongA(self.hwnd, GWL_STYLE) as u32;

            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };

            AdjustWindowRect(&mut rect, style, 0);

            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;

            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                0,
                0,
                w,
                h,
                SWP_NOZORDER | SWP_NOMOVE,
            );
        }
    }
    pub fn show(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
        }
    }

    fn update_style(&self, add: u32, remove: u32) {
        unsafe {
            let mut style = GetWindowLongA(self.hwnd, GWL_STYLE) as u32;

            style |= add;
            style &= !remove;

            SetWindowLongA(self.hwnd, GWL_STYLE, style as i32);

            // Apply changes
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
            );
        }
    }   

    pub fn set_decorations(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_CAPTION, 0);
        } else {
            self.update_style(0, WS_CAPTION);
        }
    }
    pub fn set_maximizable(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_MAXIMIZEBOX, 0);
        } else {
            self.update_style(0, WS_MAXIMIZEBOX);
        }
    }
    pub fn set_minimizable(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_MINIMIZEBOX, 0);
        } else {
            self.update_style(0, WS_MINIMIZEBOX);
        }
    }
    pub fn set_resizable(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_THICKFRAME, 0);
        } else {
            self.update_style(0, WS_THICKFRAME);
        }
    }

}

