//! Implements the `events` module.
//!
//! This module documents both public API items and internal implementation details.

use std::sync::{ Mutex, OnceLock };
use std::collections::HashSet;
/// Static storage used for event queue.
///
/// Visibility: **public**.
pub static EVENT_QUEUE: OnceLock<Mutex<Vec<WindowEvent>>> = OnceLock::new();
/// Static storage used for keys down.
///
/// Visibility: **public**.
pub static KEYS_DOWN: OnceLock<Mutex<HashSet<Key>>> = OnceLock::new();
/// Static storage used for touch queue.
///
/// Visibility: **public**.
pub static TOUCH_QUEUE: OnceLock<Mutex<Vec<Touch>>> = OnceLock::new();
#[derive(Debug, Clone, Copy)]
/// Defines the available window event choices.
///
/// Visibility: **public**.
pub enum WindowEvent {
    /// Represents the close variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Close,
    /// Represents the resized variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Resized(u32, u32),
    /// Represents the maximized variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Maximized(u32, u32),
    /// Represents the restored variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Restored(u32, u32),
    /// Represents the minimized variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Minimized,
    /// Represents the repositioned variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Repositioned(i32, i32),
    /// Represents the key pressed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    KeyPressed(Key),
    /// Represents the key held variant.
    ///
    /// Visibility: inherited from the **public** enum.
    KeyHeld(Key),
    /// Represents the key released variant.
    ///
    /// Visibility: inherited from the **public** enum.
    KeyReleased(Key),
    /// Pointer position in client-area pixels.
    CursorMoved(i32, i32),
    /// Scroll-wheel delta. Positive Y scrolls up; positive X scrolls right.
    Scroll(i32, i32),
}

/// Stores the state and data for touch.
///
/// Visibility: **public**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Touch {
    /// Stores device id.
    ///
    /// Visibility: **public**.
    pub device_id: String,
    /// Stores event.
    ///
    /// Visibility: **public**.
    pub event: TouchEvent,
}

/// Defines the available touch event choices.
///
/// Visibility: **public**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEvent {
    /// Represents the started variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Started(u64, u64),
    /// Represents the ended variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Ended(u64, u64),
    /// Represents the moved variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Moved(u64, u64),
    /// Represents the cancelled variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Cancelled,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
/// Defines the available key choices.
///
/// Visibility: **public**.
pub enum Key {
    /// Represents the char variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Char(char),
    /// Represents the num variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Num(u8),

    /// Represents the backspace variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Backspace,
    /// Represents the enter variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Enter,
    /// Represents the tab variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Tab,
    /// Represents the escape variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Escape,

    /// Represents the left ctrl variant.
    ///
    /// Visibility: inherited from the **public** enum.
    LeftCtrl,
    /// Represents the right ctrl variant.
    ///
    /// Visibility: inherited from the **public** enum.
    RightCtrl,
    /// Represents the left shift variant.
    ///
    /// Visibility: inherited from the **public** enum.
    LeftShift,
    /// Represents the right shift variant.
    ///
    /// Visibility: inherited from the **public** enum.
    RightShift,

    /// Represents the space variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Space,

    /// Represents the up variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Up,
    /// Represents the down variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Down,
    /// Represents the left variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Left,
    /// Represents the right variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Right,

    /// Represents the f variant.
    ///
    /// Visibility: inherited from the **public** enum.
    F(u8),

    /// Represents the mouse left variant.
    ///
    /// Visibility: inherited from the **public** enum.
    MouseLeft,
    /// Represents the mouse right variant.
    ///
    /// Visibility: inherited from the **public** enum.
    MouseRight,
    /// Represents the mouse middle variant.
    ///
    /// Visibility: inherited from the **public** enum.
    MouseMiddle,
}

/// Stores the state and data for handlers.
///
/// Visibility: **public**.
pub struct Handlers {
    /// Stores on close.
    ///
    /// Visibility: **public**.
    pub on_close: Option<Box<dyn FnMut()>>,
    /// Stores on resized.
    ///
    /// Visibility: **public**.
    pub on_resized: Option<Box<dyn FnMut(u32, u32)>>,
    /// Stores on maximized.
    ///
    /// Visibility: **public**.
    pub on_maximized: Option<Box<dyn FnMut(u32, u32)>>,
    /// Stores on restored.
    ///
    /// Visibility: **public**.
    pub on_restored: Option<Box<dyn FnMut(u32, u32)>>,
    /// Stores on minimized.
    ///
    /// Visibility: **public**.
    pub on_minimized: Option<Box<dyn FnMut()>>,
    /// Stores on repositioned.
    ///
    /// Visibility: **public**.
    pub on_repositioned: Option<Box<dyn FnMut(i32, i32)>>,
    /// Stores on key pressed.
    ///
    /// Visibility: **public**.
    pub on_key_pressed: Option<Box<dyn FnMut(Key)>>,
    /// Stores on key held.
    ///
    /// Visibility: **public**.
    pub on_key_held: Option<Box<dyn FnMut(Key)>>,
    /// Stores on key released.
    ///
    /// Visibility: **public**.
    pub on_key_released: Option<Box<dyn FnMut(Key)>>,
    /// Stores on cursor moved.
    ///
    /// Visibility: **public**.
    pub on_cursor_moved: Option<Box<dyn FnMut(i32, i32)>>,
    /// Stores on scroll.
    ///
    /// Visibility: **public**.
    pub on_scroll: Option<Box<dyn FnMut(i32, i32)>>,
}

impl Default for Handlers {
    /// Performs the default operation.
    ///
    /// Visibility: **private**.
    #[doc = "Creates the default value. Visibility: private/internal."]
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
            on_key_released: None,
            on_cursor_moved: None,
            on_scroll: None,
        }
    }
}
