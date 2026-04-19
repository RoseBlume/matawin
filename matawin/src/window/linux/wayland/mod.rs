use std::ffi::{c_void, CString};
use std::mem::size_of;
use std::os::fd::FromRawFd;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use crate::buffer::Framebuffer;
use crate::events::{WindowEvent, Key};

mod events;
mod show;
mod drop;

pub static EVENT_QUEUE: OnceLock<Mutex<Vec<WindowEvent>>> = OnceLock::new();

fn push_event(event: WindowEvent) {
    if let Some(q) = EVENT_QUEUE.get() {
        q.lock().unwrap().push(event);
    }
}

type WlDisplay = *mut c_void;
type WlRegistry = *mut c_void;
type WlCompositor = *mut c_void;
type WlSurface = *mut c_void;
type WlShm = *mut c_void;
type WlBuffer = *mut c_void;

type XdgWmBase = *mut c_void;
type XdgSurface = *mut c_void;
type XdgToplevel = *mut c_void;



pub struct Window {
    display: WlDisplay,
    surface: WlSurface,
    shm: WlShm,
    buffer: WlBuffer,
    buffer_data: *mut u8,

    xdg_surface: XdgSurface,
    xdg_toplevel: XdgToplevel,

    size: (u32, u32),
    handlers: events::Handlers,
}

impl Window {
    pub fn new(size: (u32, u32), title: &str) -> Self {
        EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::new()));

        unsafe {
            let display = wl_display_connect(null_mut());
            let registry = wl_display_get_registry(display);

            // -------------------------
            // Globals we fill via registry
            // -------------------------
            static mut COMPOSITOR: WlCompositor = null_mut();
            static mut SHM: WlShm = null_mut();
            static mut WM_BASE: XdgWmBase = null_mut();

            extern "C" fn registry_handler(
                _data: *mut c_void,
                registry: WlRegistry,
                name: u32,
                interface: *const i8,
                _version: u32,
            ) {
                unsafe {
                    let iface = std::ffi::CStr::from_ptr(interface).to_str().unwrap();

                    if iface == "wl_compositor" {
                        COMPOSITOR =
                            wl_registry_bind(registry, name, &wl_compositor_interface, 4);
                    } else if iface == "wl_shm" {
                        SHM = wl_registry_bind(registry, name, &wl_shm_interface, 1);
                    } else if iface == "xdg_wm_base" {
                        WM_BASE =
                            wl_registry_bind(registry, name, &xdg_wm_base_interface, 1);
                    }
                }
            }

            let registry_listener = wl_registry_listener {
                global: registry_handler,
                global_remove: None,
            };

            wl_registry_add_listener(registry, &registry_listener, null_mut());

            // BLOCK until globals are ready
            wl_display_roundtrip(display);

            // -------------------------
            // xdg_wm_base listener (REQUIRED)
            // -------------------------
            extern "C" fn wm_base_ping(
                _data: *mut c_void,
                wm_base: XdgWmBase,
                serial: u32,
            ) {
                unsafe {
                    xdg_wm_base_pong(wm_base, serial);
                }
            }

            let wm_listener = xdg_wm_base_listener {
                ping: wm_base_ping,
            };

            xdg_wm_base_add_listener(WM_BASE, &wm_listener, null_mut());

            // -------------------------
            // Create surface
            // -------------------------
            let surface = wl_compositor_create_surface(COMPOSITOR);

            // -------------------------
            // xdg_surface
            // -------------------------
            let xdg_surface = xdg_wm_base_get_xdg_surface(WM_BASE, surface);

            extern "C" fn surface_configure(
                _data: *mut c_void,
                surface: XdgSurface,
                serial: u32,
            ) {
                unsafe {
                    xdg_surface_ack_configure(surface, serial);
                }
            }

            let surface_listener = xdg_surface_listener {
                configure: surface_configure,
            };

            xdg_surface_add_listener(xdg_surface, &surface_listener, null_mut());

            // -------------------------
            // xdg_toplevel
            // -------------------------
            let xdg_toplevel = xdg_surface_get_toplevel(xdg_surface);

            extern "C" fn toplevel_configure(
                _data: *mut c_void,
                _toplevel: XdgToplevel,
                width: i32,
                height: i32,
                _states: *mut c_void,
            ) {
                if width > 0 && height > 0 {
                    push_event(WindowEvent::Resized(
                        width as u32,
                        height as u32,
                    ));
                }
            }

            extern "C" fn toplevel_close(
                _data: *mut c_void,
                _toplevel: XdgToplevel,
            ) {
                push_event(WindowEvent::Close);
            }

            let toplevel_listener = xdg_toplevel_listener {
                configure: toplevel_configure,
                close: toplevel_close,
            };

            xdg_toplevel_add_listener(xdg_toplevel, &toplevel_listener, null_mut());

            // -------------------------
            // Title
            // -------------------------
            let c_title = CString::new(title).unwrap();
            xdg_toplevel_set_title(xdg_toplevel, c_title.as_ptr());

            // -------------------------
            // Initial commit (REQUIRED)
            // -------------------------
            wl_surface_commit(surface);

            // Wait for initial configure
            wl_display_roundtrip(display);

            // -------------------------
            // Create SHM buffer
            // -------------------------
            let (buffer, buffer_data) = create_shm_buffer(SHM, size);

            Self {
                display,
                surface,
                shm,
                buffer,
                xdg_surface,
                xdg_toplevel,
                size,
                handlers: events::Handlers::default(),
            }
        }
    }

    pub fn pump_events(&self) {
        unsafe {
            wl_display_dispatch(self.display);
        }
    }

    pub fn present(&mut self, fb: &Framebuffer) {
        unsafe {
            // copy framebuffer into shm memory
            std::ptr::copy_nonoverlapping(
                fb.as_ptr(),
                self.buffer_data,
                (self.size.0 * self.size.1 * 4) as usize,
            );

            wl_surface_attach(self.surface, self.buffer, 0, 0);
            wl_surface_commit(self.surface);
        }
    }

    pub fn set_title(&self, title: &str) {
        let c = CString::new(title).unwrap();
        unsafe {
            xdg_toplevel_set_title(self.xdg_toplevel, c.as_ptr());
        }
    }

    pub fn handle(&self) -> *mut c_void {
        self.surface
    }
}

