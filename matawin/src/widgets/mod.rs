//! MataWin-native widgets.
//!
//! Ordinary controls are software rendered by MataWin itself. Input comes from
//! `matawin::Window`, pixels go through `matawin::Framebuffer`, text is decoded
//! by `matafonts`, images by `mataimage`, and audio by `mata-audio`.
//!
//! The optional `webview` feature enables the sole platform-toolkit exception:
//! WebView2 on Windows and WebKitGTK 4.1 on Linux. It is disabled by default.
//! Android supports MataWin's software widgets; the optional `webview` backend
//! is currently desktop-only.

#[cfg(all(feature = "webview", target_os = "android"))]
compile_error!(
    "matawin's optional `webview` feature is not implemented on Android; disable `webview` when targeting Android"
);

/// Contains the catalog implementation.
///
/// Visibility: **public**.
pub mod catalog;
/// Contains the font implementation.
///
/// Visibility: **public**.
pub mod font;
/// Contains the media implementation.
///
/// Visibility: **public**.
pub mod media;
/// Contains the style implementation.
///
/// Visibility: **public**.
pub mod style;

/// Contains the controls implementation.
///
/// Visibility: **public**.
pub mod controls;
/// Contains the events implementation.
///
/// Visibility: **public**.
pub mod events;
/// Contains the geometry implementation.
///
/// Visibility: **public**.
pub mod geometry;
/// Contains the render implementation.
///
/// Visibility: **private**.
mod render;
#[cfg(feature = "webview")]
/// Contains the webview implementation.
///
/// Visibility: **public**.
pub mod webview;
/// Contains the window implementation.
///
/// Visibility: **public**.
pub mod window;

#[cfg(all(feature = "webview", target_os = "windows"))]
/// Contains the windows implementation.
///
/// Visibility: **crate-visible**.
pub(crate) mod windows;
#[cfg(all(feature = "webview", target_os = "linux"))]
/// Contains the linux implementation.
///
/// Visibility: **private**.
mod linux;

#[cfg(all(feature = "webview", target_os = "windows"))]
use windows as webview_platform;
#[cfg(all(feature = "webview", target_os = "linux"))]
use linux as webview_platform;

pub use catalog::*;
pub use controls::{ Button, TextField };
pub use events::{ WidgetError, WidgetEvent, WidgetId };
pub use font::FontRenderer;
pub use geometry::{ Anchors, Rect, ResizePolicy };
pub use media::{ AudioSource, AudioSourceState };
pub use style::*;
#[cfg(feature = "webview")]
pub use webview::WebView;
pub use window::WidgetWindow;

pub(crate) use events::next_id;
#[cfg(feature = "webview")]
pub(crate) use events::push_event;
