//! MataWin-native widget catalog grouped by responsibility.
//!
//! The catalog remains flat at its public surface (`catalog::*`) for source
//! compatibility, while implementation is split into focused submodules.

/// Contains the base implementation.
///
/// Visibility: **public**.
pub mod base;
/// Contains the buttons implementation.
///
/// Visibility: **public**.
pub mod buttons;
/// Contains the containers implementation.
///
/// Visibility: **public**.
pub mod containers;
/// Contains the drawing implementation.
///
/// Visibility: **public**.
pub mod drawing;
/// Contains the image implementation.
///
/// Visibility: **public**.
pub mod image;
/// Contains the lists implementation.
///
/// Visibility: **public**.
pub mod lists;
/// Contains the media widgets implementation.
///
/// Visibility: **public**.
pub mod media_widgets;
/// Contains the misc implementation.
///
/// Visibility: **public**.
pub mod misc;
/// Contains the text implementation.
///
/// Visibility: **public**.
pub mod text;
/// Contains the types implementation.
///
/// Visibility: **public**.
pub mod types;
/// Contains the values implementation.
///
/// Visibility: **public**.
pub mod values;

pub use base::{ ContainerState, Widget, WidgetExt, WidgetLike };
pub use buttons::*;
pub use containers::*;
pub use drawing::*;
pub use image::*;
pub use lists::*;
pub use media_widgets::*;
pub use misc::*;
pub use text::*;
pub use types::*;
pub use values::*;
