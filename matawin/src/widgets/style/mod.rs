//! Dependency-free CSS-like styling primitives used by MataWin widgets.
//!
//! The style engine supports viewport/font/percentage lengths, the CSS box
//! model, relative/absolute positioning, z-order, border radii, typography,
//! opacity, pseudo-classes, media queries, and keyframe animations.

/// Contains the animation implementation.
///
/// Visibility: **private**.
mod animation;
/// Contains the box model implementation.
///
/// Visibility: **private**.
mod box_model;
/// Contains the length implementation.
///
/// Visibility: **private**.
mod length;
/// Contains the media implementation.
///
/// Visibility: **private**.
mod media;
/// Contains the position implementation.
///
/// Visibility: **private**.
mod position;
/// Contains the pseudo implementation.
///
/// Visibility: **private**.
mod pseudo;
/// Contains the state implementation.
///
/// Visibility: **private**.
mod state;
/// Contains the typography implementation.
///
/// Visibility: **private**.
mod typography;

pub use animation::{ parse_duration, AnimationIterationCount, AnimationSpec, Keyframe, Keyframes };
pub use box_model::{
    BorderRadius,
    CornerRadius,
    Edges,
    ResolvedBorderRadius,
    ResolvedCornerRadius,
    ResolvedEdges,
};
pub use length::{ Axis, Length, LengthContext };
pub use media::{ MediaOrientation, MediaQuery };
pub use position::{ Insets, PositionMode, Positioning };
pub use pseudo::{ InteractionState, NthChild, PseudoClass, PseudoSelector, TreeState };
pub use state::{
    ComputedStyle,
    IntoLength,
    Style,
    StyleHandle,
    StylePatch,
    StyleProperties,
    StyleRule,
    Stylable,
};
pub use typography::{ FontPatch, FontProperties, FontStretch, FontStyle, FontVariant, FontWeight };
