//! Implements the `callbacks` module.
//!
//! This module documents both public API items and internal implementation details.

use super::*;

/// Implements env query interface.
///
/// Visibility: **private**.
#[doc = "Implements the env query interface operation. Visibility: private/internal."]
unsafe extern "system" fn env_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    object: *mut *mut c_void
) -> HRESULT {
    if object.is_null() || iid.is_null() {
        return E_POINTER;
    }

    unsafe {
        *object = ptr::null_mut();
    }
    let iid = unsafe { *iid };
    if iid != IID_IUNKNOWN && iid != IID_ENVIRONMENT_COMPLETED_HANDLER {
        return E_NOINTERFACE;
    }

    unsafe {
        *object = this;
    }
    unsafe {
        env_add_ref(this);
    }
    HRESULT(0)
}

/// Implements env add ref.
///
/// Visibility: **private**.
#[doc = "Implements the env add ref operation. Visibility: private/internal."]
unsafe extern "system" fn env_add_ref(this: *mut c_void) -> u32 {
    let this = this.cast::<EnvironmentCompletedHandler>();
    unsafe { (*this).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}

/// Implements env release.
///
/// Visibility: **parent-module-visible**.
#[doc = "Implements the env release operation. Visibility: parent-module-visible."]
pub(super) unsafe extern "system" fn env_release(this: *mut c_void) -> u32 {
    let this = this.cast::<EnvironmentCompletedHandler>();
    let remaining = unsafe { (*this).refs.fetch_sub(1, Ordering::Release) - 1 };
    if remaining == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        unsafe {
            drop(Box::from_raw(this));
        }
    }
    remaining
}

/// Implements controller query interface.
///
/// Visibility: **private**.
#[doc = "Implements the controller query interface operation. Visibility: private/internal."]
unsafe extern "system" fn controller_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    object: *mut *mut c_void
) -> HRESULT {
    if object.is_null() || iid.is_null() {
        return E_POINTER;
    }

    unsafe {
        *object = ptr::null_mut();
    }
    let iid = unsafe { *iid };
    if iid != IID_IUNKNOWN && iid != IID_CONTROLLER_COMPLETED_HANDLER {
        return E_NOINTERFACE;
    }

    unsafe {
        *object = this;
    }
    unsafe {
        controller_add_ref(this);
    }
    HRESULT(0)
}

/// Implements controller add ref.
///
/// Visibility: **private**.
#[doc = "Implements the controller add ref operation. Visibility: private/internal."]
unsafe extern "system" fn controller_add_ref(this: *mut c_void) -> u32 {
    let this = this.cast::<EnvironmentControllerCompletedHandler>();
    unsafe { (*this).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}

/// Implements controller release.
///
/// Visibility: **private**.
#[doc = "Implements the controller release operation. Visibility: private/internal."]
unsafe extern "system" fn controller_release(this: *mut c_void) -> u32 {
    let this = this.cast::<EnvironmentControllerCompletedHandler>();
    let remaining = unsafe { (*this).refs.fetch_sub(1, Ordering::Release) - 1 };
    if remaining == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        unsafe {
            drop(Box::from_raw(this));
        }
    }
    remaining
}

/// Static storage used for env completed vtbl.
///
/// Visibility: **parent-module-visible**.
pub(super) static ENV_COMPLETED_VTBL: EnvironmentCompletedVtbl = EnvironmentCompletedVtbl {
    unknown: IUnknownVtbl {
        query_interface: env_query_interface,
        add_ref: env_add_ref,
        release: env_release,
    },
    invoke: environment_completed,
};

/// Static storage used for controller completed vtbl.
///
/// Visibility: **private**.
static CONTROLLER_COMPLETED_VTBL: EnvironmentControllerCompletedVtbl =
    EnvironmentControllerCompletedVtbl {
        unknown: IUnknownVtbl {
            query_interface: controller_query_interface,
            add_ref: controller_add_ref,
            release: controller_release,
        },
        invoke: controller_completed,
    };

/// Implements release com.
///
/// Visibility: **parent-module-visible**.
#[doc = "Implements the release com operation. Visibility: parent-module-visible."]
pub(super) unsafe fn release_com<T>(object: *mut T) {
    if object.is_null() {
        return;
    }
    let unknown = unsafe { *object.cast::<*const IUnknownVtbl>() };
    unsafe { ((*unknown).release)(object.cast()) }
}

