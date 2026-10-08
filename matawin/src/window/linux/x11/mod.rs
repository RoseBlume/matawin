//! Implements the `x11` module.
//!
//! This module documents both public API items and internal implementation details.

use std::{ cell::{ Cell, RefCell }, collections::HashSet, ffi::c_void, sync::Mutex };

use x11rb::{
    connection::Connection,
    protocol::xproto::{
        Atom,
        AtomEnum,
        ChangeWindowAttributesAux,
        CreateWindowAux,
        EventMask,
        PropMode,
        Window,
        WindowClass,
        ConnectionExt as _,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
    COPY_DEPTH_FROM_PARENT,
    COPY_FROM_PARENT,
};

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
use renderer::X11Renderer;

#[derive(Clone, Copy)]
/// Stores the state and data for atoms.
///
/// Visibility: **parent-module-visible**.
pub(super) struct Atoms {
    /// Stores wm protocols.
    ///
    /// Visibility: **public**.
    pub wm_protocols: Atom,
    /// Stores wm delete window.
    ///
    /// Visibility: **public**.
    pub wm_delete_window: Atom,
    /// Stores wm change state.
    ///
    /// Visibility: **public**.
    pub wm_change_state: Atom,
    /// Stores net wm name.
    ///
    /// Visibility: **public**.
    pub net_wm_name: Atom,
    /// Stores utf8 string.
    ///
    /// Visibility: **public**.
    pub utf8_string: Atom,
    /// Stores net wm state.
    ///
    /// Visibility: **public**.
    pub net_wm_state: Atom,
    /// Stores net wm state maximized horz.
    ///
    /// Visibility: **public**.
    pub net_wm_state_maximized_horz: Atom,
    /// Stores net wm state maximized vert.
    ///
    /// Visibility: **public**.
    pub net_wm_state_maximized_vert: Atom,
    /// Stores motif wm hints.
    ///
    /// Visibility: **public**.
    pub motif_wm_hints: Atom,
}

/// Implements intern.
///
/// Visibility: **private**.
#[doc = "Implements the intern operation. Visibility: private/internal."]
fn intern(conn: &RustConnection, name: &str) -> Result<Atom, String> {
    let cookie = conn
        .intern_atom(false, name.as_bytes())
        .map_err(|error| format!("failed to request X11 atom {name}: {error}"))?;
    let reply = cookie.reply().map_err(|error| format!("failed to read X11 atom {name}: {error}"))?;
    Ok(reply.atom)
}

/// Loads atoms.
///
/// Visibility: **private**.
#[doc = "Loads atoms. Visibility: private/internal."]
fn load_atoms(conn: &RustConnection) -> Result<Atoms, String> {
    Ok(Atoms {
        wm_protocols: intern(conn, "WM_PROTOCOLS")?,
        wm_delete_window: intern(conn, "WM_DELETE_WINDOW")?,
        wm_change_state: intern(conn, "WM_CHANGE_STATE")?,
        net_wm_name: intern(conn, "_NET_WM_NAME")?,
        utf8_string: intern(conn, "UTF8_STRING")?,
        net_wm_state: intern(conn, "_NET_WM_STATE")?,
        net_wm_state_maximized_horz: intern(conn, "_NET_WM_STATE_MAXIMIZED_HORZ")?,
        net_wm_state_maximized_vert: intern(conn, "_NET_WM_STATE_MAXIMIZED_VERT")?,
        motif_wm_hints: intern(conn, "_MOTIF_WM_HINTS")?,
    })
}

/// Pushes event.
///
/// Visibility: **parent-module-visible**.
#[doc = "Pushes event. Visibility: parent-module-visible."]
pub(super) fn push_event(event: WindowEvent) {
    if let Some(queue) = EVENT_QUEUE.get() {
        queue.lock().unwrap().push(event);
    }
}

/// Sets key down.
///
/// Visibility: **parent-module-visible**.
#[doc = "Sets key down. Visibility: parent-module-visible."]
pub(super) fn set_key_down(key: Key, down: bool) -> bool {
    if let Some(keys) = KEYS_DOWN.get() {
        let mut keys = keys.lock().unwrap();
        if down {
            return keys.insert(key);
        }
        return keys.remove(&key);
    }
    false
}

/// Native X11 backend implemented with x11rb's pure-Rust connection.
///
/// It connects to the current user's X server through `DISPLAY`. No root access,
/// `/dev/input` access, Xlib FFI, or `sudo` is required.
pub struct PlatformWindow {
    /// Stores connection.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) connection: RustConnection,
    /// Stores screen num.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) screen_num: usize,
    /// Stores root.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) root: Window,
    /// Stores window.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) window: Window,
    /// Stores atoms.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) atoms: Atoms,
    /// Stores renderer.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) renderer: X11Renderer,

    /// Stores size.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) size: Cell<(u32, u32)>,
    /// Stores client size.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) client_size: Cell<(u32, u32)>,
    /// Stores position.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) position: Cell<(i32, i32)>,
    /// Stores hidden.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) hidden: Cell<bool>,
    /// Stores maximized.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) maximized: Cell<bool>,
    /// Stores decorations.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) decorations: Cell<bool>,
    /// Stores maximizable.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) maximizable: Cell<bool>,
    /// Stores minimizable.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) minimizable: Cell<bool>,
    /// Stores resizable.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) resizable: Cell<bool>,
    /// Stores keymap.
    ///
    /// Visibility: **parent-module-visible**.
    pub(super) keymap: RefCell<[Option<Key>; 256]>,
}

impl PlatformWindow {
    /// Creates an X11 window, panicking if X11 cannot be initialized.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        Self::try_new(title, size).unwrap_or_else(|error|
            panic!("Failed to create X11 window: {error}")
        )
    }

    /// Attempts to create an X11 window without panicking on setup failure.
    ///
    /// This is used by the Linux runtime selector after a Wayland attempt fails.
    #[doc = "Implements the try new operation. Visibility: public."]
    pub fn try_new(title: &str, size: (u32, u32)) -> Result<Self, String> {
        if size.0 == 0 || size.1 == 0 {
            return Err("X11 windows must have a non-zero size".to_owned());
        }
        if size.0 > (u16::MAX as u32) || size.1 > (u16::MAX as u32) {
            return Err("X11 window dimensions must fit in u16".to_owned());
        }

        KEYS_DOWN.get_or_init(|| Mutex::new(HashSet::with_capacity(16)));
        EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::with_capacity(32)));

        let (connection, screen_num) = x11rb
            ::connect(None)
            .map_err(|error| format!("failed to connect to the X11 server: {error}"))?;
        let screen = connection
            .setup()
            .roots.get(screen_num)
            .ok_or_else(|| format!("X11 screen index {screen_num} does not exist"))?;
        let root = screen.root;
        let black_pixel = screen.black_pixel;
        let window = connection
            .generate_id()
            .map_err(|error| format!("failed to allocate X11 window ID: {error}"))?;

        let event_mask =
            EventMask::EXPOSURE |
            EventMask::STRUCTURE_NOTIFY |
            EventMask::KEY_PRESS |
            EventMask::KEY_RELEASE |
            EventMask::BUTTON_PRESS |
            EventMask::BUTTON_RELEASE |
            EventMask::POINTER_MOTION |
            EventMask::FOCUS_CHANGE |
            EventMask::PROPERTY_CHANGE;

        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                size.0 as u16,
                size.1 as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                COPY_FROM_PARENT,
                &CreateWindowAux::new().background_pixel(black_pixel).event_mask(event_mask)
            )
            .map_err(|error| format!("failed to create X11 window: {error}"))?;

        let atoms = load_atoms(&connection)?;
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                atoms.wm_protocols,
                AtomEnum::ATOM,
                &[atoms.wm_delete_window]
            )
            .map_err(|error| format!("failed to install WM_DELETE_WINDOW protocol: {error}"))?;

        set_title_raw(&connection, window, atoms, title);
        connection
            .map_window(window)
            .map_err(|error| format!("failed to map X11 window: {error}"))?;
        connection.flush().map_err(|error| format!("failed to flush X11 setup requests: {error}"))?;

        let keymap = load_keymap(&connection);
        let renderer = X11Renderer::new(&connection, window, screen_num, size, size);

        Ok(Self {
            connection,
            screen_num,
            root,
            window,
            atoms,
            renderer,
            size: Cell::new(size),
            client_size: Cell::new(size),
            position: Cell::new((0, 0)),
            hidden: Cell::new(false),
            maximized: Cell::new(false),
            decorations: Cell::new(true),
            maximizable: Cell::new(true),
            minimizable: Cell::new(true),
            resizable: Cell::new(true),
            keymap: RefCell::new(keymap),
        })
    }

    /// Performs the present operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the present operation. Visibility: public."]
    pub fn present(&mut self, fb: &Framebuffer) {
        if self.hidden.get() {
            return;
        }
        let target = self.client_size.get();
        if target.0 == 0 || target.1 == 0 {
            return;
        }
        let source = (fb.width, fb.height);
        self.size.set(source);
        self.renderer.resize_framebuffer(source);
        self.renderer.resize_target(target);
        self.renderer.present(&self.connection, self.window, fb.as_ptr());
    }

    /// Sets title.
    ///
    /// Visibility: **public**.
    #[doc = "Sets title. Visibility: public."]
    pub fn set_title(&self, title: &str) {
        set_title_raw(&self.connection, self.window, self.atoms, title);
        let _ = self.connection.flush();
    }

    /// Returns the X11 Window XID encoded as a pointer-sized value for API parity.
    #[doc = "Implements the handle operation. Visibility: public."]
    pub fn handle(&self) -> *mut c_void {
        self.window as usize as *mut c_void
    }

    /// Performs the window operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the window operation. Visibility: public."]
    pub fn window(&self) -> Window {
        self.window
    }
    /// Performs the framebuffer size operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the framebuffer size operation. Visibility: public."]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        self.size.get()
    }
    /// Computes client size.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the client size operation. Visibility: public."]
    pub fn client_size(&self) -> (u32, u32) {
        self.client_size.get()
    }

    /// Performs the reload keymap operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the reload keymap operation. Visibility: parent-module-visible."]
    pub(super) fn reload_keymap(&self) {
        *self.keymap.borrow_mut() = load_keymap(&self.connection);
    }
}