unsafe fn create_shm_buffer(
    shm: *mut c_void,
    size: (u32, u32),
) -> (*mut c_void, *mut u8) {
    let stride = size.0 * 4;
    let total = stride * size.1;

    let fd = libc::memfd_create(std::ptr::null(), 0);
    libc::ftruncate(fd, total as i64);

    let data = libc::mmap(
        std::ptr::null_mut(),
        total as usize,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED,
        fd,
        0,
    ) as *mut u8;

    let pool = wl_shm_create_pool(shm, fd, total as i32);

    let buffer = wl_shm_pool_create_buffer(
        pool,
        0,
        size.0 as i32,
        size.1 as i32,
        stride as i32,
        0, // WL_SHM_FORMAT_ARGB8888
    );

    (buffer, data)
}

extern "C" fn toplevel_configure(
    data: *mut c_void,
    _toplevel: *mut c_void,
    width: i32,
    height: i32,
    _states: *mut c_void,
) {
    if width > 0 && height > 0 {
        push_event(WindowEvent::Resized(width as u32, height as u32));
    }
}

extern "C" fn toplevel_close(data: *mut c_void, _toplevel: *mut c_void) {
    push_event(WindowEvent::Close);
}

#[link(name = "wayland-client")]
unsafe extern "C" {
    fn wl_display_connect(name: *mut i8) -> WlDisplay;
    fn wl_display_dispatch_pending(display: WlDisplay) -> i32;
    fn wl_display_flush(display: WlDisplay) -> i32;

    fn wl_display_get_registry(display: WlDisplay) -> WlRegistry;

    fn wl_compositor_create_surface(compositor: WlCompositor) -> WlSurface;

    fn wl_surface_attach(surface: WlSurface, buffer: WlBuffer, x: i32, y: i32);
    fn wl_surface_commit(surface: WlSurface);
}

