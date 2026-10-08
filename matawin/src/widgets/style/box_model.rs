//! Margin, padding, and CSS border-radius values.

use super::{ Axis, Length, LengthContext };

#[derive(Debug, Clone, Copy, PartialEq)]
/// Stores the state and data for edges.
///
/// Visibility: **public**.
pub struct Edges<T> {
    /// Stores top.
    ///
    /// Visibility: **public**.
    pub top: T,
    /// Stores right.
    ///
    /// Visibility: **public**.
    pub right: T,
    /// Stores bottom.
    ///
    /// Visibility: **public**.
    pub bottom: T,
    /// Stores left.
    ///
    /// Visibility: **public**.
    pub left: T,
}
impl<T: Default + Copy> Default for Edges<T> {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self { top: T::default(), right: T::default(), bottom: T::default(), left: T::default() }
    }
}
impl<T: Copy> Edges<T> {
    /// Performs the all operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the all operation. Visibility: public."]
    pub const fn all(v: T) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }
    /// Performs the vertical horizontal operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the vertical horizontal operation. Visibility: public."]
    pub const fn vertical_horizontal(vertical: T, horizontal: T) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
    /// Performs the trbl operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the trbl operation. Visibility: public."]
    pub const fn trbl(top: T, right: T, bottom: T, left: T) -> Self {
        Self { top, right, bottom, left }
    }
}
impl Edges<Length> {
    /// Parses parse from the supplied representation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(raw: &str) -> Result<Self, String> {
        let values: Result<Vec<_>, _> = raw.split_whitespace().map(Length::parse).collect();
        let v = values?;
        match v.as_slice() {
            [a] => Ok(Self::all(*a)),
            [v, h] => Ok(Self::vertical_horizontal(*v, *h)),
            [t, h, b] => Ok(Self { top: *t, right: *h, bottom: *b, left: *h }),
            [t, r, b, l] => Ok(Self::trbl(*t, *r, *b, *l)),
            _ => Err(format!("expected 1-4 lengths, got {raw:?}")),
        }
    }
    /// Resolves resolve from the current state.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resolve operation. Visibility: public."]
    pub fn resolve(self, ctx: LengthContext) -> ResolvedEdges {
        ResolvedEdges {
            top: self.top.resolve(Axis::Vertical, ctx).unwrap_or(0.0),
            right: self.right.resolve(Axis::Horizontal, ctx).unwrap_or(0.0),
            bottom: self.bottom.resolve(Axis::Vertical, ctx).unwrap_or(0.0),
            left: self.left.resolve(Axis::Horizontal, ctx).unwrap_or(0.0),
        }
    }
    /// Performs the interpolate operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the interpolate operation. Visibility: crate-visible."]
    pub(crate) fn interpolate(self, other: Self, t: f32) -> Self {
        Self {
            top: self.top.interpolate(other.top, t),
            right: self.right.interpolate(other.right, t),
            bottom: self.bottom.interpolate(other.bottom, t),
            left: self.left.interpolate(other.left, t),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
/// Stores the state and data for resolved edges.
///
/// Visibility: **public**.
pub struct ResolvedEdges {
    /// Stores top.
    ///
    /// Visibility: **public**.
    pub top: f32,
    /// Stores right.
    ///
    /// Visibility: **public**.
    pub right: f32,
    /// Stores bottom.
    ///
    /// Visibility: **public**.
    pub bottom: f32,
    /// Stores left.
    ///
    /// Visibility: **public**.
    pub left: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Stores the state and data for corner radius.
///
/// Visibility: **public**.
pub struct CornerRadius<T> {
    /// Stores x.
    ///
    /// Visibility: **public**.
    pub x: T,
    /// Stores y.
    ///
    /// Visibility: **public**.
    pub y: T,
}
impl<T: Default + Copy> Default for CornerRadius<T> {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self { x: T::default(), y: T::default() }
    }
}
impl<T: Copy> CornerRadius<T> {
    #[doc = "Creates a new value. Visibility: public."]
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
    #[doc = "Implements the circular operation. Visibility: public."]
    pub const fn circular(v: T) -> Self {
        Self { x: v, y: v }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Stores the state and data for border radius.
///
/// Visibility: **public**.
pub struct BorderRadius<T> {
    /// Stores top left.
    ///
    /// Visibility: **public**.
    pub top_left: CornerRadius<T>,
    /// Stores top right.
    ///
    /// Visibility: **public**.
    pub top_right: CornerRadius<T>,
    /// Stores bottom right.
    ///
    /// Visibility: **public**.
    pub bottom_right: CornerRadius<T>,
    /// Stores bottom left.
    ///
    /// Visibility: **public**.
    pub bottom_left: CornerRadius<T>,
}
impl<T: Default + Copy> Default for BorderRadius<T> {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self {
            top_left: CornerRadius::default(),
            top_right: CornerRadius::default(),
            bottom_right: CornerRadius::default(),
            bottom_left: CornerRadius::default(),
        }
    }
}
impl BorderRadius<Length> {
    /// Performs the all operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the all operation. Visibility: public."]
    pub fn all(v: Length) -> Self {
        let c = CornerRadius::circular(v);
        Self { top_left: c, top_right: c, bottom_right: c, bottom_left: c }
    }
    /// Two-value shorthand: top-left/bottom-right, then top-right/bottom-left.
    #[doc = "Implements the two operation. Visibility: public."]
    pub fn two(a: Length, b: Length) -> Self {
        Self {
            top_left: CornerRadius::circular(a),
            top_right: CornerRadius::circular(b),
            bottom_right: CornerRadius::circular(a),
            bottom_left: CornerRadius::circular(b),
        }
    }
    /// Three-value shorthand: top-left, top-right/bottom-left, bottom-right.
    #[doc = "Implements the three operation. Visibility: public."]
    pub fn three(a: Length, b: Length, c: Length) -> Self {
        Self {
            top_left: CornerRadius::circular(a),
            top_right: CornerRadius::circular(b),
            bottom_right: CornerRadius::circular(c),
            bottom_left: CornerRadius::circular(b),
        }
    }
    /// Four-value shorthand in top-left, top-right, bottom-right, bottom-left order.
    #[doc = "Implements the four operation. Visibility: public."]
    pub fn four(a: Length, b: Length, c: Length, d: Length) -> Self {
        Self {
            top_left: CornerRadius::circular(a),
            top_right: CornerRadius::circular(b),
            bottom_right: CornerRadius::circular(c),
            bottom_left: CornerRadius::circular(d),
        }
    }
    /// Elliptical four-corner shorthand. `x` and `y` use TL, TR, BR, BL order.
    #[doc = "Implements the elliptical operation. Visibility: public."]
    pub fn elliptical(x: [Length; 4], y: [Length; 4]) -> Self {
        Self {
            top_left: CornerRadius::new(x[0], y[0]),
            top_right: CornerRadius::new(x[1], y[1]),
            bottom_right: CornerRadius::new(x[2], y[2]),
            bottom_left: CornerRadius::new(x[3], y[3]),
        }
    }
    /// Parses parse from the supplied representation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut split = raw.splitn(2, '/');
        let x = parse_radius_list(split.next().unwrap_or(""))?;
        let y = match split.next() {
            Some(v) => parse_radius_list(v)?,
            None => x,
        };
        Ok(Self {
            top_left: CornerRadius { x: x[0], y: y[0] },
            top_right: CornerRadius { x: x[1], y: y[1] },
            bottom_right: CornerRadius { x: x[2], y: y[2] },
            bottom_left: CornerRadius { x: x[3], y: y[3] },
        })
    }
    /// Resolves resolve from the current state.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resolve operation. Visibility: public."]
    pub fn resolve(self, ctx: LengthContext) -> ResolvedBorderRadius {
        /// Performs the c operation.
        ///
        /// Visibility: **private**.
        #[doc = "Implements the c operation. Visibility: private/internal."]
        fn c(v: CornerRadius<Length>, ctx: LengthContext) -> ResolvedCornerRadius {
            ResolvedCornerRadius {
                x: v.x.resolve(Axis::Horizontal, ctx).unwrap_or(0.0).max(0.0),
                y: v.y.resolve(Axis::Vertical, ctx).unwrap_or(0.0).max(0.0),
            }
        }
        ResolvedBorderRadius {
            top_left: c(self.top_left, ctx),
            top_right: c(self.top_right, ctx),
            bottom_right: c(self.bottom_right, ctx),
            bottom_left: c(self.bottom_left, ctx),
        }
    }
    /// Performs the interpolate operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the interpolate operation. Visibility: crate-visible."]
    pub(crate) fn interpolate(self, o: Self, t: f32) -> Self {
        /// Performs the c operation.
        ///
        /// Visibility: **private**.
        #[doc = "Implements the c operation. Visibility: private/internal."]
        fn c(a: CornerRadius<Length>, b: CornerRadius<Length>, t: f32) -> CornerRadius<Length> {
            CornerRadius { x: a.x.interpolate(b.x, t), y: a.y.interpolate(b.y, t) }
        }
        Self {
            top_left: c(self.top_left, o.top_left, t),
            top_right: c(self.top_right, o.top_right, t),
            bottom_right: c(self.bottom_right, o.bottom_right, t),
            bottom_left: c(self.bottom_left, o.bottom_left, t),
        }
    }
}
/// Parses radius list from the supplied representation.
///
/// Visibility: **private**.
#[doc = "Parses radius list. Visibility: private/internal."]
fn parse_radius_list(raw: &str) -> Result<[Length; 4], String> {
    let v: Result<Vec<_>, _> = raw.split_whitespace().map(Length::parse).collect();
    let v = v?;
    Ok(match v.as_slice() {
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => {
            return Err(format!("invalid border-radius: {raw:?}"));
        }
    })
}
#[derive(Debug, Clone, Copy, Default, PartialEq)]
/// Stores the state and data for resolved corner radius.
///
/// Visibility: **public**.
pub struct ResolvedCornerRadius {
    /// Stores x.
    ///
    /// Visibility: **public**.
    pub x: f32,
    /// Stores y.
    ///
    /// Visibility: **public**.
    pub y: f32,
}
#[derive(Debug, Clone, Copy, Default, PartialEq)]
/// Stores the state and data for resolved border radius.
///
/// Visibility: **public**.
pub struct ResolvedBorderRadius {
    /// Stores top left.
    ///
    /// Visibility: **public**.
    pub top_left: ResolvedCornerRadius,
    /// Stores top right.
    ///
    /// Visibility: **public**.
    pub top_right: ResolvedCornerRadius,
    /// Stores bottom right.
    ///
    /// Visibility: **public**.
    pub bottom_right: ResolvedCornerRadius,
    /// Stores bottom left.
    ///
    /// Visibility: **public**.
    pub bottom_left: ResolvedCornerRadius,
}
