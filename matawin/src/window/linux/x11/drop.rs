use std::ptr::null_mut;

use super::{Display, WindowHandle, GC, XImage};

#[link(name = "X11")]
unsafe extern "C" {
    fn XDestroyWindow(display: Display, window: WindowHandle);
    fn XFreeGC(display: Display, gc: GC);
    fn XCloseDisplay(display: Display);

    fn XDestroyImage(image: *mut XImage);
}

impl Drop for super::PlatformWindow {
    fn drop(&mut self) {
        unsafe {
            // Destroy image (also frees data if owned)
            if !self.image.is_null() {
                XDestroyImage(self.image);
                self.image = null_mut();
            }

            // Free graphics context
            if !self.gc.is_null() {
                XFreeGC(self.display, self.gc);
                self.gc = null_mut();
            }

            // Destroy window
            if self.window != 0 {
                XDestroyWindow(self.display, self.window);
                self.window = 0;
            }

            // Close display
            if !self.display.is_null() {
                XCloseDisplay(self.display);
                self.display = null_mut();
            }
        }
    }
}