#[link(name = "wayland-client")]
unsafe extern "C" {
    // fn wl_display_connect(name: *const i8) -> *mut c_void;
    fn wl_display_dispatch(display: *mut c_void) -> i32;
    fn wl_display_roundtrip(display: *mut c_void) -> i32;

    // fn wl_display_get_registry(display: *mut c_void) -> *mut c_void;

    fn wl_registry_add_listener(
        registry: *mut c_void,
        listener: *const wl_registry_listener,
        data: *mut c_void,
    ) -> i32;

    fn wl_registry_bind(
        registry: *mut c_void,
        name: u32,
        interface: *const wl_interface,
        version: u32,
    ) -> *mut c_void;

    // fn wl_compositor_create_surface(compositor: *mut c_void) -> *mut c_void;

    // fn wl_surface_attach(surface: *mut c_void, buffer: *mut c_void, x: i32, y: i32);
    // fn wl_surface_commit(surface: *mut c_void);
}

#[link(name = "wayland-client")]
unsafe extern "C" {
    fn wl_shm_create_pool(
        shm: *mut c_void,
        fd: i32,
        size: i32,
    ) -> *mut c_void;

    fn wl_shm_pool_create_buffer(
        pool: *mut c_void,
        offset: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: u32,
    ) -> *mut c_void;
}

#[link(name = "wayland-client")]
unsafe extern "C" {
    fn xdg_wm_base_add_listener(
        wm_base: *mut c_void,
        listener: *const xdg_wm_base_listener,
        data: *mut c_void,
    ) -> i32;

    fn xdg_wm_base_get_xdg_surface(
        wm_base: *mut c_void,
        surface: *mut c_void,
    ) -> *mut c_void;

    fn xdg_wm_base_pong(wm_base: *mut c_void, serial: u32);

    fn xdg_surface_add_listener(
        surface: *mut c_void,
        listener: *const xdg_surface_listener,
        data: *mut c_void,
    ) -> i32;

    fn xdg_surface_get_toplevel(surface: *mut c_void) -> *mut c_void;

    fn xdg_surface_ack_configure(surface: *mut c_void, serial: u32);

    fn xdg_toplevel_add_listener(
        toplevel: *mut c_void,
        listener: *const xdg_toplevel_listener,
        data: *mut c_void,
    ) -> i32;

    fn xdg_toplevel_set_title(toplevel: *mut c_void, title: *const i8);
}

#[repr(C)]
pub struct wl_registry_listener {
    pub global: extern "C" fn(
        data: *mut c_void,
        registry: *mut c_void,
        name: u32,
        interface: *const i8,
        version: u32,
    ),
    pub global_remove: Option<extern "C" fn(*mut c_void, *mut c_void, u32)>,
}

#[repr(C)]
pub struct wl_interface {
    _private: [u8; 0],
}

unsafe extern "C" {
    static wl_compositor_interface: wl_interface;
    static wl_shm_interface: wl_interface;
    static xdg_wm_base_interface: wl_interface;
}

#[repr(C)]
pub struct xdg_wm_base_listener {
    pub ping: extern "C" fn(*mut c_void, *mut c_void, u32),
}

#[repr(C)]
pub struct xdg_surface_listener {
    pub configure: extern "C" fn(*mut c_void, *mut c_void, u32),
}

#[repr(C)]
pub struct xdg_toplevel_listener {
    pub configure: extern "C" fn(*mut c_void, *mut c_void, i32, i32, *mut c_void),
    pub close: extern "C" fn(*mut c_void, *mut c_void),
}