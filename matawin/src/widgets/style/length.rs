//! CSS-like length units and resolution.

use std::{ fmt, str::FromStr };

#[derive(Debug, Clone, Copy, PartialEq)]
/// Defines the available length choices.
///
/// Visibility: **public**.
pub enum Length {
    /// Represents the px variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Px(f32),
    /// Represents the percent variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Percent(f32),
    /// Represents the vw variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Vw(f32),
    /// Represents the vh variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Vh(f32),
    /// Represents the vmin variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Vmin(f32),
    /// Represents the vmax variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Vmax(f32),
    /// Represents the em variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Em(f32),
    /// Represents the rem variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Rem(f32),
    /// Represents the auto variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Auto,
}

impl Default for Length {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::Px(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Defines the available axis choices.
///
/// Visibility: **public**.
pub enum Axis {
    /// Represents the horizontal variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Horizontal,
    /// Represents the vertical variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Vertical,
}

#[derive(Debug, Clone, Copy)]
/// Stores the state and data for length context.
///
/// Visibility: **public**.
pub struct LengthContext {
    /// Stores viewport.
    ///
    /// Visibility: **public**.
    pub viewport: (f32, f32),
    /// Stores containing.
    ///
    /// Visibility: **public**.
    pub containing: (f32, f32),
    /// Stores font size.
    ///
    /// Visibility: **public**.
    pub font_size: f32,
    /// Stores root font size.
    ///
    /// Visibility: **public**.
    pub root_font_size: f32,
}

impl LengthContext {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(
        viewport: (u32, u32),
        containing: (u32, u32),
        font_size: f32,
        root_font_size: f32
    ) -> Self {
        Self {
            viewport: (viewport.0 as f32, viewport.1 as f32),
            containing: (containing.0 as f32, containing.1 as f32),
            font_size,
            root_font_size,
        }
    }
}

impl Length {
    /// Resolves resolve from the current state.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resolve operation. Visibility: public."]
    pub fn resolve(self, axis: Axis, ctx: LengthContext) -> Option<f32> {
        let containing = match axis {
            Axis::Horizontal => ctx.containing.0,
            Axis::Vertical => ctx.containing.1,
        };
        Some(match self {
            Self::Px(v) => v,
            Self::Percent(v) => (containing * v) / 100.0,
            Self::Vw(v) => (ctx.viewport.0 * v) / 100.0,
            Self::Vh(v) => (ctx.viewport.1 * v) / 100.0,
            Self::Vmin(v) => (ctx.viewport.0.min(ctx.viewport.1) * v) / 100.0,
            Self::Vmax(v) => (ctx.viewport.0.max(ctx.viewport.1) * v) / 100.0,
            Self::Em(v) => ctx.font_size * v,
            Self::Rem(v) => ctx.root_font_size * v,
            Self::Auto => {
                return None;
            }
        })
    }

    /// Parses parse from the supplied representation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(value: &str) -> Result<Self, String> {
        value.parse()
    }

    /// Performs the interpolate operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the interpolate operation. Visibility: crate-visible."]
    pub(crate) fn interpolate(self, other: Self, t: f32) -> Self {
        macro_rules! same {
            ($a:ident, $b:ident, $ctor:ident) => { Self::$ctor($a + ($b - $a) * t) };
        }
        match (self, other) {
            (Self::Px(a), Self::Px(b)) => same!(a, b, Px),
            (Self::Percent(a), Self::Percent(b)) => same!(a, b, Percent),
            (Self::Vw(a), Self::Vw(b)) => same!(a, b, Vw),
            (Self::Vh(a), Self::Vh(b)) => same!(a, b, Vh),
            (Self::Vmin(a), Self::Vmin(b)) => same!(a, b, Vmin),
            (Self::Vmax(a), Self::Vmax(b)) => same!(a, b, Vmax),
            (Self::Em(a), Self::Em(b)) => same!(a, b, Em),
            (Self::Rem(a), Self::Rem(b)) => same!(a, b, Rem),
            _ if t >= 1.0 => other,
            _ => self,
        }
    }
}

impl FromStr for Length {
    /// Type alias for err.
    ///
    /// Visibility: **private**.
    type Err = String;
    /// Performs the from str operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the from str operation. Visibility: private/internal."]
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let s = raw.trim().to_ascii_lowercase();
        if s == "auto" {
            return Ok(Self::Auto);
        }
        if s == "0" || s == "+0" || s == "-0" {
            return Ok(Self::Px(0.0));
        }
        for (suffix, make) in [
            ("vmin", Self::Vmin as fn(f32) -> Self),
            ("vmax", Self::Vmax as fn(f32) -> Self),
            ("rem", Self::Rem as fn(f32) -> Self),
            ("vw", Self::Vw as fn(f32) -> Self),
            ("vh", Self::Vh as fn(f32) -> Self),
            ("em", Self::Em as fn(f32) -> Self),
            ("px", Self::Px as fn(f32) -> Self),
            ("%", Self::Percent as fn(f32) -> Self),
        ] {
            if let Some(n) = s.strip_suffix(suffix) {
                let value = n
                    .trim()
                    .parse::<f32>()
                    .map_err(|_| format!("invalid length: {raw}"))?;
                return Ok(make(value));
            }
        }
        Err(format!("unsupported length unit in {raw:?}"))
    }
}

impl fmt::Display for Length {
    /// Performs the fmt operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the fmt operation. Visibility: private/internal."]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Px(v) => write!(f, "{v}px"),
            Self::Percent(v) => write!(f, "{v}%"),
            Self::Vw(v) => write!(f, "{v}vw"),
            Self::Vh(v) => write!(f, "{v}vh"),
            Self::Vmin(v) => write!(f, "{v}vmin"),
            Self::Vmax(v) => write!(f, "{v}vmax"),
            Self::Em(v) => write!(f, "{v}em"),
            Self::Rem(v) => write!(f, "{v}rem"),
            Self::Auto => f.write_str("auto"),
        }
    }
}
