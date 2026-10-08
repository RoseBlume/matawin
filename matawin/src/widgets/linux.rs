//! Linux WebKitGTK backend.
//!
//! This is the only MataWin widget module that uses GTK/WebKit FFI. Ordinary
//! controls are software-rendered by MataWin and never create GTK widgets.
//!
//! WebKitGTK API 4.1 is built on GTK 3. A GTK host is therefore required for
//! `WebKitWebView`. On Linux the WebView owns that minimal host window instead
//! of turning the rest of MataWin's controls into GTK controls.

use std::{ ffi::{ CString, c_char, c_int, c_void }, ptr };
use super::{ Rect, WidgetError, WidgetEvent, WidgetId, WidgetWindow, push_event };

/// Opaque GTK widget type used only across the Linux FFI boundary.
///
/// Visibility: **private**.
#[repr(C)]
struct GtkWidget {
    /// Prevents construction and models an opaque foreign type.
    ///
    /// Visibility: **private**.
    _private: [u8; 0],
}
/// Opaque GTK window type used only across the Linux FFI boundary.
///
/// Visibility: **private**.
#[repr(C)]
struct GtkWindow {
    /// Prevents construction and models an opaque foreign type.
    ///
    /// Visibility: **private**.
    _private: [u8; 0],
}
/// Opaque GTK container type used only across the Linux FFI boundary.
///
/// Visibility: **private**.
#[repr(C)]
struct GtkContainer {
    /// Prevents construction and models an opaque foreign type.
    ///
    /// Visibility: **private**.
    _private: [u8; 0],
}
/// Opaque WebKit web-view type used only across the Linux FFI boundary.
///
/// Visibility: **private**.
#[repr(C)]
struct WebKitWebView {
    /// Prevents construction and models an opaque foreign type.
    ///
    /// Visibility: **private**.
    _private: [u8; 0],
}
/// Constant used for gtk window toplevel.
///
/// Visibility: **private**.
const GTK_WINDOW_TOPLEVEL: c_int = 0;

#[link(name = "gtk-3")]
unsafe extern "C" {
    /// Implements gtk init check.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk init check operation. Visibility: private/internal."]
    fn gtk_init_check(argc: *mut c_int, argv: *mut *mut *mut c_char) -> c_int;
    /// Implements gtk window new.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk window new operation. Visibility: private/internal."]
    fn gtk_window_new(window_type: c_int) -> *mut GtkWidget;
    /// Implements gtk window set title.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk window set title operation. Visibility: private/internal."]
    fn gtk_window_set_title(window: *mut GtkWindow, title: *const c_char);
    /// Implements gtk window set default size.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk window set default size operation. Visibility: private/internal."]
    fn gtk_window_set_default_size(window: *mut GtkWindow, width: c_int, height: c_int);
    /// Implements gtk window resize.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk window resize operation. Visibility: private/internal."]
    fn gtk_window_resize(window: *mut GtkWindow, width: c_int, height: c_int);
    /// Implements gtk container add.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk container add operation. Visibility: private/internal."]
    fn gtk_container_add(container: *mut GtkContainer, widget: *mut GtkWidget);
    /// Implements gtk widget show all.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk widget show all operation. Visibility: private/internal."]
    fn gtk_widget_show_all(widget: *mut GtkWidget);
    /// Implements gtk widget destroy.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk widget destroy operation. Visibility: private/internal."]
    fn gtk_widget_destroy(widget: *mut GtkWidget);
    /// Implements gtk widget set size request.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the gtk widget set size request operation. Visibility: private/internal."]
    fn gtk_widget_set_size_request(widget: *mut GtkWidget, width: c_int, height: c_int);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    #[doc = "Implements the g main context iteration operation. Visibility: private/internal."]
    fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
}
#[link(name = "webkit2gtk-4.1")]
unsafe extern "C" {
    /// Implements webkit web view new.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the webkit web view new operation. Visibility: private/internal."]
    fn webkit_web_view_new() -> *mut GtkWidget;
    /// Implements webkit web view load uri.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the webkit web view load uri operation. Visibility: private/internal."]
    fn webkit_web_view_load_uri(web_view: *mut WebKitWebView, uri: *const c_char);
    /// Implements webkit web view go back.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the webkit web view go back operation. Visibility: private/internal."]
    fn webkit_web_view_go_back(web_view: *mut WebKitWebView);
    /// Implements webkit web view go forward.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the webkit web view go forward operation. Visibility: private/internal."]
    fn webkit_web_view_go_forward(web_view: *mut WebKitWebView);
    /// Implements webkit web view reload.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the webkit web view reload operation. Visibility: private/internal."]
    fn webkit_web_view_reload(web_view: *mut WebKitWebView);
}

