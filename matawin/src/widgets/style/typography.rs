//! Font properties shared by all stylable widgets.

use super::Length;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available font style choices.
///
/// Visibility: **public**.
pub enum FontStyle {
    #[default]
    /// Represents the normal variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Normal,
    /// Represents the italic variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Italic,
    /// Represents the oblique variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Oblique,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available font variant choices.
///
/// Visibility: **public**.
pub enum FontVariant {
    #[default]
    /// Represents the normal variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Normal,
    /// Represents the small caps variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SmallCaps,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Defines the available font weight choices.
///
/// Visibility: **public**.
pub enum FontWeight {
    /// Represents the thin variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Thin,
    /// Represents the extra light variant.
    ///
    /// Visibility: inherited from the **public** enum.
    ExtraLight,
    /// Represents the light variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Light,
    /// Represents the normal variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Normal,
    /// Represents the medium variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Medium,
    /// Represents the semi bold variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SemiBold,
    /// Represents the bold variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Bold,
    /// Represents the extra bold variant.
    ///
    /// Visibility: inherited from the **public** enum.
    ExtraBold,
    /// Represents the black variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Black,
    /// Represents the numeric variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Numeric(u16),
}
impl Default for FontWeight {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::Normal
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available font stretch choices.
///
/// Visibility: **public**.
pub enum FontStretch {
    /// Represents the ultra condensed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    UltraCondensed,
    /// Represents the extra condensed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    ExtraCondensed,
    /// Represents the condensed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Condensed,
    /// Represents the semi condensed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SemiCondensed,
    #[default]
    /// Represents the normal variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Normal,
    /// Represents the semi expanded variant.
    ///
    /// Visibility: inherited from the **public** enum.
    SemiExpanded,
    /// Represents the expanded variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Expanded,
    /// Represents the extra expanded variant.
    ///
    /// Visibility: inherited from the **public** enum.
    ExtraExpanded,
    /// Represents the ultra expanded variant.
    ///
    /// Visibility: inherited from the **public** enum.
    UltraExpanded,
}

#[derive(Debug, Clone, PartialEq)]
/// Stores the state and data for font properties.
///
/// Visibility: **public**.
pub struct FontProperties {
    /// Stores family.
    ///
    /// Visibility: **public**.
    pub family: Vec<String>,
    pub size: Length,
    pub style: FontStyle,
    pub variant: FontVariant,
    pub weight: FontWeight,
    pub stretch: FontStretch,
    pub line_height: Option<Length>,
}
impl Default for FontProperties {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self {
            family: Vec::new(),
            size: Length::Px(16.0),
            style: FontStyle::Normal,
            variant: FontVariant::Normal,
            weight: FontWeight::Normal,
            stretch: FontStretch::Normal,
            line_height: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
/// Stores the state and data for font patch.
///
/// Visibility: **public**.
pub struct FontPatch {
    /// Stores family.
    ///
    /// Visibility: **public**.
    pub family: Option<Vec<String>>,
    pub size: Option<Length>,
    pub style: Option<FontStyle>,
    pub variant: Option<FontVariant>,
    pub weight: Option<FontWeight>,
    pub stretch: Option<FontStretch>,
    pub line_height: Option<Option<Length>>,
}
impl FontPatch {
    #[doc = "Implements the merge from operation. Visibility: crate-visible."]
    pub(crate) fn merge_from(&mut self, o: &Self) {
        if o.family.is_some() {
            self.family = o.family.clone();
        }
        if o.size.is_some() {
            self.size = o.size;
        }
        if o.style.is_some() {
            self.style = o.style;
        }
        if o.variant.is_some() {
            self.variant = o.variant;
        }
        if o.weight.is_some() {
            self.weight = o.weight;
        }
        if o.stretch.is_some() {
            self.stretch = o.stretch;
        }
        if o.line_height.is_some() {
            self.line_height = o.line_height;
        }
    }
}

/// Parses family from the supplied representation.
///
/// Visibility: **crate-visible**.
#[doc = "Parses family. Visibility: crate-visible."]
pub(crate) fn parse_family(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s|
            s
                .trim()
                .trim_matches(|c| (c == '\'' || c == '"'))
                .to_string()
        )
        .filter(|s| !s.is_empty())
        .collect()
}
/// Parses font style from the supplied representation.
///
/// Visibility: **crate-visible**.
#[doc = "Parses font style. Visibility: crate-visible."]
pub(crate) fn parse_font_style(v: &str) -> Option<FontStyle> {
    match v.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(FontStyle::Normal),
        "italic" => Some(FontStyle::Italic),
        "oblique" => Some(FontStyle::Oblique),
        _ => None,
    }
}
/// Parses variant from the supplied representation.
///
/// Visibility: **crate-visible**.
#[doc = "Parses variant. Visibility: crate-visible."]
pub(crate) fn parse_variant(v: &str) -> Option<FontVariant> {
    match v.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(FontVariant::Normal),
        "small-caps" => Some(FontVariant::SmallCaps),
        _ => None,
    }
}
/// Parses weight from the supplied representation.
///
/// Visibility: **crate-visible**.
#[doc = "Parses weight. Visibility: crate-visible."]
pub(crate) fn parse_weight(v: &str) -> Option<FontWeight> {
    match v.trim().to_ascii_lowercase().as_str() {
        "thin" | "100" => Some(FontWeight::Thin),
        "200" => Some(FontWeight::ExtraLight),
        "light" | "300" => Some(FontWeight::Light),
        "normal" | "400" => Some(FontWeight::Normal),
        "medium" | "500" => Some(FontWeight::Medium),
        "600" => Some(FontWeight::SemiBold),
        "bold" | "700" => Some(FontWeight::Bold),
        "800" => Some(FontWeight::ExtraBold),
        "900" => Some(FontWeight::Black),
        s => s.parse::<u16>().ok().map(FontWeight::Numeric),
    }
}