/// Sets title raw.
///
/// Visibility: **private**.
#[doc = "Sets title raw. Visibility: private/internal."]
fn set_title_raw(conn: &RustConnection, window: Window, atoms: Atoms, title: &str) {
    let _ = conn.change_property8(
        PropMode::REPLACE,
        window,
        atoms.net_wm_name,
        atoms.utf8_string,
        title.as_bytes()
    );
    let _ = conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        title.as_bytes()
    );
}

/// Loads keymap.
///
/// Visibility: **private**.
#[doc = "Loads keymap. Visibility: private/internal."]
fn load_keymap(conn: &RustConnection) -> [Option<Key>; 256] {
    let setup = conn.setup();
    let first = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(first).saturating_add(1);
    let Ok(cookie) = conn.get_keyboard_mapping(first, count) else {
        return [None; 256];
    };
    let Ok(reply) = cookie.reply() else {
        return [None; 256];
    };
    let per = reply.keysyms_per_keycode as usize;
    if per == 0 {
        return [None; 256];
    }

    let mut result = [None; 256];
    for i in 0..count as usize {
        let start = i * per;
        let end = (start + per).min(reply.keysyms.len());
        let mut mapped = None;
        for &sym in &reply.keysyms[start..end] {
            if let Some(key) = map_keysym(sym) {
                mapped = Some(key);
                break;
            }
        }
        if let Some(key) = mapped {
            result[first.wrapping_add(i as u8) as usize] = Some(key);
        }
    }
    result
}

