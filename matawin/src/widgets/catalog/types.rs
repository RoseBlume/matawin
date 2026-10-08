//! Shared catalog enums.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available orientation choices.
///
/// Visibility: **public**.
pub enum Orientation {
    /// Represents the horizontal variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Horizontal,
    #[default]
    /// Represents the vertical variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available align choices.
///
/// Visibility: **public**.
pub enum Align {
    /// Represents the start variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Start,
    /// Represents the center variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Center,
    /// Represents the end variant.
    ///
    /// Visibility: inherited from the **public** enum.
    End,
    /// Represents the fill variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Fill,
    #[default]
    /// Represents the baseline variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available selection mode choices.
///
/// Visibility: **public**.
pub enum SelectionMode {
    /// Represents the none variant.
    ///
    /// Visibility: inherited from the **public** enum.
    None,
    /// Represents the single variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Single,
    /// Represents the browse variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Browse,
    #[default]
    /// Represents the multiple variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Multiple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available policy type choices.
///
/// Visibility: **public**.
pub enum PolicyType {
    /// Represents the always variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Always,
    #[default]
    /// Represents the automatic variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Automatic,
    /// Represents the never variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Never,
    /// Represents the external variant.
    ///
    /// Visibility: inherited from the **public** enum.
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available position type choices.
///
/// Visibility: **public**.
pub enum PositionType {
    /// Represents the left variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Left,
    /// Represents the right variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Right,
    /// Represents the top variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Top,
    #[default]
    /// Represents the bottom variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available revealer transition choices.
///
/// Visibility: **public**.
pub enum RevealerTransition {
    /// Represents the none variant.
    ///
    /// Visibility: inherited from the **public** enum.
    None,
    /// Represents the crossfade variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Crossfade,
    /// Represents the slide right variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SlideRight,
    /// Represents the slide left variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SlideLeft,
    /// Represents the slide up variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SlideUp,
    #[default]
    /// Represents the slide down variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SlideDown,
    /// Represents the swing right variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SwingRight,
    /// Represents the swing left variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SwingLeft,
    /// Represents the swing up variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SwingUp,
    /// Represents the swing down variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SwingDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available content fit choices.
///
/// Visibility: **public**.
pub enum ContentFit {
    /// Represents the fill variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Fill,
    /// Represents the contain variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Contain,
    /// Represents the cover variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Cover,
    #[default]
    /// Represents the scale down variant.
    ///
    /// Visibility: inherited from the **public** enum.
    ScaleDown,
}
