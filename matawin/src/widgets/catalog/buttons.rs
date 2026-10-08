//! Toggle, check, and switch widgets.

use std::sync::{ Arc, Mutex };

use super::base::{ Widget, WidgetExt };

#[derive(Debug, Clone)]
/// Stores the state and data for toggle button.
///
/// Visibility: **public**.
pub struct ToggleButton {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores label.
    ///
    /// Visibility: **private**.
    label: Arc<Mutex<String>>,
    /// Stores active.
    ///
    /// Visibility: **private**.
    active: Arc<Mutex<bool>>,
}
impl ToggleButton {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            widget: Widget::new(),
            label: Arc::new(Mutex::new(label.into())),
            active: Arc::new(Mutex::new(false)),
        }
    }
    /// Performs the active operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the active operation. Visibility: public."]
    pub fn active(&self) -> bool {
        *self.active.lock().unwrap()
    }
    /// Sets active.
    ///
    /// Visibility: **public**.
    #[doc = "Sets active. Visibility: public."]
    pub fn set_active(&self, active: bool) {
        *self.active.lock().unwrap() = active;
    }
    /// Performs the label operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the label operation. Visibility: public."]
    pub fn label(&self) -> String {
        self.label.lock().unwrap().clone()
    }
}
impl WidgetExt for ToggleButton {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for check button.
///
/// Visibility: **public**.
pub struct CheckButton {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: ToggleButton,
}
impl CheckButton {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(label: impl Into<String>) -> Self {
        Self { inner: ToggleButton::new(label) }
    }
    #[doc = "Implements the active operation. Visibility: public."]
    pub fn active(&self) -> bool {
        self.inner.active()
    }
    #[doc = "Sets active. Visibility: public."]
    pub fn set_active(&self, v: bool) {
        self.inner.set_active(v);
    }
}
impl WidgetExt for CheckButton {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.inner.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for switch.
///
/// Visibility: **public**.
pub struct Switch {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores active.
    ///
    /// Visibility: **private**.
    active: Arc<Mutex<bool>>,
}
impl Default for Switch {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Switch {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self { widget: Widget::new(), active: Arc::new(Mutex::new(false)) }
    }
    #[doc = "Implements the active operation. Visibility: public."]
    pub fn active(&self) -> bool {
        *self.active.lock().unwrap()
    }
    #[doc = "Sets active. Visibility: public."]
    pub fn set_active(&self, v: bool) {
        *self.active.lock().unwrap() = v;
    }
}
impl WidgetExt for Switch {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}
