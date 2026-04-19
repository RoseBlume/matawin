use super::{MSG, TranslateMessage, DispatchMessageA, PeekMessageA};

impl super::PlatformWindow {
    pub fn pump_events(&self) {
        let mut msg = MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt_x: 0,
            pt_y: 0,
        };

        unsafe {
            while PeekMessageA(&mut msg, std::ptr::null_mut(), 0, 0, 1) != 0 {
                TranslateMessage(&msg);
                DispatchMessageA(&msg);
            }
        }
    }    
}