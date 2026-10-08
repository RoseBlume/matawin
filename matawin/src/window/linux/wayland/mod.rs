//! Implements the `wayland` module.
//!
//! This module documents both public API items and internal implementation details.

use std::{
    cell::{ Cell, RefCell },
    collections::HashSet,
    ffi::c_void,
    sync::{ atomic::{ AtomicBool, Ordering }, Arc, Mutex },
};

use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer,
        wl_compositor,
        wl_keyboard,
        wl_pointer,
        wl_registry,
        wl_seat,
        wl_shm,
        wl_shm_pool,
        wl_surface,
    },
    Connection,
    Dispatch,
    EventQueue,
    Proxy,
    QueueHandle,
    WEnum,
};
use wayland_protocols::xdg::shell::client::{ xdg_surface, xdg_toplevel, xdg_wm_base };

/// Contains the drop implementation.
///
/// Visibility: **private**.
mod drop;
/// Contains the events implementation.
///
/// Visibility: **private**.
mod events;
/// Contains the renderer implementation.
///
/// Visibility: **private**.
mod renderer;
/// Contains the show implementation.
///
/// Visibility: **private**.
mod show;

use crate::buffer::Framebuffer;
use crate::events::{ Key, WindowEvent, EVENT_QUEUE, KEYS_DOWN };
use renderer::WaylandRenderer;

/// State owned by the Wayland event queue.
///
/// The low-level Wayland API dispatches protocol events through this value. It
/// intentionally contains only protocol objects and small pieces of window state;
/// the large shared-memory frame buffers live in [`WaylandRenderer`].
pub(super) struct WaylandState {
    /// Stores compositor.
    ///
    /// Visibility: **private**.
    compositor: Option<wl_compositor::WlCompositor>,
    /// Stores shm.
    ///
    /// Visibility: **private**.
    shm: Option<wl_shm::WlShm>,
    /// Stores wm base.
    ///
    /// Visibility: **private**.
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    /// Stores seat.
    ///
    /// Visibility: **private**.
    seat: Option<wl_seat::WlSeat>,
    /// Stores keyboard.
    ///
    /// Visibility: **private**.
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Stores pointer.
    ///
    /// Visibility: **private**.
    pointer: Option<wl_pointer::WlPointer>,

    /// Stores configured.
    ///
    /// Visibility: **private**.
    configured: bool,
    /// Stores client size.
    ///
    /// Visibility: **private**.
    client_size: (u32, u32),
    /// Stores maximized.
    ///
    /// Visibility: **private**.
    maximized: bool,
}

impl WaylandState {
    /// Creates a new instance.
    ///
    /// Visibility: **private**.
    #[doc = "Creates a new value. Visibility: private/internal."]
    fn new(size: (u32, u32)) -> Self {
        Self {
            compositor: None,
            shm: None,
            wm_base: None,
            seat: None,
            keyboard: None,
            pointer: None,
            configured: false,
            client_size: size,
            maximized: false,
        }
    }
}

/// Pushes event.
///
/// Visibility: **private**.
#[doc = "Pushes event. Visibility: private/internal."]
fn push_event(event: WindowEvent) {
    if let Some(queue) = EVENT_QUEUE.get() {
        queue.lock().unwrap().push(event);
    }
}

/// Sets key down.
///
/// Visibility: **private**.
#[doc = "Sets key down. Visibility: private/internal."]
fn set_key_down(key: Key, down: bool) {
    if let Some(keys) = KEYS_DOWN.get() {
        let mut keys = keys.lock().unwrap();
        if down {
            keys.insert(key);
        } else {
            keys.remove(&key);
        }
    }
}

/// Native Wayland window backend.
///
/// This backend talks to the user's existing Wayland compositor through the
/// session socket. It never opens `/dev/input`, DRM devices, or other privileged
/// device nodes, so normal window creation and input handling do not require
/// root privileges or `sudo`.
pub struct PlatformWindow {
    /// Stores connection.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) connection: Connection,
    /// Stores event queue.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) event_queue: RefCell<EventQueue<WaylandState>>,
    /// Stores state.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) state: RefCell<WaylandState>,

    /// Stores surface.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) surface: wl_surface::WlSurface,
    /// Stores xdg surface.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) xdg_surface: xdg_surface::XdgSurface,
    /// Stores toplevel.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) toplevel: xdg_toplevel::XdgToplevel,

