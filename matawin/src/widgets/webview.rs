//! Optional public WebView wrapper and resize integration with [`WidgetWindow`].
//!
//! This module only exists when Cargo feature `webview` is enabled.

use std::{ cell::Cell, rc::Rc, sync::{ Arc, Mutex } };

use super::{
    events::{ next_id, WidgetError, WidgetId },
    geometry::{ Anchors, Rect, ResizePolicy, ResizeState },
    style::{ StyleHandle, Stylable },
    webview_platform,
    window::WidgetWindow,
};

/// The only widget rendered by a platform web engine/toolkit.
pub struct WebView {
    /// Stores id.
    ///
    /// Visibility: **private**.
    id: WidgetId,
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: Rc<webview_platform::WebView>,
    /// Stores resize.
    ///
    /// Visibility: **private**.
    resize: Arc<Mutex<ResizeState>>,
    /// Stores host size.
    ///
    /// Visibility: **private**.
    host_size: Rc<Cell<(u32, u32)>>,
    /// Stores style.
    ///
    /// Visibility: **private**.
    style: StyleHandle,
}
impl WebView {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(parent: &WidgetWindow, bounds: Rect) -> Result<Self, WidgetError> {
        let id = WidgetId(next_id());
        let inner = Rc::new(webview_platform::WebView::new(parent, id, bounds)?);
        let resize = Arc::new(Mutex::new(ResizeState::new(bounds, parent.size())));
        let style = StyleHandle::new();
        parent.register_webview(&inner, &resize, &style);
        Ok(Self { id, inner, resize, host_size: parent.host_size.clone(), style })
    }
    #[inline]
    #[doc = "Implements the id operation. Visibility: public."]
    pub const fn id(&self) -> WidgetId {
        self.id
    }
    #[inline]
    #[doc = "Implements the navigate operation. Visibility: public."]
    pub fn navigate(&self, uri: &str) -> Result<(), WidgetError> {
        self.inner.navigate(uri)
    }
    #[inline]
    #[doc = "Implements the go back operation. Visibility: public."]
    pub fn go_back(&self) {
        self.inner.go_back()
    }
    #[inline]
    #[doc = "Implements the go forward operation. Visibility: public."]
    pub fn go_forward(&self) {
        self.inner.go_forward()
    }
    #[inline]
    #[doc = "Implements the reload operation. Visibility: public."]
    pub fn reload(&self) {
        self.inner.reload()
    }
    /// Sets bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bounds. Visibility: public."]
    pub fn set_bounds(&self, bounds: Rect) {
        let mut resize = self.resize.lock().unwrap();
        resize.reset(bounds, self.host_size.get());
        drop(resize);
        let computed = self.style.computed(bounds, self.host_size.get(), None);
        self.inner.set_bounds(computed.bounds)
    }
    /// Performs the bounds operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the bounds operation. Visibility: public."]
    pub fn bounds(&self) -> Rect {
        let base = self.resize.lock().unwrap().resolve(self.host_size.get());
        self.style.computed(base, self.host_size.get(), None).bounds
    }
    /// Sets resize policy.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resize policy. Visibility: public."]
    pub fn set_resize_policy(&self, policy: ResizePolicy) {
        let mut resize = self.resize.lock().unwrap();
        let bounds = resize.resolve(self.host_size.get());
        resize.set_policy(policy, bounds, self.host_size.get())
    }
    /// Sets anchors.
    ///
    /// Visibility: **public**.
    #[doc = "Sets anchors. Visibility: public."]
    pub fn set_anchors(&self, anchors: Anchors) {
        self.set_resize_policy(ResizePolicy::Anchored(anchors))
    }
    /// Performs the style handle operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the style handle operation. Visibility: public."]
    pub fn style_handle(&self) -> StyleHandle {
        self.style.clone()
    }
}
impl Stylable for WebView {
    #[doc = "Implements the style handle operation. Visibility: private/internal."]
    fn style_handle(&self) -> StyleHandle {
        WebView::style_handle(self)
    }
}
