//! Positioning and z-order.

use crate::widgets::WidgetId;
use super::Length;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available position mode choices.
///
/// Visibility: **public**.
pub enum PositionMode {
    #[default]
    /// Represents the static variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Static,
    /// Represents the relative variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Relative,
    /// Represents the absolute variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Absolute,
}
#[derive(Debug, Clone, Copy, Default, PartialEq)]
/// Stores the state and data for insets.
///
/// Visibility: **public**.
pub struct Insets {
    /// Stores top.
    ///
    /// Visibility: **public**.
    pub top: Option<Length>,
    /// Stores right.
    ///
    /// Visibility: **public**.
    pub right: Option<Length>,
    /// Stores bottom.
    ///
    /// Visibility: **public**.
    pub bottom: Option<Length>,
    /// Stores left.
    ///
    /// Visibility: **public**.
    pub left: Option<Length>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq)]
/// Stores the state and data for positioning.
///
/// Visibility: **public**.
pub struct Positioning {
    /// Stores mode.
    ///
    /// Visibility: **public**.
    pub mode: PositionMode,
    /// Stores insets.
    ///
    /// Visibility: **public**.
    pub insets: Insets,
    /// Stores relative to.
    ///
    /// Visibility: **public**.
    pub relative_to: Option<WidgetId>,
    /// Stores z index.
    ///
    /// Visibility: **public**.
    pub z_index: i32,
}
