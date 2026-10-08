//! Implements the `show` module.
//!
//! This module documents both public API items and internal implementation details.

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{ LPARAM, RECT, WPARAM },
        UI::WindowsAndMessaging::{
            HICON,
            AdjustWindowRectEx,
            DestroyIcon,
            GetWindowLongPtrW,
            LoadImageW,
            SendMessageW,
            SetWindowLongPtrW,
            SetWindowPos,
            ShowWindow,
            GWL_STYLE,
            ICON_BIG,
            ICON_SMALL,
            IMAGE_ICON,
            LR_DEFAULTSIZE,
            LR_LOADFROMFILE,
            SW_HIDE,
            SW_MAXIMIZE,
            SW_MINIMIZE,
            SW_RESTORE,
            SW_SHOW,
            SWP_FRAMECHANGED,
            SWP_NOMOVE,
            SWP_NOSIZE,
            SWP_NOZORDER,
            WM_SETICON,
            WS_CAPTION,
            WS_MAXIMIZEBOX,
            WS_MINIMIZEBOX,
            WS_THICKFRAME,
            WINDOW_EX_STYLE,
            WINDOW_STYLE,
        },
    },
};

/// Implements wide null.
///
/// Visibility: **private**.
#[doc = "Implements the wide null operation. Visibility: private/internal."]
fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

impl super::PlatformWindow {
    /// Performs the hide operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the hide operation. Visibility: public."]
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Sets icon.
    ///
    /// Visibility: **public**.
    #[doc = "Sets icon. Visibility: public."]
    pub fn set_icon(&self, path: &str) {
        let path = wide_null(path);

        unsafe {
            let handle = LoadImageW(
                None,
                PCWSTR(path.as_ptr()),
                IMAGE_ICON,
                0,
                0,
                LR_LOADFROMFILE | LR_DEFAULTSIZE
            ).expect("LoadImageW failed to load the window icon");

            let icon = HICON(handle.0);

            SendMessageW(
                self.hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icon.0 as isize))
            );
            SendMessageW(
                self.hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(icon.0 as isize))
            );

            if let Some(old) = self.icon.replace(Some(icon)) {
                let _ = DestroyIcon(old);
            }
        }
    }

    /// Performs the maximize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the maximize operation. Visibility: public."]
    pub fn maximize(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_MAXIMIZE);
        }
        self.refresh_client_size();
    }

    /// Performs the minimize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the minimize operation. Visibility: public."]
    pub fn minimize(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
        }
        self.refresh_client_size();
    }

    /// Performs the restore operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the restore operation. Visibility: public."]
    pub fn restore(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_RESTORE);
        }
        self.refresh_client_size();
    }

    /// Performs the reposition operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the reposition operation. Visibility: public."]
    pub fn reposition(&self, x: i32, y: i32) {
        unsafe {
            SetWindowPos(self.hwnd, None, x, y, 0, 0, SWP_NOZORDER | SWP_NOSIZE).expect(
                "SetWindowPos failed while repositioning the window"
            );
        }
    }

    /// Performs the resize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resize operation. Visibility: public."]
    pub fn resize(&self, width: u32, height: u32) {
        unsafe {
            let style = WINDOW_STYLE(GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32);
            let ex_style = WINDOW_EX_STYLE(0);

            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };

            AdjustWindowRectEx(&mut rect, style, false, ex_style).expect(
                "AdjustWindowRectEx failed while resizing the window"
            );

            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOMOVE
            ).expect("SetWindowPos failed while resizing the window");
        }

        // resize() specifies the desired client area, so this avoids an extra
        // GetClientRect call in the common explicit-resize path.
        self.client_size.set((width, height));
    }

    /// Performs the show operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the show operation. Visibility: public."]
    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
        }
        self.refresh_client_size();
    }

    /// Updates style.
    ///
    /// Visibility: **private**.
    #[doc = "Updates style. Visibility: private/internal."]
    fn update_style(&self, add: WINDOW_STYLE, remove: WINDOW_STYLE) {
        unsafe {
            let current = GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32;
            let updated = (current | add.0) & !remove.0;

            SetWindowLongPtrW(self.hwnd, GWL_STYLE, updated as isize);

            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED
            ).expect("SetWindowPos failed while applying a window style change");
        }

        self.refresh_client_size();
    }

    /// Sets decorations.
    ///
    /// Visibility: **public**.
    #[doc = "Sets decorations. Visibility: public."]
    pub fn set_decorations(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_CAPTION, WINDOW_STYLE(0));
        } else {
            self.update_style(WINDOW_STYLE(0), WS_CAPTION);
        }
    }

    /// Sets maximizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets maximizable. Visibility: public."]
    pub fn set_maximizable(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_MAXIMIZEBOX, WINDOW_STYLE(0));
        } else {
            self.update_style(WINDOW_STYLE(0), WS_MAXIMIZEBOX);
        }
    }

    /// Sets minimizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets minimizable. Visibility: public."]
    pub fn set_minimizable(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_MINIMIZEBOX, WINDOW_STYLE(0));
        } else {
            self.update_style(WINDOW_STYLE(0), WS_MINIMIZEBOX);
        }
    }

    /// Sets resizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resizable. Visibility: public."]
    pub fn set_resizable(&self, enabled: bool) {
        if enabled {
            self.update_style(WS_THICKFRAME, WINDOW_STYLE(0));
        } else {
            self.update_style(WINDOW_STYLE(0), WS_THICKFRAME);
        }
    }
}