/// Implements pump events.
///
/// Visibility: **crate-visible**.
#[doc = "Implements the pump events operation. Visibility: crate-visible."]
pub(crate) fn pump_events() {
    unsafe {
        while g_main_context_iteration(ptr::null_mut(), 0) != 0 {}
    }
}

/// Stores the state and data for web view.
///
/// Visibility: **public**.
pub struct WebView {
    /// Stores host.
    ///
    /// Visibility: **private**.
    host: *mut GtkWidget,
    /// Stores webview.
    ///
    /// Visibility: **private**.
    webview: *mut GtkWidget,
}
impl WebView {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(parent: &WidgetWindow, id: WidgetId, bounds: Rect) -> Result<Self, WidgetError> {
        if (unsafe { gtk_init_check(ptr::null_mut(), ptr::null_mut()) }) == 0 {
            return Err(WidgetError("WebKitGTK could not initialize GTK".into()));
        }
        let host = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
        let webview = unsafe { webkit_web_view_new() };
        if host.is_null() || webview.is_null() {
            if !host.is_null() {
                unsafe {
                    gtk_widget_destroy(host);
                }
            }
            return Err(WidgetError("WebKitGTK failed to create a web view".into()));
        }
        let title = CString::new("MataWin WebView").unwrap();
        unsafe {
            gtk_window_set_title(host.cast::<GtkWindow>(), title.as_ptr());
            gtk_window_set_default_size(
                host.cast::<GtkWindow>(),
                dim(bounds.width),
                dim(bounds.height)
            );
            gtk_widget_set_size_request(webview, dim(bounds.width), dim(bounds.height));
            gtk_container_add(host.cast::<GtkContainer>(), webview);
            gtk_widget_show_all(host);
        }
        push_event(parent.id(), WidgetEvent::WebViewReady(id));
        Ok(Self { host, webview })
    }
    /// Performs the navigate operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the navigate operation. Visibility: public."]
    pub fn navigate(&self, uri: &str) -> Result<(), WidgetError> {
        let uri = CString::new(uri).map_err(|_| WidgetError("WebView URI contains NUL".into()))?;
        unsafe {
            webkit_web_view_load_uri(self.webview.cast::<WebKitWebView>(), uri.as_ptr());
        }
        Ok(())
    }
    /// Sets bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bounds. Visibility: public."]
    pub fn set_bounds(&self, bounds: Rect) {
        unsafe {
            gtk_window_resize(self.host.cast::<GtkWindow>(), dim(bounds.width), dim(bounds.height));
            gtk_widget_set_size_request(self.webview, dim(bounds.width), dim(bounds.height));
        }
    }
    /// Performs the go back operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the go back operation. Visibility: public."]
    pub fn go_back(&self) {
        unsafe { webkit_web_view_go_back(self.webview.cast::<WebKitWebView>()) }
    }
    /// Performs the go forward operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the go forward operation. Visibility: public."]
    pub fn go_forward(&self) {
        unsafe { webkit_web_view_go_forward(self.webview.cast::<WebKitWebView>()) }
    }
    /// Performs the reload operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the reload operation. Visibility: public."]
    pub fn reload(&self) {
        unsafe { webkit_web_view_reload(self.webview.cast::<WebKitWebView>()) }
    }
}
impl Drop for WebView {
    #[doc = "Implements the drop operation. Visibility: private/internal."]
    fn drop(&mut self) {
        if !self.host.is_null() {
            unsafe {
                gtk_widget_destroy(self.host);
            }
            self.host = ptr::null_mut();
            self.webview = ptr::null_mut();
        }
    }
}
#[inline]
#[doc = "Implements the dim operation. Visibility: private/internal."]
fn dim(v: u32) -> c_int {
    v.max(1).min(c_int::MAX as u32) as c_int
}
