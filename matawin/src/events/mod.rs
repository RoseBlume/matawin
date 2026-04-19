use std::sync::{Mutex, OnceLock};
use std::collections::HashSet;
pub static EVENT_QUEUE: OnceLock<Mutex<Vec<WindowEvent>>> = OnceLock::new();
pub static KEYS_DOWN: OnceLock<Mutex<HashSet<Key>>> = OnceLock::new();
pub static TOUCH_QUEUE: OnceLock<Mutex<Vec<Touch>>> = OnceLock::new();
#[derive(Debug, Clone, Copy)]
pub enum WindowEvent {
    Close,
    Resized(u32, u32),
    Maximized(u32, u32),
    Restored(u32, u32),
    Minimized,
    Repositioned(i32, i32),
    KeyPressed(Key),
    KeyHeld(Key),
    KeyReleased(Key),
}

pub struct Touch {
    pub device_id: String,
    pub event: TouchEvent,
}

pub enum TouchEvent {
    Started(u64, u64),
    Ended(u64, u64),
    Moved(u64, u64),
    Cancelled
}



#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum Key {
    Char(char),
    Num(u8),

    Backspace,
    Enter,
    Tab,
    Escape,

    LeftCtrl,
    RightCtrl,
    LeftShift,
    RightShift,

    Space,

    Up,
    Down,
    Left,
    Right,

    F(u8),

    MouseLeft,
    MouseRight,
    MouseMiddle,
}


pub struct Handlers {
    pub on_close: Option<Box<dyn FnMut()>>,
    pub on_resized: Option<Box<dyn FnMut(u32, u32)>>,
    pub on_maximized: Option<Box<dyn FnMut(u32, u32)>>,
    pub on_restored: Option<Box<dyn FnMut(u32, u32)>>,
    pub on_minimized: Option<Box<dyn FnMut()>>,
    pub on_repositioned: Option<Box<dyn FnMut(i32, i32)>>,
    pub on_key_pressed: Option<Box<dyn FnMut(Key)>>,
    pub on_key_held: Option<Box<dyn FnMut(Key)>>,
    pub on_key_released: Option<Box<dyn FnMut(Key)>>
}

impl Default for Handlers {
    fn default() -> Self {
        Self {
            on_close: None,
            on_resized: None,
            on_maximized: None,
            on_restored: None,
            on_minimized: None,
            on_repositioned: None,
            on_key_pressed: None,
            on_key_held: None,
            on_key_released: None
        }
    }
}