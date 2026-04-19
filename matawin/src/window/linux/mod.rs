#[cfg_attr(feature = "x11", path = "x11/mod.rs")]
#[cfg_attr(feature = "wayland", path = "wayland/mod.rs")]
mod platform;

pub use platform::PlatformWindow;

// pub use super::{WindowEvent, Key};