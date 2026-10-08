//! Windows WebView2 backend. Ordinary widgets are MataWin software widgets.

use std::{ ffi::c_void, mem, ptr, sync::{ Arc, Mutex }, sync::atomic::{ AtomicU32, Ordering } };
use ::windows::{
    core::{ GUID, HRESULT, PCSTR, PCWSTR },
    Win32::{
        Foundation::{ HWND, RECT },
        System::{
            Com::{ COINIT_APARTMENTTHREADED, CoInitializeEx },
            LibraryLoader::{ GetProcAddress, LoadLibraryW },
        },
    },
};
use super::{ Rect, WidgetError, WidgetEvent, WidgetId, WidgetWindow, push_event };

#[inline]
#[doc = "Implements the pump events operation. Visibility: crate-visible."]
pub(crate) fn pump_events() {}
/// Implements wide.
///
/// Visibility: **private**.
#[doc = "Implements the wide operation. Visibility: private/internal."]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---- Minimal WebView2 ABI -------------------------------------------------
//
// WebView2 is not part of the Windows SDK metadata consumed by the `windows`
// crate, so using only that crate means defining the small COM ABI surface we
// need and dynamically loading Microsoft's WebView2Loader.dll. No third-party
// Rust WebView2 crate is used.

/// Constant used for iid iunknown.
///
/// Visibility: **private**.
const IID_IUNKNOWN: GUID = GUID::from_u128(0x0000000000000000c000000000000046);
/// Constant used for iid environment completed handler.
///
/// Visibility: **private**.
const IID_ENVIRONMENT_COMPLETED_HANDLER: GUID = GUID::from_u128(0x4e8a3389c9d84bd2b6b5124fee6cc14d);
/// Constant used for iid controller completed handler.
///
/// Visibility: **private**.
const IID_CONTROLLER_COMPLETED_HANDLER: GUID = GUID::from_u128(0x6c4819f3c9b742608127c9f5bde7f68c);
/// Constant used for e pointer.
///
/// Visibility: **private**.
const E_POINTER: HRESULT = HRESULT(0x80004003u32 as i32);
/// Constant used for e nointerface.
///
/// Visibility: **private**.
const E_NOINTERFACE: HRESULT = HRESULT(0x80004002u32 as i32);