/// Implements map keysym.
///
/// Visibility: **private**.
#[doc = "Implements the map keysym operation. Visibility: private/internal."]
fn map_keysym(sym: u32) -> Option<Key> {
    match sym {
        0xff08 => Some(Key::Backspace),
        0xff09 => Some(Key::Tab),
        0xff0d => Some(Key::Enter),
        0xff1b => Some(Key::Escape),
        0x0020 => Some(Key::Space),
        0xff51 => Some(Key::Left),
        0xff52 => Some(Key::Up),
        0xff53 => Some(Key::Right),
        0xff54 => Some(Key::Down),
        0xffe1 => Some(Key::LeftShift),
        0xffe2 => Some(Key::RightShift),
        0xffe3 => Some(Key::LeftCtrl),
        0xffe4 => Some(Key::RightCtrl),
        0xffbe..=0xffc9 => Some(Key::F((sym - 0xffbe + 1) as u8)),
        0x30..=0x39 => Some(Key::Num((sym - 0x30) as u8)),
        0x41..=0x5a => Some(Key::Char((sym as u8 as char).to_ascii_lowercase())),
        0x61..=0x7a => Some(Key::Char(sym as u8 as char)),
        0x21..=0x7e => Some(Key::Char(sym as u8 as char)),
        _ => None,
    }
}
