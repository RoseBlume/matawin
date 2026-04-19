use super::{HWND, HDC};
unsafe extern "system" {
    fn DestroyWindow(hwnd: HWND) -> i32;
    fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
}

impl Drop for super::PlatformWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.hdc.is_null() {
                ReleaseDC(self.hwnd, self.hdc);
            }

            if !self.hwnd.is_null() {
                DestroyWindow(self.hwnd);
            }
        }
    }
}