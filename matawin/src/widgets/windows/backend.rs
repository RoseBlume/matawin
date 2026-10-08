//! Implements the `backend` module.
//!
//! This module documents both public API items and internal implementation details.

use super::*;
use super::callbacks::{ env_release, release_com, ENV_COMPLETED_VTBL };
use super::loader::load_webview2_create;

/// Stores the state and data for web view.
///
/// Visibility: **public**.
pub struct WebView {
    /// Stores state.
    ///
    /// Visibility: **private**.
    state: Arc<Mutex<WebViewState>>,
}

impl WebView {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(parent: &WidgetWindow, id: WidgetId, bounds: Rect) -> Result<Self, WidgetError> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let create = load_webview2_create()?;
        let state = Arc::new(
            Mutex::new(WebViewState {
                bounds,
                ..Default::default()
            })
        );

        let handler = Box::new(EnvironmentCompletedHandler {
            vtbl: &ENV_COMPLETED_VTBL,
            refs: AtomicU32::new(1),
            parent: HWND(parent.native_handle()),
            host_id: parent.id(),
            widget_id: id,
            state: state.clone(),
        });
        let raw_handler = Box::into_raw(handler);

        let hr = unsafe { create(PCWSTR::null(), PCWSTR::null(), ptr::null_mut(), raw_handler) };

        // Release the creator's reference. The loader owns another reference if
        // asynchronous creation started successfully.
        unsafe {
            env_release(raw_handler.cast());
        }

        if hr.0 < 0 {
            return Err(
                WidgetError(
                    format!(
                        "CreateCoreWebView2EnvironmentWithOptions failed: 0x{:08X}",
                        hr.0 as u32
                    )
                )
            );
        }

        Ok(Self { state })
    }

    /// Performs the navigate operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the navigate operation. Visibility: public."]
    pub fn navigate(&self, uri: &str) -> Result<(), WidgetError> {
        let uri = wide(uri);
        let mut state = self.state.lock().unwrap();
        if state.webview == 0 {
            state.pending_uri = Some(uri);
            return Ok(());
        }
        let webview = state.webview as *mut CoreWebView2;
        drop(state);
        let hr = unsafe { ((*(*webview).vtbl).navigate)(webview, PCWSTR(uri.as_ptr())) };
        if hr.0 < 0 {
            Err(WidgetError(format!("ICoreWebView2::Navigate failed: 0x{:08X}", hr.0 as u32)))
        } else {
            Ok(())
        }
    }

    /// Sets bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bounds. Visibility: public."]
    pub fn set_bounds(&self, bounds: Rect) {
        let mut state = self.state.lock().unwrap();
        state.bounds = bounds;
        if state.controller == 0 {
            return;
        }
        let controller = state.controller as *mut Controller;
        drop(state);
        let rect = RECT {
            left: bounds.x,
            top: bounds.y,
            right: bounds.x.saturating_add(bounds.width as i32),
            bottom: bounds.y.saturating_add(bounds.height as i32),
        };
        unsafe {
            let _ = ((*(*controller).vtbl).put_bounds)(controller, rect);
        }
    }

    /// Performs the go back operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the go back operation. Visibility: public."]
    pub fn go_back(&self) {
        let state = self.state.lock().unwrap();
        if state.webview == 0 {
            return;
        }
        let webview = state.webview as *mut CoreWebView2;
        drop(state);
        unsafe {
            let _ = ((*(*webview).vtbl).go_back)(webview);
        }
    }

    /// Performs the go forward operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the go forward operation. Visibility: public."]
    pub fn go_forward(&self) {
        let state = self.state.lock().unwrap();
        if state.webview == 0 {
            return;
        }
        let webview = state.webview as *mut CoreWebView2;
        drop(state);
        unsafe {
            let _ = ((*(*webview).vtbl).go_forward)(webview);
        }
    }

    /// Performs the reload operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the reload operation. Visibility: public."]
    pub fn reload(&self) {
        let state = self.state.lock().unwrap();
        if state.webview == 0 {
            return;
        }
        let webview = state.webview as *mut CoreWebView2;
        drop(state);
        unsafe {
            let _ = ((*(*webview).vtbl).reload)(webview);
        }
    }
}

impl Drop for WebView {
    /// Performs the drop operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the drop operation. Visibility: private/internal."]
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        let webview = state.webview as *mut CoreWebView2;
        let controller = state.controller as *mut Controller;
        state.webview = 0;
        state.controller = 0;
        drop(state);

        unsafe {
            release_com(webview);
            release_com(controller);
        }
    }
}