/// Adds ref com.
///
/// Visibility: **private**.
#[doc = "Adds ref com. Visibility: private/internal."]
unsafe fn add_ref_com<T>(object: *mut T) {
    if object.is_null() {
        return;
    }
    let unknown = unsafe { *object.cast::<*const IUnknownVtbl>() };
    unsafe { ((*unknown).add_ref)(object.cast()) }
}

/// Implements environment completed.
///
/// Visibility: **private**.
#[doc = "Implements the environment completed operation. Visibility: private/internal."]
unsafe extern "system" fn environment_completed(
    this: *mut EnvironmentCompletedHandler,
    error_code: HRESULT,
    environment: *mut Environment
) -> HRESULT {
    if error_code.0 < 0 || environment.is_null() {
        let this_ref = unsafe { &*this };
        push_event(
            this_ref.host_id,
            WidgetEvent::WebViewError(
                this_ref.widget_id,
                format!("WebView2 environment creation failed: 0x{:08X}", error_code.0 as u32)
            )
        );
        return HRESULT(0);
    }

    let this_ref = unsafe { &*this };
    let handler = Box::new(EnvironmentControllerCompletedHandler {
        vtbl: &CONTROLLER_COMPLETED_VTBL,
        refs: AtomicU32::new(1),
        host_id: this_ref.host_id,
        widget_id: this_ref.widget_id,
        state: this_ref.state.clone(),
    });
    let raw_handler = Box::into_raw(handler);

    let hr = unsafe {
        ((*(*environment).vtbl).create_core_webview2_controller)(
            environment,
            this_ref.parent,
            raw_handler
        )
    };

    // Drop our creator reference. WebView2 keeps its own reference while the
    // asynchronous operation is outstanding.
    unsafe {
        controller_release(raw_handler.cast());
    }

    if hr.0 < 0 {
        push_event(
            this_ref.host_id,
            WidgetEvent::WebViewError(
                this_ref.widget_id,
                format!("CreateCoreWebView2Controller failed: 0x{:08X}", hr.0 as u32)
            )
        );
    }
    HRESULT(0)
}

/// Implements controller completed.
///
/// Visibility: **private**.
#[doc = "Implements the controller completed operation. Visibility: private/internal."]
unsafe extern "system" fn controller_completed(
    this: *mut EnvironmentControllerCompletedHandler,
    error_code: HRESULT,
    controller: *mut Controller
) -> HRESULT {
    let this_ref = unsafe { &*this };
    if error_code.0 < 0 || controller.is_null() {
        push_event(
            this_ref.host_id,
            WidgetEvent::WebViewError(
                this_ref.widget_id,
                format!("WebView2 controller creation failed: 0x{:08X}", error_code.0 as u32)
            )
        );
        return HRESULT(0);
    }

    unsafe {
        add_ref_com(controller);
    }
    let mut webview: *mut CoreWebView2 = ptr::null_mut();
    let hr = unsafe { ((*(*controller).vtbl).get_core_webview2)(controller, &mut webview) };
    if hr.0 < 0 || webview.is_null() {
        unsafe {
            release_com(controller);
        }
        push_event(
            this_ref.host_id,
            WidgetEvent::WebViewError(
                this_ref.widget_id,
                format!("ICoreWebView2Controller::get_CoreWebView2 failed: 0x{:08X}", hr.0 as u32)
            )
        );
        return HRESULT(0);
    }

    let mut state = this_ref.state.lock().unwrap();
    if state.closed {
        unsafe {
            release_com(webview);
            release_com(controller);
        }
        return HRESULT(0);
    }

    state.controller = controller as usize;
    state.webview = webview as usize;
    let bounds = state.bounds;
    let pending = state.pending_uri.take();
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

    if let Some(uri) = pending {
        unsafe {
            let _ = ((*(*webview).vtbl).navigate)(webview, PCWSTR(uri.as_ptr()));
        }
    }

    push_event(this_ref.host_id, WidgetEvent::WebViewReady(this_ref.widget_id));
    HRESULT(0)
}
