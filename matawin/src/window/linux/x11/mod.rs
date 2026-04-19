use std::ffi::{CString, c_void};
use std::ptr::{null, null_mut};


use crate::buffer::Framebuffer;
use crate::events::{WindowEvent, Key, EVENT_QUEUE};

mod show;
mod drop;

type Display = *mut c_void;
type WindowHandle = u64;
type GC = *mut c_void;

// #[repr(C)]
// struct XEvent {
//     pad: [u64; 24], // big enough for all variants
// }

#[repr(C)]
struct XImage {
    width: i32,
    height: i32,
    xoffset: i32,
    format: i32,
    data: *mut i8,
    byte_order: i32,
    bitmap_unit: i32,
    bitmap_bit_order: i32,
    bitmap_pad: i32,
    depth: i32,
    bytes_per_line: i32,
    bits_per_pixel: i32,
    red_mask: u64,
    green_mask: u64,
    blue_mask: u64,
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const i8) -> Display;
    fn XDefaultScreen(display: Display) -> i32;
    fn XRootWindow(display: Display, screen: i32) -> WindowHandle;

    fn XCreateSimpleWindow(
        display: Display,
        parent: WindowHandle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        border_width: u32,
        border: u64,
        background: u64,
    ) -> WindowHandle;

    fn XMapWindow(display: Display, window: WindowHandle);
    fn XStoreName(display: Display, window: WindowHandle, name: *const i8);

    fn XCreateGC(display: Display, window: WindowHandle, valuemask: u64, values: *mut c_void) -> GC;

    fn XNextEvent(display: Display, event: *mut XEvent);
    fn XPending(display: Display) -> i32;

    fn XSelectInput(display: Display, window: WindowHandle, event_mask: i64);

    fn XCreateImage(
        display: Display,
        visual: *mut c_void,
        depth: u32,
        format: i32,
        offset: i32,
        data: *mut i8,
        width: u32,
        height: u32,
        bitmap_pad: i32,
        bytes_per_line: i32,
    ) -> *mut XImage;

    fn XPutImage(
        display: Display,
        drawable: WindowHandle,
        gc: GC,
        image: *mut XImage,
        src_x: i32,
        src_y: i32,
        dest_x: i32,
        dest_y: i32,
        width: u32,
        height: u32,
    );
}

const EXPOSURE_MASK: i64 = 0x8000;
const KEY_PRESS_MASK: i64 = 0x0001;
const BUTTON_PRESS_MASK: i64 = 0x0004;
const STRUCTURE_NOTIFY_MASK: i64 = 0x20000;

fn push_event(event: WindowEvent) {
    if let Some(q) = EVENT_QUEUE.get() {
        q.lock().unwrap().push(event);
    }
}

pub struct PlatformWindow {
    display: Display,
    window: WindowHandle,
    gc: GC,
    wm_delete: Atom,
    size: (u32, u32),
    image: *mut XImage,
}

