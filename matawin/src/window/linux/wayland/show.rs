impl super::Window {
    pub fn show(&self) {
        unsafe {
            // surface becomes visible after commit
            // nothing explicit needed
        }
    }

    pub fn hide(&self) {
        unsafe {
            // destroy buffer / detach
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);

        super::push_event(crate::events::WindowEvent::Resized(width, height));
    }

    pub fn reposition(&self, _x: i32, _y: i32) {
        // Not allowed in Wayland (compositor decides)
    }

    pub fn minimize(&self) {
        // Requires xdg_toplevel
    }

    pub fn maximize(&self) {
        // Requires xdg_toplevel
    }

    pub fn set_decorations(&self, _enabled: bool) {
        // compositor-controlled
    }

    pub fn set_resizable(&self, _enabled: bool) {
        // compositor-controlled
    }
}