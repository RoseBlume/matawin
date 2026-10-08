//! Software drawing surfaces.

use std::sync::{ Arc, Mutex };

use crate::Framebuffer;
use super::base::{ Widget, WidgetExt };

#[derive(Debug, Clone)]
/// Stores the state and data for drawing area.
///
/// Visibility: **public**.
pub struct DrawingArea {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores framebuffer.
    ///
    /// Visibility: **private**.
    framebuffer: Arc<Mutex<Framebuffer>>,
}
impl DrawingArea {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            widget: Widget::new(),
            framebuffer: Arc::new(Mutex::new(Framebuffer::new(width, height))),
        }
    }
    /// Performs the framebuffer operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the framebuffer operation. Visibility: public."]
    pub fn framebuffer(&self) -> Arc<Mutex<Framebuffer>> {
        self.framebuffer.clone()
    }
    /// Performs the resize surface operation.
    ///
    /// Visibility: **public**.
    #[doc = "Resizes surface. Visibility: public."]
    pub fn resize_surface(&self, width: u32, height: u32) {
        self.framebuffer.lock().unwrap().resize(width, height);
    }
}
impl Default for DrawingArea {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new(1, 1)
    }
}
impl WidgetExt for DrawingArea {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

/// No OpenGL dependency/FFI is introduced; this provides GLArea-compatible
/// ownership semantics with a MataWin software render surface.
#[derive(Debug, Clone, Default)]
/// Stores the state and data for glarea.
///
/// Visibility: **public**.
pub struct GLArea {
    /// Stores area.
    ///
    /// Visibility: **private**.
    area: DrawingArea,
}
impl GLArea {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(width: u32, height: u32) -> Self {
        Self { area: DrawingArea::new(width, height) }
    }
    #[doc = "Implements the framebuffer operation. Visibility: public."]
    pub fn framebuffer(&self) -> Arc<Mutex<Framebuffer>> {
        self.area.framebuffer()
    }
}
impl WidgetExt for GLArea {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.area.widget()
    }
}