    /// Size of the most recently presented software framebuffer. Applications
    /// may redraw at the compositor-configured size to avoid post-raster scaling.
    pub(super) size: Cell<(u32, u32)>,

    /// Stores renderer.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) renderer: WaylandRenderer,
    /// Stores hidden.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) hidden: Cell<bool>,
    /// Stores resizable.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) resizable: Cell<bool>,
}

impl PlatformWindow {
    /// Creates a Wayland window, panicking if Wayland cannot be initialized.
    ///
    /// Linux runtime backend selection should normally call [`Self::try_new`] so
    /// an unavailable Wayland session can fall back to X11 instead of aborting.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        Self::try_new(title, size).unwrap_or_else(|error|
            panic!("Failed to create Wayland window: {error}")
        )
    }

    /// Attempts to create a Wayland window without panicking on setup failure.
    ///
    /// Connection, registry discovery, required-global validation, and the initial
    /// XDG configure roundtrip are all part of this attempt. This allows the Linux
    /// selector to cleanly fall back to X11 when Wayland is not usable.
    #[doc = "Implements the try new operation. Visibility: public."]
    pub fn try_new(title: &str, size: (u32, u32)) -> Result<Self, String> {
        if size.0 == 0 || size.1 == 0 {
            return Err("Wayland windows must have a non-zero size".to_owned());
        }

        KEYS_DOWN.get_or_init(|| Mutex::new(HashSet::with_capacity(16)));
        EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::with_capacity(32)));

        let connection = Connection::connect_to_env().map_err(|error|
            format!("failed to connect to the Wayland compositor: {error}")
        )?;
        let mut event_queue = connection.new_event_queue::<WaylandState>();
        let qh = event_queue.handle();

        let display = connection.display();
        display.get_registry(&qh, ());

        let mut state = WaylandState::new(size);

        // First roundtrip discovers globals. The second receives immediate events
        // generated by objects bound while the registry events were dispatched,
        // notably wl_seat capabilities.
        event_queue
            .roundtrip(&mut state)
            .map_err(|error| format!("registry roundtrip failed: {error}"))?;
        event_queue
            .roundtrip(&mut state)
            .map_err(|error| format!("global initialization roundtrip failed: {error}"))?;

        let compositor = state.compositor
            .as_ref()
            .ok_or_else(|| "compositor does not expose wl_compositor".to_owned())?
            .clone();
        let shm = state.shm
            .as_ref()
            .ok_or_else(|| "compositor does not expose wl_shm".to_owned())?
            .clone();
        let wm_base = state.wm_base
            .as_ref()
            .ok_or_else(|| "compositor does not expose xdg_wm_base".to_owned())?
            .clone();

        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let toplevel = xdg_surface.get_toplevel(&qh, ());

        toplevel.set_title(title.to_owned());
        toplevel.set_app_id("matawin".to_owned());

        // XDG shell windows must perform an empty initial commit. The compositor
        // then replies with the first configure event before pixels are attached.
        surface.commit();
        event_queue
            .roundtrip(&mut state)
            .map_err(|error| format!("initial configure roundtrip failed: {error}"))?;

        if !state.configured {
            return Err("compositor did not configure the XDG toplevel".to_owned());
        }

        let target_size = state.client_size;
        let renderer = WaylandRenderer::new(shm, qh, size, target_size);

        Ok(Self {
            connection,
            event_queue: RefCell::new(event_queue),
            state: RefCell::new(state),
            surface,
            xdg_surface,
            toplevel,
            size: Cell::new(size),
            renderer,
            hidden: Cell::new(false),
            resizable: Cell::new(true),
        })
    }

    /// Performs the present operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the present operation. Visibility: public."]
    pub fn present(&mut self, fb: &Framebuffer) {
        // Event pumping is intentionally separate from presentation. Call
        // Window::poll_events[_into] once per frame; avoiding a second pump here
        // removes duplicate protocol work from the render hot path.
        if self.hidden.get() {
            return;
        }

        let state = self.state.borrow();
        if !state.configured {
            return;
        }

        let target_size = state.client_size;
        drop(state);

        if target_size.0 == 0 || target_size.1 == 0 {
            return;
        }

        let source_size = (fb.width, fb.height);
        self.size.set(source_size);
        self.renderer.resize_framebuffer(source_size);
        self.renderer.resize_target(target_size);
        self.renderer.present(&self.surface, fb.as_ptr() as *const u32);

        let _ = self.connection.flush();
    }

    /// Sets title.
    ///
    /// Visibility: **public**.
    #[doc = "Sets title. Visibility: public."]
    pub fn set_title(&self, title: &str) {
        self.toplevel.set_title(title.to_owned());
        let _ = self.connection.flush();
    }

    /// Wayland's pure-Rust backend does not expose a stable native pointer like
    /// Win32's HWND. This method is retained for source compatibility and returns
    /// null. Use [`Self::surface`] or [`Self::surface_id`] for Wayland-specific code.
    #[doc = "Implements the handle operation. Visibility: public."]
    pub fn handle(&self) -> *mut c_void {
        std::ptr::null_mut()
    }

    /// Returns the underlying Wayland surface proxy.
    #[doc = "Implements the surface operation. Visibility: public."]
    pub fn surface(&self) -> &wl_surface::WlSurface {
        &self.surface
    }

    /// Returns the protocol object ID of the underlying `wl_surface`.
    ///
    /// Protocol IDs are connection-local and can be reused after destruction, so
    /// they should not be treated as process-wide unique handles.
    #[doc = "Implements the surface id operation. Visibility: public."]
    pub fn surface_id(&self) -> u32 {
        self.surface.id().protocol_id()
    }

    /// Returns the logical software framebuffer dimensions.
    #[doc = "Implements the framebuffer size operation. Visibility: public."]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        self.size.get()
    }

    /// Returns the most recently configured surface size.
    #[doc = "Implements the client size operation. Visibility: public."]
    pub fn client_size(&self) -> (u32, u32) {
        self.state.borrow().client_size
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>
    ) {
        let wl_registry::Event::Global { name, interface, version } = event else {
            return;
        };

        match interface.as_str() {
            "wl_compositor" if state.compositor.is_none() => {
                state.compositor = Some(
                    registry.bind::<wl_compositor::WlCompositor, _, _>(name, version.min(6), qh, ())
                );
            }
            "wl_shm" if state.shm.is_none() => {
                state.shm = Some(
                    registry.bind::<wl_shm::WlShm, _, _>(name, version.min(2), qh, ())
                );
            }
            "xdg_wm_base" if state.wm_base.is_none() => {
                state.wm_base = Some(
                    registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, version.min(6), qh, ())
                );
            }
            "wl_seat" if state.seat.is_none() => {
                state.seat = Some(
                    registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(9), qh, ())
                );
            }
            _ => {}
        }
    }
}