impl PlatformWindow {
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        EVENT_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        unsafe {
            let display = XOpenDisplay(null());
            let screen = XDefaultScreen(display);
            let root = XRootWindow(display, screen);

            let window = XCreateSimpleWindow(
                display,
                root,
                0,
                0,
                size.0,
                size.1,
                0,
                0,
                0,
            );

            let c_title = CString::new(title).unwrap();
            XStoreName(display, window, c_title.as_ptr());
            let wm_delete = XInternAtom(display, CString::new("WM_DELETE_WINDOW").unwrap().as_ptr(), 0);
            XSetWMProtocols(display, window, &wm_delete as *const _ as *mut _, 1);

            XSelectInput(
                display,
                window,
                EXPOSURE_MASK
                    | KEY_PRESS_MASK
                    | BUTTON_PRESS_MASK
                    | STRUCTURE_NOTIFY_MASK,
            );

            XMapWindow(display, window);

            let gc = XCreateGC(display, window, 0, null_mut());

            let image = XCreateImage(
                display,
                null_mut(),
                24,
                2, // ZPixmap
                0,
                null_mut(),
                size.0,
                size.1,
                32,
                0,
            );

            Self {
                display,
                window,
                gc,
                size,
                image,
                wm_delete,
            }
        }
    }

    pub fn pump_events(&self) {
        unsafe {
            let mut event: XEvent = std::mem::zeroed();

            while XPending(self.display) > 0 {
                XNextEvent(self.display, &mut event);

                match event.type_ {
                    KEY_PRESS => {
                        let key_event =
                            &*(&event.xkey as *const _ as *const XKeyEvent);

                        let keysym =
                            XLookupKeysym(key_event as *const _ as *mut _, 0);

                        if let Some(key) = Self::map_key(keysym) {
                            push_event(WindowEvent::KeyPressed(key));
                        }
                    }

                    BUTTON_PRESS => {
                        let btn =
                            &*(&event.xbutton as *const _ as *const XButtonEvent);

                        match btn.button {
                            1 => push_event(WindowEvent::KeyPressed(Key::MouseLeft)),
                            2 => push_event(WindowEvent::KeyPressed(Key::MouseMiddle)),
                            3 => push_event(WindowEvent::KeyPressed(Key::MouseRight)),
                            _ => {}
                        }
                    }

                    CONFIGURE_NOTIFY => {
                        let cfg =
                            &*(&event.xconfigure as *const _ as *const XConfigureEvent);

                        push_event(WindowEvent::Resized(
                            cfg.width as u32,
                            cfg.height as u32,
                        ));

                        push_event(WindowEvent::Repositioned(cfg.x, cfg.y));
                    }

                    CLIENT_MESSAGE => {
                        let client =
                            &*(&event as *const _ as *const XClientMessageEvent);

                        if client.data[0] == self.wm_delete {
                            push_event(WindowEvent::Close);
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    pub fn present(&mut self, fb: &Framebuffer) {
        unsafe {
            (*self.image).data = fb.as_ptr() as *mut i8;

            XPutImage(
                self.display,
                self.window,
                self.gc,
                self.image,
                0,
                0,
                0,
                0,
                self.size.0,
                self.size.1,
            );
        }
    }

    pub fn set_title(&self, title: &str) {
        let c_title = CString::new(title).unwrap();

        unsafe {
            XStoreName(self.display, self.window, c_title.as_ptr());
        }
    }

    pub fn handle(&self) -> *mut c_void {
        self.window as *mut c_void
    }

    fn map_key(keysym: KeySym) -> Option<Key> {
        match keysym {
            0xff08 => Some(Key::Backspace),
            0xff0d => Some(Key::Enter),
            0xff09 => Some(Key::Tab),
            0xff1b => Some(Key::Escape),
            0x20 => Some(Key::Space),

            0xff51 => Some(Key::Left),
            0xff52 => Some(Key::Up),
            0xff53 => Some(Key::Right),
            0xff54 => Some(Key::Down),

            0xffe1 => Some(Key::LeftShift),
            0xffe2 => Some(Key::RightShift),

            0xffe3 => Some(Key::LeftCtrl),
            0xffe4 => Some(Key::RightCtrl),

            0xffbe..=0xffc9 => Some(Key::F((keysym - 0xffbe + 1) as u8)),

            0x30..=0x39 => Some(Key::Num((keysym - 0x30) as u8)),

            0x61..=0x7a => Some(Key::Char((keysym as u8) as char)),

            0x41..=0x5a => Some(Key::Char((keysym as u8 as char).to_ascii_lowercase())),

            _ => None,
        }
    }
}


type Atom = u64;
type KeySym = u64;

#[link(name = "X11")]
unsafe extern "C" {
    fn XLookupKeysym(event: *mut XKeyEvent, index: i32) -> KeySym;

    fn XInternAtom(display: Display, name: *const i8, only_if_exists: i32) -> Atom;

    fn XSetWMProtocols(
        display: Display,
        window: WindowHandle,
        protocols: *mut Atom,
        count: i32,
    ) -> i32;
}

#[repr(C)]
pub struct XClientMessageEvent {
    pub type_: i32,
    pub serial: u64,
    pub send_event: i32,
    pub display: Display,
    pub window: WindowHandle,
    pub message_type: u64,
    pub format: i32,
    pub data: [u64; 5],
}

#[repr(C)]
pub struct XKeyEvent {
    pub type_: i32,
    pub serial: u64,
    pub send_event: i32,
    pub display: Display,
    pub window: WindowHandle,
    pub root: WindowHandle,
    pub subwindow: WindowHandle,
    pub time: u64,
    pub x: i32,
    pub y: i32,
    pub x_root: i32,
    pub y_root: i32,
    pub state: u32,
    pub keycode: u32,
    pub same_screen: i32,
}

#[repr(C)]
pub struct XButtonEvent {
    pub type_: i32,
    pub serial: u64,
    pub send_event: i32,
    pub display: Display,
    pub window: WindowHandle,
    pub root: WindowHandle,
    pub subwindow: WindowHandle,
    pub time: u64,
    pub x: i32,
    pub y: i32,
    pub x_root: i32,
    pub y_root: i32,
    pub state: u32,
    pub button: u32,
    pub same_screen: i32,
}

#[repr(C)]
pub struct XConfigureEvent {
    pub type_: i32,
    pub serial: u64,
    pub send_event: i32,
    pub display: Display,
    pub event: WindowHandle,
    pub window: WindowHandle,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub border_width: i32,
    pub above: WindowHandle,
    pub override_redirect: i32,
}

#[repr(C)]
pub union XEvent {
    pub type_: i32,
    pub xkey: std::mem::ManuallyDrop<XKeyEvent>,
    pub xbutton: std::mem::ManuallyDrop<XButtonEvent>,
    pub xconfigure: std::mem::ManuallyDrop<XConfigureEvent>,
    pub pad: [u64; 24],
}

const KEY_PRESS: i32 = 2;
const BUTTON_PRESS: i32 = 4;
const CONFIGURE_NOTIFY: i32 = 22;
const CLIENT_MESSAGE: i32 = 33;