#[repr(C)]
/// Stores the state and data for iunknown vtbl.
///
/// Visibility: **private**.
struct IUnknownVtbl {
    /// Stores query interface.
    ///
    /// Visibility: **private**.
    query_interface: unsafe extern "system" fn(
        *mut c_void,
        *const GUID,
        *mut *mut c_void
    ) -> HRESULT,
    /// Stores add ref.
    ///
    /// Visibility: **private**.
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    /// Stores release.
    ///
    /// Visibility: **private**.
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
/// Stores the state and data for environment vtbl.
///
/// Visibility: **private**.
struct EnvironmentVtbl {
    /// Stores unknown.
    ///
    /// Visibility: **private**.
    unknown: IUnknownVtbl,
    /// Stores create core webview2 controller.
    ///
    /// Visibility: **private**.
    create_core_webview2_controller: unsafe extern "system" fn(
        *mut Environment,
        HWND,
        *mut EnvironmentControllerCompletedHandler
    ) -> HRESULT,
    // Remaining ICoreWebView2Environment methods are intentionally omitted.
}

#[repr(C)]
/// Stores the state and data for environment.
///
/// Visibility: **private**.
struct Environment {
    /// Stores vtbl.
    ///
    /// Visibility: **private**.
    vtbl: *const EnvironmentVtbl,
}

#[repr(C)]
/// Stores the state and data for controller vtbl.
///
/// Visibility: **private**.
struct ControllerVtbl {
    /// Stores unknown.
    ///
    /// Visibility: **private**.
    unknown: IUnknownVtbl,
    /// Stores get is visible.
    ///
    /// Visibility: **private**.
    get_is_visible: usize,
    /// Stores put is visible.
    ///
    /// Visibility: **private**.
    put_is_visible: usize,
    /// Stores get bounds.
    ///
    /// Visibility: **private**.
    get_bounds: usize,
    /// Stores put bounds.
    ///
    /// Visibility: **private**.
    put_bounds: unsafe extern "system" fn(*mut Controller, RECT) -> HRESULT,
    /// Stores get zoom factor.
    ///
    /// Visibility: **private**.
    get_zoom_factor: usize,
    /// Stores put zoom factor.
    ///
    /// Visibility: **private**.
    put_zoom_factor: usize,
    /// Stores add zoom factor changed.
    ///
    /// Visibility: **private**.
    add_zoom_factor_changed: usize,
    /// Stores remove zoom factor changed.
    ///
    /// Visibility: **private**.
    remove_zoom_factor_changed: usize,
    /// Stores set bounds and zoom factor.
    ///
    /// Visibility: **private**.
    set_bounds_and_zoom_factor: usize,
    /// Stores move focus.
    ///
    /// Visibility: **private**.
    move_focus: usize,
    /// Stores add move focus requested.
    ///
    /// Visibility: **private**.
    add_move_focus_requested: usize,
    /// Stores remove move focus requested.
    ///
    /// Visibility: **private**.
    remove_move_focus_requested: usize,
    /// Stores add got focus.
    ///
    /// Visibility: **private**.
    add_got_focus: usize,
    /// Stores remove got focus.
    ///
    /// Visibility: **private**.
    remove_got_focus: usize,
    /// Stores add lost focus.
    ///
    /// Visibility: **private**.
    add_lost_focus: usize,
    /// Stores remove lost focus.
    ///
    /// Visibility: **private**.
    remove_lost_focus: usize,
    /// Stores add accelerator key pressed.
    ///
    /// Visibility: **private**.
    add_accelerator_key_pressed: usize,
    /// Stores remove accelerator key pressed.
    ///
    /// Visibility: **private**.
    remove_accelerator_key_pressed: usize,
    /// Stores get parent window.
    ///
    /// Visibility: **private**.
    get_parent_window: usize,
    /// Stores put parent window.
    ///
    /// Visibility: **private**.
    put_parent_window: usize,
    /// Stores notify parent window position changed.
    ///
    /// Visibility: **private**.
    notify_parent_window_position_changed: usize,
    /// Stores close.
    ///
    /// Visibility: **private**.
    close: usize,
    /// Stores get core webview2.
    ///
    /// Visibility: **private**.
    get_core_webview2: unsafe extern "system" fn(
        *mut Controller,
        *mut *mut CoreWebView2
    ) -> HRESULT,
}

#[repr(C)]
/// Stores the state and data for controller.
///
/// Visibility: **private**.
struct Controller {
    /// Stores vtbl.
    ///
    /// Visibility: **private**.
    vtbl: *const ControllerVtbl,
}

#[repr(C)]
/// Stores the state and data for core web view2 vtbl.
///
/// Visibility: **private**.
struct CoreWebView2Vtbl {
    /// Stores unknown.
    ///
    /// Visibility: **private**.
    unknown: IUnknownVtbl,
    /// Stores get settings.
    ///
    /// Visibility: **private**.
    get_settings: usize,
    /// Stores get source.
    ///
    /// Visibility: **private**.
    get_source: usize,
    /// Stores navigate.
    ///
    /// Visibility: **private**.
    navigate: unsafe extern "system" fn(*mut CoreWebView2, PCWSTR) -> HRESULT,
    /// Stores navigate to string.
    ///
    /// Visibility: **private**.
    navigate_to_string: usize,
    /// Stores add navigation starting.
    ///
    /// Visibility: **private**.
    add_navigation_starting: usize,
    /// Stores remove navigation starting.
    ///
    /// Visibility: **private**.
    remove_navigation_starting: usize,
    /// Stores add content loading.
    ///
    /// Visibility: **private**.
    add_content_loading: usize,
    /// Stores remove content loading.
    ///
    /// Visibility: **private**.
    remove_content_loading: usize,
    /// Stores add source changed.
    ///
    /// Visibility: **private**.
    add_source_changed: usize,
    /// Stores remove source changed.
    ///
    /// Visibility: **private**.
    remove_source_changed: usize,
    /// Stores add history changed.
    ///
    /// Visibility: **private**.
    add_history_changed: usize,
    /// Stores remove history changed.
    ///
    /// Visibility: **private**.
    remove_history_changed: usize,
    /// Stores add navigation completed.
    ///
    /// Visibility: **private**.
    add_navigation_completed: usize,
    /// Stores remove navigation completed.
    ///
    /// Visibility: **private**.
    remove_navigation_completed: usize,
    /// Stores add frame navigation starting.
    ///
    /// Visibility: **private**.
    add_frame_navigation_starting: usize,
    /// Stores remove frame navigation starting.
    ///
    /// Visibility: **private**.
    remove_frame_navigation_starting: usize,
    /// Stores add frame navigation completed.
    ///
    /// Visibility: **private**.
    add_frame_navigation_completed: usize,
    /// Stores remove frame navigation completed.
    ///
    /// Visibility: **private**.
    remove_frame_navigation_completed: usize,
    /// Stores add script dialog opening.
    ///
    /// Visibility: **private**.
    add_script_dialog_opening: usize,
    /// Stores remove script dialog opening.
    ///
    /// Visibility: **private**.
    remove_script_dialog_opening: usize,
    /// Stores add permission requested.
    ///
    /// Visibility: **private**.
    add_permission_requested: usize,
    /// Stores remove permission requested.
    ///
    /// Visibility: **private**.
    remove_permission_requested: usize,
    /// Stores add process failed.
    ///
    /// Visibility: **private**.
    add_process_failed: usize,
    /// Stores remove process failed.
    ///
    /// Visibility: **private**.
    remove_process_failed: usize,
    /// Stores add script to execute on document created.
    ///
    /// Visibility: **private**.
    add_script_to_execute_on_document_created: usize,
    /// Stores remove script to execute on document created.
    ///
    /// Visibility: **private**.
    remove_script_to_execute_on_document_created: usize,
    /// Stores execute script.
    ///
    /// Visibility: **private**.
    execute_script: usize,
    /// Stores capture preview.
    ///
    /// Visibility: **private**.
    capture_preview: usize,
    /// Stores reload.
    ///
    /// Visibility: **private**.
    reload: unsafe extern "system" fn(*mut CoreWebView2) -> HRESULT,
    /// Stores post web message as json.
    ///
    /// Visibility: **private**.
    post_web_message_as_json: usize,
    /// Stores post web message as string.
    ///
    /// Visibility: **private**.
    post_web_message_as_string: usize,
    /// Stores add web message received.
    ///
    /// Visibility: **private**.
    add_web_message_received: usize,
    /// Stores remove web message received.
    ///
    /// Visibility: **private**.
    remove_web_message_received: usize,
    /// Stores call dev tools protocol method.
    ///
    /// Visibility: **private**.
    call_dev_tools_protocol_method: usize,
    /// Stores get browser process id.
    ///
    /// Visibility: **private**.
    get_browser_process_id: usize,
    /// Stores get can go back.
    ///
    /// Visibility: **private**.
    get_can_go_back: usize,
    /// Stores get can go forward.
    ///
    /// Visibility: **private**.
    get_can_go_forward: usize,
    /// Stores go back.
    ///
    /// Visibility: **private**.
    go_back: unsafe extern "system" fn(*mut CoreWebView2) -> HRESULT,
    /// Stores go forward.
    ///
    /// Visibility: **private**.
    go_forward: unsafe extern "system" fn(*mut CoreWebView2) -> HRESULT,
    /// Stores get dev tools protocol event receiver.
    ///
    /// Visibility: **private**.
    get_dev_tools_protocol_event_receiver: usize,
    /// Stores stop.
    ///
    /// Visibility: **private**.
    stop: usize,
    /// Stores add new window requested.
    ///
    /// Visibility: **private**.
    add_new_window_requested: usize,
    /// Stores remove new window requested.
    ///
    /// Visibility: **private**.
    remove_new_window_requested: usize,
    /// Stores add document title changed.
    ///
    /// Visibility: **private**.
    add_document_title_changed: usize,
    /// Stores remove document title changed.
    ///
    /// Visibility: **private**.
    remove_document_title_changed: usize,
    /// Stores get document title.
    ///
    /// Visibility: **private**.
    get_document_title: usize,
    /// Stores add host object to script.
    ///
    /// Visibility: **private**.
    add_host_object_to_script: usize,
    /// Stores remove host object from script.
    ///
    /// Visibility: **private**.
    remove_host_object_from_script: usize,
    /// Stores open dev tools window.
    ///
    /// Visibility: **private**.
    open_dev_tools_window: usize,
    /// Stores add contains full screen element changed.
    ///
    /// Visibility: **private**.
    add_contains_full_screen_element_changed: usize,
    /// Stores remove contains full screen element changed.
    ///
    /// Visibility: **private**.
    remove_contains_full_screen_element_changed: usize,
    /// Stores get contains full screen element.
    ///
    /// Visibility: **private**.
    get_contains_full_screen_element: usize,
    /// Stores add web resource requested.
    ///
    /// Visibility: **private**.
    add_web_resource_requested: usize,
    /// Stores remove web resource requested.
    ///
    /// Visibility: **private**.
    remove_web_resource_requested: usize,
    /// Stores add web resource requested filter.
    ///
    /// Visibility: **private**.
    add_web_resource_requested_filter: usize,
    /// Stores remove web resource requested filter.
    ///
    /// Visibility: **private**.
    remove_web_resource_requested_filter: usize,
    /// Stores add window close requested.
    ///
    /// Visibility: **private**.
    add_window_close_requested: usize,
    /// Stores remove window close requested.
    ///
    /// Visibility: **private**.
    remove_window_close_requested: usize,
}

#[repr(C)]
/// Stores the state and data for core web view2.
///
/// Visibility: **private**.
struct CoreWebView2 {
    /// Stores vtbl.
    ///
    /// Visibility: **private**.
    vtbl: *const CoreWebView2Vtbl,
}

#[derive(Default)]
/// Stores the state and data for web view state.
///
/// Visibility: **private**.
struct WebViewState {
    /// Stores controller.
    ///
    /// Visibility: **private**.
    controller: usize,
    /// Stores webview.
    ///
    /// Visibility: **private**.
    webview: usize,
    /// Stores bounds.
    ///
    /// Visibility: **private**.
    bounds: Rect,
    /// Stores pending uri.
    ///
    /// Visibility: **private**.
    pending_uri: Option<Vec<u16>>,
    /// Stores closed.
    ///
    /// Visibility: **private**.
    closed: bool,
}

#[repr(C)]
/// Stores the state and data for environment completed vtbl.
///
/// Visibility: **private**.
struct EnvironmentCompletedVtbl {
    /// Stores unknown.
    ///
    /// Visibility: **private**.
    unknown: IUnknownVtbl,
    /// Stores invoke.
    ///
    /// Visibility: **private**.
    invoke: unsafe extern "system" fn(
        *mut EnvironmentCompletedHandler,
        HRESULT,
        *mut Environment
    ) -> HRESULT,
}

#[repr(C)]
/// Stores the state and data for environment completed handler.
///
/// Visibility: **private**.
struct EnvironmentCompletedHandler {
    /// Stores vtbl.
    ///
    /// Visibility: **private**.
    vtbl: *const EnvironmentCompletedVtbl,
    /// Stores refs.
    ///
    /// Visibility: **private**.
    refs: AtomicU32,
    /// Stores parent.
    ///
    /// Visibility: **private**.
    parent: HWND,
    /// Stores host id.
    ///
    /// Visibility: **private**.
    host_id: u64,
    /// Stores widget id.
    ///
    /// Visibility: **private**.
    widget_id: WidgetId,
    /// Stores state.
    ///
    /// Visibility: **private**.
    state: Arc<Mutex<WebViewState>>,
}

#[repr(C)]
/// Stores the state and data for environment controller completed vtbl.
///
/// Visibility: **private**.
struct EnvironmentControllerCompletedVtbl {
    /// Stores unknown.
    ///
    /// Visibility: **private**.
    unknown: IUnknownVtbl,
    /// Stores invoke.
    ///
    /// Visibility: **private**.
    invoke: unsafe extern "system" fn(
        *mut EnvironmentControllerCompletedHandler,
        HRESULT,
        *mut Controller
    ) -> HRESULT,
}

#[repr(C)]
/// Stores the state and data for environment controller completed handler.
///
/// Visibility: **private**.
struct EnvironmentControllerCompletedHandler {
    /// Stores vtbl.
    ///
    /// Visibility: **private**.
    vtbl: *const EnvironmentControllerCompletedVtbl,
    /// Stores refs.
    ///
    /// Visibility: **private**.
    refs: AtomicU32,
    /// Stores host id.
    ///
    /// Visibility: **private**.
    host_id: u64,
    /// Stores widget id.
    ///
    /// Visibility: **private**.
    widget_id: WidgetId,
    /// Stores state.
    ///
    /// Visibility: **private**.
    state: Arc<Mutex<WebViewState>>,
}

/// Contains the callbacks implementation.
///
/// Visibility: **private**.
mod callbacks;
/// Contains the loader implementation.
///
/// Visibility: **private**.
mod loader;
/// Contains the backend implementation.
///
/// Visibility: **private**.
mod backend;

pub(crate) use backend::WebView;
