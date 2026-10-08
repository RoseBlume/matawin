//! Cross-platform native window creation, presentation, and input handling.
//!
//! `matawin` intentionally contains only the platform/windowing layer. Game
//! objects, sprites, audio, menus, image helpers, and other engine facilities
//! live in the separate `mata-engine` crate.

/// Contains the buffer implementation.
///
/// Visibility: **public**.
pub mod buffer;
/// Contains the events implementation.
///
/// Visibility: **public**.
pub mod events;
/// Contains the window implementation.
///
/// Visibility: **public**.
pub mod window;
/// Contains the widgets implementation.
///
/// Visibility: **public**.
pub mod widgets;

pub use buffer::Framebuffer;
pub use window::Window;
#[cfg(target_os = "android")]
pub use window::AndroidApp;
