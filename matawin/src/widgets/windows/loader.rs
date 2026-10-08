//! Implements the `loader` module.
//!
//! This module documents both public API items and internal implementation details.

use super::*;

/// Type alias for create environment with options.
///
/// Visibility: **private**.
type CreateEnvironmentWithOptions = unsafe extern "system" fn(
    PCWSTR,
    PCWSTR,
    *mut c_void,
    *mut EnvironmentCompletedHandler
) -> HRESULT;

/// Loads webview2 create.
///
/// Visibility: **parent-module-visible**.
#[doc = "Loads webview2 create. Visibility: parent-module-visible."]
pub(super) fn load_webview2_create() -> Result<CreateEnvironmentWithOptions, WidgetError> {
    let module = (unsafe { LoadLibraryW(::windows::core::w!("WebView2Loader.dll")) }).map_err(|e|
        WidgetError(
            format!(
                "could not load WebView2Loader.dll ({e}). Put Microsoft's WebView2Loader.dll beside the executable and install the WebView2 Runtime"
            )
        )
    )?;

    let proc = (
        unsafe {
            GetProcAddress(module, PCSTR(b"CreateCoreWebView2EnvironmentWithOptions\0".as_ptr()))
        }
    ).ok_or_else(||
        WidgetError(
            "WebView2Loader.dll does not export CreateCoreWebView2EnvironmentWithOptions".into()
        )
    )?;

    Ok(unsafe { mem::transmute(proc) })
}