delegate_noop!(WaylandState: ignore wl_compositor::WlCompositor);
delegate_noop!(WaylandState: ignore wl_surface::WlSurface);
delegate_noop!(WaylandState: ignore wl_shm::WlShm);
delegate_noop!(WaylandState: ignore wl_shm_pool::WlShmPool);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, states } => {
                let width = if width > 0 { width as u32 } else { state.client_size.0 };
                let height = if height > 0 { height as u32 } else { state.client_size.1 };

                let maximized = states
                    .chunks_exact(4)
                    .map(|bytes| u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    .any(|raw| raw == (xdg_toplevel::State::Maximized as u32));

                state.client_size = (width, height);

                if maximized {
                    state.maximized = true;
                    push_event(WindowEvent::Maximized(width, height));
                } else {
                    state.maximized = false;
                    push_event(WindowEvent::Resized(width, height));
                }
            }
            xdg_toplevel::Event::Close => {
                push_event(WindowEvent::Close);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(capabilities) } = event {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                if state.keyboard.is_none() {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            } else if let Some(keyboard) = state.keyboard.take() {
                if keyboard.version() >= 3 {
                    keyboard.release();
                }
            }

            if capabilities.contains(wl_seat::Capability::Pointer) {
                if state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
            } else if let Some(pointer) = state.pointer.take() {
                if pointer.version() >= 3 {
                    pointer.release();
                }
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        _: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>
    ) {
        match event {
            wl_keyboard::Event::Key { key, state: WEnum::Value(key_state), .. } => {
                let Some(key) = map_evdev_key(key) else {
                    return;
                };

                match key_state {
                    wl_keyboard::KeyState::Pressed => {
                        push_event(WindowEvent::KeyPressed(key));
                        set_key_down(key, true);
                    }
                    wl_keyboard::KeyState::Released => {
                        push_event(WindowEvent::KeyReleased(key));
                        set_key_down(key, false);
                    }
                    _ => {}
                }
            }
            wl_keyboard::Event::Leave { .. } => {
                // A focus change conceptually releases all keys for this window.
                if let Some(keys) = KEYS_DOWN.get() {
                    keys.lock().unwrap().clear();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        _: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>
    ) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                push_event(
                    WindowEvent::CursorMoved(surface_x.round() as i32, surface_y.round() as i32)
                );
            }
            wl_pointer::Event::Axis { axis: WEnum::Value(axis), value, .. } => {
                let delta = (-value * 12.0).round() as i32;
                match axis {
                    wl_pointer::Axis::VerticalScroll => push_event(WindowEvent::Scroll(0, delta)),
                    wl_pointer::Axis::HorizontalScroll => push_event(WindowEvent::Scroll(delta, 0)),
                    _ => {}
                }
            }
            wl_pointer::Event::Button { button, state: WEnum::Value(button_state), .. } => {
                let key = match button {
                    0x110 => Some(Key::MouseLeft),
                    0x111 => Some(Key::MouseRight),
                    0x112 => Some(Key::MouseMiddle),
                    _ => None,
                };
                let Some(key) = key else {
                    return;
                };
                match button_state {
                    wl_pointer::ButtonState::Pressed => {
                        push_event(WindowEvent::KeyPressed(key));
                        set_key_down(key, true);
                    }
                    wl_pointer::ButtonState::Released => {
                        push_event(WindowEvent::KeyReleased(key));
                        set_key_down(key, false);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, Arc<AtomicBool>> for WaylandState {
    /// Performs the event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the event operation. Visibility: private/internal."]
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        busy: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>
    ) {
        if let wl_buffer::Event::Release = event {
            busy.store(false, Ordering::Release);
        }
    }
}

/// Implements map evdev key.
///
/// Visibility: **private**.
#[doc = "Implements the map evdev key operation. Visibility: private/internal."]
fn map_evdev_key(code: u32) -> Option<Key> {
    match code {
        1 => Some(Key::Escape),
        14 => Some(Key::Backspace),
        15 => Some(Key::Tab),
        28 => Some(Key::Enter),
        29 => Some(Key::LeftCtrl),
        42 => Some(Key::LeftShift),
        54 => Some(Key::RightShift),
        57 => Some(Key::Space),
        97 => Some(Key::RightCtrl),

        103 => Some(Key::Up),
        105 => Some(Key::Left),
        106 => Some(Key::Right),
        108 => Some(Key::Down),

        2..=10 => Some(Key::Num((code - 1) as u8)),
        11 => Some(Key::Num(0)),

        59..=68 => Some(Key::F((code - 58) as u8)),
        87 => Some(Key::F(11)),
        88 => Some(Key::F(12)),

        16 => Some(Key::Char('q')),
        17 => Some(Key::Char('w')),
        18 => Some(Key::Char('e')),
        19 => Some(Key::Char('r')),
        20 => Some(Key::Char('t')),
        21 => Some(Key::Char('y')),
        22 => Some(Key::Char('u')),
        23 => Some(Key::Char('i')),
        24 => Some(Key::Char('o')),
        25 => Some(Key::Char('p')),
        30 => Some(Key::Char('a')),
        31 => Some(Key::Char('s')),
        32 => Some(Key::Char('d')),
        33 => Some(Key::Char('f')),
        34 => Some(Key::Char('g')),
        35 => Some(Key::Char('h')),
        36 => Some(Key::Char('j')),
        37 => Some(Key::Char('k')),
        38 => Some(Key::Char('l')),
        44 => Some(Key::Char('z')),
        45 => Some(Key::Char('x')),
        46 => Some(Key::Char('c')),
        47 => Some(Key::Char('v')),
        48 => Some(Key::Char('b')),
        49 => Some(Key::Char('n')),
        50 => Some(Key::Char('m')),

        12 => Some(Key::Char('-')),
        13 => Some(Key::Char('=')),
        26 => Some(Key::Char('[')),
        27 => Some(Key::Char(']')),
        39 => Some(Key::Char(';')),
        40 => Some(Key::Char('\'')),
        41 => Some(Key::Char('`')),
        43 => Some(Key::Char('\\')),
        51 => Some(Key::Char(',')),
        52 => Some(Key::Char('.')),
        53 => Some(Key::Char('/')),

        _ => None,
    }
}
