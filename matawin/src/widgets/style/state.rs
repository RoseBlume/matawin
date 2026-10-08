//! Shared style state, conditional rules, and computed layout.

use std::{ sync::{ Arc, Mutex, atomic::{ AtomicBool, Ordering } }, time::{ Duration, Instant } };
use crate::widgets::{ Rect, WidgetId };
use super::{
    animation::{ self, AnimationIterationCount, AnimationSpec, Keyframes },
    box_model::{ BorderRadius, CornerRadius, Edges, ResolvedBorderRadius, ResolvedEdges },
    length::{ Axis, Length, LengthContext },
    media::MediaQuery,
    position::PositionMode,
    pseudo::{ InteractionState, NthChild, PseudoClass, PseudoSelector, TreeState },
    typography::{
        parse_family,
        parse_font_style,
        parse_variant,
        parse_weight,
        FontPatch,
        FontProperties,
        FontStretch,
        FontStyle,
        FontVariant,
        FontWeight,
    },
};

/// Converts a supported value into a [`Length`].
///
/// Visibility: **public**.
pub trait IntoLength {
    /// Converts this value into a length or returns a parse error.
    ///
    /// Visibility: **public through the public trait**.
    #[doc = "Implements the into length operation. Visibility: private/internal."]
    fn into_length(self) -> Result<Length, String>;
}
impl IntoLength for Length {
    #[doc = "Implements the into length operation. Visibility: private/internal."]
    fn into_length(self) -> Result<Length, String> {
        Ok(self)
    }
}
impl IntoLength for f32 {
    #[doc = "Implements the into length operation. Visibility: private/internal."]
    fn into_length(self) -> Result<Length, String> {
        Ok(Length::Px(self))
    }
}
impl IntoLength for i32 {
    #[doc = "Implements the into length operation. Visibility: private/internal."]
    fn into_length(self) -> Result<Length, String> {
        Ok(Length::Px(self as f32))
    }
}
impl IntoLength for &str {
    #[doc = "Implements the into length operation. Visibility: private/internal."]
    fn into_length(self) -> Result<Length, String> {
        Length::parse(self)
    }
}
impl IntoLength for String {
    #[doc = "Implements the into length operation. Visibility: private/internal."]
    fn into_length(self) -> Result<Length, String> {
        Length::parse(&self)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
/// Stores the state and data for style properties.
///
/// Visibility: **public**.
pub struct StyleProperties {
    /// Stores width.
    ///
    /// Visibility: **public**.
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_width: Option<Length>,
    pub max_height: Option<Length>,
    /// Stores position.
    ///
    /// Visibility: **public**.
    pub position: Option<PositionMode>,
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
    pub left: Option<Length>,
    pub relative_to: Option<Option<WidgetId>>,
    pub z_index: Option<i32>,
    /// Stores margin.
    ///
    /// Visibility: **public**.
    pub margin: Option<Edges<Length>>,
    pub padding: Option<Edges<Length>>,
    pub border_radius: Option<BorderRadius<Length>>,
    /// Stores font.
    ///
    /// Visibility: **public**.
    pub font: FontPatch,
    pub opacity: Option<f32>,
    /// Stores background color.
    ///
    /// Visibility: **public**.
    pub background_color: Option<u32>,
    pub foreground_color: Option<u32>,
    pub border_color: Option<u32>,
}
impl StyleProperties {
    /// Returns the value configured with width.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with width. Visibility: public."]
    pub fn with_width(mut self, v: Length) -> Self {
        self.width = Some(v);
        self
    }
    /// Returns the value configured with height.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with height. Visibility: public."]
    pub fn with_height(mut self, v: Length) -> Self {
        self.height = Some(v);
        self
    }
    /// Returns the value configured with min width.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with min width. Visibility: public."]
    pub fn with_min_width(mut self, v: Length) -> Self {
        self.min_width = Some(v);
        self
    }
    /// Returns the value configured with min height.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with min height. Visibility: public."]
    pub fn with_min_height(mut self, v: Length) -> Self {
        self.min_height = Some(v);
        self
    }
    /// Returns the value configured with max width.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with max width. Visibility: public."]
    pub fn with_max_width(mut self, v: Length) -> Self {
        self.max_width = Some(v);
        self
    }
    /// Returns the value configured with max height.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with max height. Visibility: public."]
    pub fn with_max_height(mut self, v: Length) -> Self {
        self.max_height = Some(v);
        self
    }
    /// Returns the value configured with position.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with position. Visibility: public."]
    pub fn with_position(mut self, v: PositionMode) -> Self {
        self.position = Some(v);
        self
    }
    /// Returns the value configured with left.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with left. Visibility: public."]
    pub fn with_left(mut self, v: Length) -> Self {
        self.left = Some(v);
        self
    }
    /// Returns the value configured with right.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with right. Visibility: public."]
    pub fn with_right(mut self, v: Length) -> Self {
        self.right = Some(v);
        self
    }
    /// Returns the value configured with top.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with top. Visibility: public."]
    pub fn with_top(mut self, v: Length) -> Self {
        self.top = Some(v);
        self
    }
    /// Returns the value configured with bottom.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with bottom. Visibility: public."]
    pub fn with_bottom(mut self, v: Length) -> Self {
        self.bottom = Some(v);
        self
    }
    /// Returns the value configured with relative to.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with relative to. Visibility: public."]
    pub fn with_relative_to(mut self, v: Option<WidgetId>) -> Self {
        self.relative_to = Some(v);
        self
    }
    /// Returns the value configured with z index.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with z index. Visibility: public."]
    pub fn with_z_index(mut self, v: i32) -> Self {
        self.z_index = Some(v);
        self
    }
    /// Returns the value configured with margin.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with margin. Visibility: public."]
    pub fn with_margin(mut self, v: Edges<Length>) -> Self {
        self.margin = Some(v);
        self
    }
    /// Returns the value configured with padding.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with padding. Visibility: public."]
    pub fn with_padding(mut self, v: Edges<Length>) -> Self {
        self.padding = Some(v);
        self
    }
    /// Returns the value configured with border radius.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with border radius. Visibility: public."]
    pub fn with_border_radius(mut self, v: BorderRadius<Length>) -> Self {
        self.border_radius = Some(v);
        self
    }
    /// Returns the value configured with font family.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with font family. Visibility: public."]
    pub fn with_font_family(mut self, v: Vec<String>) -> Self {
        self.font.family = Some(v);
        self
    }
    /// Returns the value configured with font size.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with font size. Visibility: public."]
    pub fn with_font_size(mut self, v: Length) -> Self {
        self.font.size = Some(v);
        self
    }
    /// Returns the value configured with font style.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with font style. Visibility: public."]
    pub fn with_font_style(mut self, v: FontStyle) -> Self {
        self.font.style = Some(v);
        self
    }
    /// Returns the value configured with font variant.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with font variant. Visibility: public."]
    pub fn with_font_variant(mut self, v: FontVariant) -> Self {
        self.font.variant = Some(v);
        self
    }
    /// Returns the value configured with font weight.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with font weight. Visibility: public."]
    pub fn with_font_weight(mut self, v: FontWeight) -> Self {
        self.font.weight = Some(v);
        self
    }
    /// Returns the value configured with font stretch.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with font stretch. Visibility: public."]
    pub fn with_font_stretch(mut self, v: FontStretch) -> Self {
        self.font.stretch = Some(v);
        self
    }
    /// Returns the value configured with line height.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with line height. Visibility: public."]
    pub fn with_line_height(mut self, v: Option<Length>) -> Self {
        self.font.line_height = Some(v);
        self
    }
    /// Returns the value configured with opacity.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with opacity. Visibility: public."]
    pub fn with_opacity(mut self, v: f32) -> Self {
        self.opacity = Some(v.clamp(0.0, 1.0));
        self
    }
    /// Returns the value configured with background color.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with background color. Visibility: public."]
    pub fn with_background_color(mut self, v: u32) -> Self {
        self.background_color = Some(v);
        self
    }
    /// Returns the value configured with foreground color.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with foreground color. Visibility: public."]
    pub fn with_foreground_color(mut self, v: u32) -> Self {
        self.foreground_color = Some(v);
        self
    }
    /// Returns the value configured with border color.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with border color. Visibility: public."]
    pub fn with_border_color(mut self, v: u32) -> Self {
        self.border_color = Some(v);
        self
    }
    /// Performs the merge from operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the merge from operation. Visibility: public."]
    pub fn merge_from(&mut self, o: &Self) {
        macro_rules! take {
            ($($f:ident),* $(,)?) => {$(if o.$f.is_some(){self.$f=o.$f;})*};
        }
        take!(
            width,
            height,
            min_width,
            min_height,
            max_width,
            max_height,
            position,
            top,
            right,
            bottom,
            left,
            relative_to,
            z_index,
            margin,
            padding,
            border_radius,
            opacity,
            background_color,
            foreground_color,
            border_color
        );
        self.font.merge_from(&o.font);
    }
    /// Sets property.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Sets property. Visibility: crate-visible."]
    pub(crate) fn set_property(&mut self, name: &str, value: &str) -> Result<(), String> {
        let n = name.trim().to_ascii_lowercase();
        match n.as_str() {
            "width" => {
                self.width = Some(Length::parse(value)?);
            }
            "height" => {
                self.height = Some(Length::parse(value)?);
            }
            "min-width" => {
                self.min_width = Some(Length::parse(value)?);
            }
            "min-height" => {
                self.min_height = Some(Length::parse(value)?);
            }
            "max-width" => {
                self.max_width = Some(Length::parse(value)?);
            }
            "max-height" => {
                self.max_height = Some(Length::parse(value)?);
            }
            "position" => {
                self.position = Some(match value.trim().to_ascii_lowercase().as_str() {
                    "static" => PositionMode::Static,
                    "relative" => PositionMode::Relative,
                    "absolute" => PositionMode::Absolute,
                    _ => {
                        return Err(format!("invalid position: {value}"));
                    }
                });
            }
            "top" => {
                self.top = Some(Length::parse(value)?);
            }
            "right" => {
                self.right = Some(Length::parse(value)?);
            }
            "bottom" => {
                self.bottom = Some(Length::parse(value)?);
            }
            "left" => {
                self.left = Some(Length::parse(value)?);
            }
            "z-index" => {
                self.z_index = Some(
                    value
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid z-index: {value}"))?
                );
            }
            "margin" => {
                self.margin = Some(Edges::parse(value)?);
            }
            "padding" => {
                self.padding = Some(Edges::parse(value)?);
            }
            "margin-top" => set_edge(&mut self.margin, 0, Length::parse(value)?),
            "margin-right" => set_edge(&mut self.margin, 1, Length::parse(value)?),
            "margin-bottom" => set_edge(&mut self.margin, 2, Length::parse(value)?),
            "margin-left" => set_edge(&mut self.margin, 3, Length::parse(value)?),
            "padding-top" => set_edge(&mut self.padding, 0, Length::parse(value)?),
            "padding-right" => set_edge(&mut self.padding, 1, Length::parse(value)?),
            "padding-bottom" => set_edge(&mut self.padding, 2, Length::parse(value)?),
            "padding-left" => set_edge(&mut self.padding, 3, Length::parse(value)?),
            "border-radius" => {
                self.border_radius = Some(BorderRadius::parse(value)?);
            }
            "border-top-left-radius" => set_corner(&mut self.border_radius, 0, value)?,
            "border-top-right-radius" => set_corner(&mut self.border_radius, 1, value)?,
            "border-bottom-right-radius" => set_corner(&mut self.border_radius, 2, value)?,
            "border-bottom-left-radius" => set_corner(&mut self.border_radius, 3, value)?,
            "font-family" => {
                self.font.family = Some(parse_family(value));
            }
            "font-size" => {
                self.font.size = Some(Length::parse(value)?);
            }
            "font-style" => {
                self.font.style = Some(
                    parse_font_style(value).ok_or_else(|| format!("invalid font-style: {value}"))?
                );
            }
            "font-variant" => {
                self.font.variant = Some(
                    parse_variant(value).ok_or_else(|| format!("invalid font-variant: {value}"))?
                );
            }
            "font-weight" => {
                self.font.weight = Some(
                    parse_weight(value).ok_or_else(|| format!("invalid font-weight: {value}"))?
                );
            }
            "font-stretch" => {
                self.font.stretch = Some(parse_stretch(value)?);
            }
            "line-height" => {
                self.font.line_height = Some(
                    if value.trim().eq_ignore_ascii_case("normal") {
                        None
                    } else {
                        Some(Length::parse(value)?)
                    }
                );
            }
            "opacity" => {
                self.opacity = Some(
                    value
                        .trim()
                        .parse::<f32>()
                        .map_err(|_| format!("invalid opacity: {value}"))?
                        .clamp(0.0, 1.0)
                );
            }
            "transparency" => {
                let t = value
                    .trim()
                    .trim_end_matches('%')
                    .parse::<f32>()
                    .map_err(|_| format!("invalid transparency: {value}"))?;
                self.opacity = Some(
                    if value.trim().ends_with('%') {
                        1.0 - (t / 100.0).clamp(0.0, 1.0)
                    } else {
                        1.0 - t.clamp(0.0, 1.0)
                    }
                );
            }
            "background" | "background-color" => {
                self.background_color = Some(parse_color(value)?);
            }
            "color" => {
                self.foreground_color = Some(parse_color(value)?);
            }
            "border-color" => {
                self.border_color = Some(parse_color(value)?);
            }
            _ => {
                return Err(format!("unsupported style property: {name}"));
            }
        }
        Ok(())
    }
    /// Performs the declarations operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the declarations operation. Visibility: crate-visible."]
    pub(crate) fn declarations(css: &str) -> Result<Self, String> {
        let mut patch = Self::default();
        for declaration in css.split(';') {
            let d = declaration.trim();
            if d.is_empty() {
                continue;
            }
            let Some((name, value)) = d.split_once(':') else {
                return Err(format!("invalid declaration: {d}"));
            };
            patch.set_property(name, value)?;
        }
        Ok(patch)
    }
    /// Performs the interpolate operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the interpolate operation. Visibility: crate-visible."]
    pub(crate) fn interpolate(&self, o: &Self, t: f32) -> Self {
        let mut r = self.clone();
        macro_rules! len {
            ($($f:ident),*) => {$(r.$f=interp_opt_len(self.$f,o.$f,t);)*};
        }
        len!(width, height, min_width, min_height, max_width, max_height, top, right, bottom, left);
        r.opacity = interp_opt_f32(self.opacity, o.opacity, t);
        r.z_index = if t >= 1.0 { o.z_index.or(self.z_index) } else { self.z_index.or(o.z_index) };
        r.margin = match (self.margin, o.margin) {
            (Some(a), Some(b)) => Some(a.interpolate(b, t)),
            (_, Some(b)) if t >= 1.0 => Some(b),
            (Some(a), _) => Some(a),
            _ => None,
        };
        r.padding = match (self.padding, o.padding) {
            (Some(a), Some(b)) => Some(a.interpolate(b, t)),
            (_, Some(b)) if t >= 1.0 => Some(b),
            (Some(a), _) => Some(a),
            _ => None,
        };
        r.border_radius = match (self.border_radius, o.border_radius) {
            (Some(a), Some(b)) => Some(a.interpolate(b, t)),
            (_, Some(b)) if t >= 1.0 => Some(b),
            (Some(a), _) => Some(a),
            _ => None,
        };
        r.background_color = interp_color(self.background_color, o.background_color, t);
        r.foreground_color = interp_color(self.foreground_color, o.foreground_color, t);
        r.border_color = interp_color(self.border_color, o.border_color, t);
        if let (Some(a), Some(b)) = (self.font.size, o.font.size) {
            r.font.size = Some(a.interpolate(b, t));
        } else if t >= 1.0 && o.font.size.is_some() {
            r.font.size = o.font.size;
        }
        if t >= 1.0 {
            r.font.merge_from(&o.font);
            if o.position.is_some() {
                r.position = o.position;
            }
            if o.relative_to.is_some() {
                r.relative_to = o.relative_to;
            }
        }
        r
    }
}

/// Backwards-compatible name for the typed widget property set.
pub type StylePatch = StyleProperties;

#[derive(Debug, Clone, Default, PartialEq)]
/// Stores the state and data for style rule.
///
/// Visibility: **public**.
pub struct StyleRule {
    /// Stores pseudo.
    ///
    /// Visibility: **public**.
    pub pseudo: PseudoSelector,
    /// Stores media.
    ///
    /// Visibility: **public**.
    pub media: Option<MediaQuery>,
    /// Stores style.
    ///
    /// Visibility: **public**.
    pub style: StyleProperties,
}
impl StyleRule {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(style: StyleProperties) -> Self {
        Self { style, ..Self::default() }
    }
    /// Performs the when pseudo operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the when pseudo operation. Visibility: public."]
    pub fn when_pseudo(mut self, pseudo: PseudoSelector) -> Self {
        self.pseudo = pseudo;
        self
    }
    /// Performs the when media operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the when media operation. Visibility: public."]
    pub fn when_media(mut self, media: MediaQuery) -> Self {
        self.media = Some(media);
        self
    }
    /// Performs the pseudo operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the pseudo operation. Visibility: crate-visible."]
    pub(crate) fn pseudo(selector: &str, declarations: &str) -> Result<Self, String> {
        Ok(
            Self::new(StyleProperties::declarations(declarations)?).when_pseudo(
                PseudoSelector::parse(selector)?
            )
        )
    }
    /// Performs the media operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the media operation. Visibility: crate-visible."]
    pub(crate) fn media(query: &str, declarations: &str) -> Result<Self, String> {
        Ok(
            Self::new(StyleProperties::declarations(declarations)?).when_media(
                MediaQuery::parse(query)?
            )
        )
    }
    /// Performs the conditional operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the conditional operation. Visibility: crate-visible."]
    pub(crate) fn conditional(
        selector: &str,
        query: &str,
        declarations: &str
    ) -> Result<Self, String> {
        Ok(
            Self::new(StyleProperties::declarations(declarations)?)
                .when_pseudo(PseudoSelector::parse(selector)?)
                .when_media(MediaQuery::parse(query)?)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores the state and data for style.
///
/// Visibility: **public**.
pub struct Style {
    /// Stores base.
    ///
    /// Visibility: **public**.
    pub base: StyleProperties,
    /// Stores rules.
    ///
    /// Visibility: **public**.
    pub rules: Vec<StyleRule>,
    /// Stores keyframes.
    ///
    /// Visibility: **public**.
    pub keyframes: Vec<Keyframes>,
    /// Stores animation.
    ///
    /// Visibility: **public**.
    pub animation: Option<AnimationSpec>,
    /// Stores root font size.
    ///
    /// Visibility: **public**.
    pub root_font_size: f32,
}
impl Default for Style {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self {
            base: StyleProperties::default(),
            rules: Vec::new(),
            keyframes: Vec::new(),
            animation: None,
            root_font_size: 16.0,
        }
    }
}
impl Style {
    /// Sets property.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Sets property. Visibility: crate-visible."]
    pub(crate) fn set_property(&mut self, name: &str, value: &str) -> Result<(), String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "animation-name" => {
                let a = self.animation.get_or_insert_with(AnimationSpec::default);
                a.name = value.trim().to_string();
                Ok(())
            }
            "animation-duration" => {
                let a = self.animation.get_or_insert_with(AnimationSpec::default);
                a.duration = animation::parse_duration(value)?;
                Ok(())
            }
            "animation-delay" => {
                let a = self.animation.get_or_insert_with(AnimationSpec::default);
                a.delay = animation::parse_duration(value)?;
                Ok(())
            }
            "animation-iteration-count" => {
                let a = self.animation.get_or_insert_with(AnimationSpec::default);
                a.iteration_count = AnimationIterationCount::parse(value)?;
                Ok(())
            }
            _ => self.base.set_property(name, value),
        }
    }
    /// Performs the declarations operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the declarations operation. Visibility: crate-visible."]
    pub(crate) fn declarations(&mut self, css: &str) -> Result<(), String> {
        for declaration in css.split(';') {
            let d = declaration.trim();
            if d.is_empty() {
                continue;
            }
            let Some((name, value)) = d.split_once(':') else {
                return Err(format!("invalid declaration: {d}"));
            };
            self.set_property(name, value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores the state and data for computed style.
///
/// Visibility: **public**.
pub struct ComputedStyle {
    /// Stores bounds.
    ///
    /// Visibility: **public**.
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub margin: ResolvedEdges,
    pub padding: ResolvedEdges,
    pub border_radius: ResolvedBorderRadius,
    pub z_index: i32,
    pub opacity: f32,
    pub font: FontProperties,
    pub font_size_px: f32,
    pub line_height_px: f32,
    pub background_color: Option<u32>,
    pub foreground_color: Option<u32>,
    pub border_color: Option<u32>,
}
impl Default for ComputedStyle {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        let r = Rect::default();
        Self {
            bounds: r,
            content_bounds: r,
            margin: ResolvedEdges::default(),
            padding: ResolvedEdges::default(),
            border_radius: ResolvedBorderRadius::default(),
            z_index: 0,
            opacity: 1.0,
            font: FontProperties::default(),
            font_size_px: 16.0,
            line_height_px: 19.2,
            background_color: None,
            foreground_color: None,
            border_color: None,
        }
    }
}

#[derive(Debug)]
/// Stores the state and data for style runtime.
///
/// Visibility: **private**.
struct StyleRuntime {
    /// Stores style.
    ///
    /// Visibility: **private**.
    style: Style,
    /// Stores interaction.
    ///
    /// Visibility: **private**.
    interaction: InteractionState,
    /// Stores tree.
    ///
    /// Visibility: **private**.
    tree: TreeState,
    /// Stores started.
    ///
    /// Visibility: **private**.
    started: Instant,
}
#[derive(Debug, Clone)]
/// Stores the state and data for style handle.
///
/// Visibility: **public**.
pub struct StyleHandle {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: Arc<Mutex<StyleRuntime>>,
    /// Stores dirty.
    ///
    /// Visibility: **private**.
    dirty: Option<Arc<AtomicBool>>,
}
impl Default for StyleHandle {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl StyleHandle {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(
                Mutex::new(StyleRuntime {
                    style: Style::default(),
                    interaction: InteractionState::default(),
                    tree: TreeState::default(),
                    started: Instant::now(),
                })
            ),
            dirty: None,
        }
    }
    /// Returns the value configured with invalidator.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Provides access configured with invalidator. Visibility: crate-visible."]
    pub(crate) fn with_invalidator(dirty: Arc<AtomicBool>) -> Self {
        Self {
            inner: Arc::new(
                Mutex::new(StyleRuntime {
                    style: Style::default(),
                    interaction: InteractionState::default(),
                    tree: TreeState::default(),
                    started: Instant::now(),
                })
            ),
            dirty: Some(dirty),
        }
    }
    #[inline]
    #[doc = "Implements the invalidate operation. Visibility: private/internal."]
    fn invalidate(&self) {
        if let Some(dirty) = &self.dirty {
            dirty.store(true, Ordering::Release)
        }
    }
    /// Performs the style operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the style operation. Visibility: public."]
    pub fn style(&self) -> Style {
        self.inner.lock().unwrap().style.clone()
    }
    /// Performs the properties operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the properties operation. Visibility: public."]
    pub fn properties(&self) -> StyleProperties {
        self.inner.lock().unwrap().style.base.clone()
    }
    /// Sets style.
    ///
    /// Visibility: **public**.
    #[doc = "Sets style. Visibility: public."]
    pub fn set_style(&self, style: Style) {
        let mut s = self.inner.lock().unwrap();
        s.style = style;
        s.started = Instant::now();
        drop(s);
        self.invalidate()
    }
    /// Sets properties.
    ///
    /// Visibility: **public**.
    #[doc = "Sets properties. Visibility: public."]
    pub fn set_properties(&self, properties: StyleProperties) {
        self.update(|s| {
            s.base = properties;
        })
    }
    /// Performs the update operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the update operation. Visibility: public."]
    pub fn update(&self, f: impl FnOnce(&mut Style)) {
        let mut s = self.inner.lock().unwrap();
        f(&mut s.style);
        drop(s);
        self.invalidate()
    }
    /// Updates properties.
    ///
    /// Visibility: **public**.
    #[doc = "Updates properties. Visibility: public."]
    pub fn update_properties(&self, f: impl FnOnce(&mut StyleProperties)) {
        self.update(|s| f(&mut s.base))
    }
    /// Sets property.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Sets property. Visibility: crate-visible."]
    pub(crate) fn set_property(&self, name: &str, value: &str) -> Result<(), String> {
        let mut s = self.inner.lock().unwrap();
        s.style.set_property(name, value)?;
        if name.starts_with("animation-") {
            s.started = Instant::now();
        }
        drop(s);
        self.invalidate();
        Ok(())
    }
    /// Performs the declarations operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the declarations operation. Visibility: crate-visible."]
    pub(crate) fn declarations(&self, css: &str) -> Result<(), String> {
        let mut s = self.inner.lock().unwrap();
        s.style.declarations(css)?;
        s.started = Instant::now();
        drop(s);
        self.invalidate();
        Ok(())
    }
    /// Sets interaction.
    ///
    /// Visibility: **public**.
    #[doc = "Sets interaction. Visibility: public."]
    pub fn set_interaction(&self, state: InteractionState) {
        let mut s = self.inner.lock().unwrap();
        if s.interaction != state {
            s.interaction = state;
            drop(s);
            self.invalidate()
        }
    }
    /// Performs the interaction operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the interaction operation. Visibility: public."]
    pub fn interaction(&self) -> InteractionState {
        self.inner.lock().unwrap().interaction
    }
    /// Sets tree state.
    ///
    /// Visibility: **public**.
    #[doc = "Sets tree state. Visibility: public."]
    pub fn set_tree_state(&self, state: TreeState) {
        let mut s = self.inner.lock().unwrap();
        if s.tree != state {
            s.tree = state;
            drop(s);
            self.invalidate()
        }
    }
    /// Performs the tree state operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the tree state operation. Visibility: public."]
    pub fn tree_state(&self) -> TreeState {
        self.inner.lock().unwrap().tree.clone()
    }
    /// Sets lang.
    ///
    /// Visibility: **public**.
    #[doc = "Sets lang. Visibility: public."]
    pub fn set_lang(&self, lang: impl Into<String>) {
        let lang = Some(lang.into());
        let mut s = self.inner.lock().unwrap();
        if s.tree.lang != lang {
            s.tree.lang = lang;
            drop(s);
            self.invalidate()
        }
    }
    /// Performs the restart animation operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the restart animation operation. Visibility: public."]
    pub fn restart_animation(&self) {
        self.inner.lock().unwrap().started = Instant::now();
        self.invalidate()
    }
    /// Performs the relative to operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the relative to operation. Visibility: public."]
    pub fn relative_to(&self) -> Option<WidgetId> {
        self.inner.lock().unwrap().style.base.relative_to.flatten()
    }
    /// Performs the animation running operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the animation running operation. Visibility: public."]
    pub fn animation_running(&self) -> bool {
        let s = self.inner.lock().unwrap();
        let Some(spec) = s.style.animation.as_ref() else {
            return false;
        };
        if !s.style.keyframes.iter().any(|k| k.name == spec.name && !k.frames.is_empty()) {
            return false;
        }
        animation::is_running(spec, s.started.elapsed())
    }
    /// Performs the computed operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the computed operation. Visibility: public."]
    pub fn computed(
        &self,
        base: Rect,
        viewport: (u32, u32),
        reference: Option<Rect>
    ) -> ComputedStyle {
        let s = self.inner.lock().unwrap();
        compute_style(
            &s.style,
            s.interaction,
            &s.tree,
            s.started.elapsed(),
            base,
            viewport,
            reference
        )
    }
}

/// Defines the behavior required for stylable.
///
/// Visibility: **public**.
pub trait Stylable {
    /// Performs the style handle operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the style handle operation. Visibility: private/internal."]
    fn style_handle(&self) -> StyleHandle;
    /// Performs the style operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the style operation. Visibility: private/internal."]
    fn style(&self) -> Style {
        self.style_handle().style()
    }
    /// Performs the style properties operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the style properties operation. Visibility: private/internal."]
    fn style_properties(&self) -> StyleProperties {
        self.style_handle().properties()
    }
    /// Sets style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets style. Visibility: private/internal."]
    fn set_style(&self, style: Style) {
        self.style_handle().set_style(style)
    }
    /// Sets style properties.
    ///
    /// Visibility: **private**.
    #[doc = "Sets style properties. Visibility: private/internal."]
    fn set_style_properties(&self, properties: StyleProperties) {
        self.style_handle().set_properties(properties)
    }
    /// Updates style.
    ///
    /// Visibility: **private**.
    #[doc = "Updates style. Visibility: private/internal."]
    fn update_style(&self, f: impl FnOnce(&mut StyleProperties)) {
        self.style_handle().update_properties(f)
    }

    /// Sets width.
    ///
    /// Visibility: **private**.
    #[doc = "Sets width. Visibility: private/internal."]
    fn set_width(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.width = Some(v);
        });
        Ok(())
    }
    /// Sets height.
    ///
    /// Visibility: **private**.
    #[doc = "Sets height. Visibility: private/internal."]
    fn set_height(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.height = Some(v);
        });
        Ok(())
    }
    /// Sets min width.
    ///
    /// Visibility: **private**.
    #[doc = "Sets min width. Visibility: private/internal."]
    fn set_min_width(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.min_width = Some(v);
        });
        Ok(())
    }
    /// Sets min height.
    ///
    /// Visibility: **private**.
    #[doc = "Sets min height. Visibility: private/internal."]
    fn set_min_height(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.min_height = Some(v);
        });
        Ok(())
    }
    /// Sets max width.
    ///
    /// Visibility: **private**.
    #[doc = "Sets max width. Visibility: private/internal."]
    fn set_max_width(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.max_width = Some(v);
        });
        Ok(())
    }
    /// Sets max height.
    ///
    /// Visibility: **private**.
    #[doc = "Sets max height. Visibility: private/internal."]
    fn set_max_height(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.max_height = Some(v);
        });
        Ok(())
    }
    /// Sets position mode.
    ///
    /// Visibility: **private**.
    #[doc = "Sets position mode. Visibility: private/internal."]
    fn set_position_mode(&self, v: PositionMode) {
        self.update_style(|s| {
            s.position = Some(v);
        })
    }
    /// Sets left.
    ///
    /// Visibility: **private**.
    #[doc = "Sets left. Visibility: private/internal."]
    fn set_left(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.left = Some(v);
        });
        Ok(())
    }
    /// Sets right.
    ///
    /// Visibility: **private**.
    #[doc = "Sets right. Visibility: private/internal."]
    fn set_right(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.right = Some(v);
        });
        Ok(())
    }
    /// Sets top.
    ///
    /// Visibility: **private**.
    #[doc = "Sets top. Visibility: private/internal."]
    fn set_top(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.top = Some(v);
        });
        Ok(())
    }
    /// Sets bottom.
    ///
    /// Visibility: **private**.
    #[doc = "Sets bottom. Visibility: private/internal."]
    fn set_bottom(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.bottom = Some(v);
        });
        Ok(())
    }
    /// Sets relative to.
    ///
    /// Visibility: **private**.
    #[doc = "Sets relative to. Visibility: private/internal."]
    fn set_relative_to(&self, id: Option<WidgetId>) {
        self.update_style(|s| {
            s.relative_to = Some(id);
        })
    }
    /// Sets relative to widget.
    ///
    /// Visibility: **private**.
    #[doc = "Sets relative to widget. Visibility: private/internal."]
    fn set_relative_to_widget<W: crate::widgets::catalog::WidgetLike>(&self, other: &W) {
        self.set_relative_to(Some(other.widget_id()))
    }
    /// Sets z index.
    ///
    /// Visibility: **private**.
    #[doc = "Sets z index. Visibility: private/internal."]
    fn set_z_index(&self, z: i32) {
        self.update_style(|s| {
            s.z_index = Some(z);
        })
    }

    /// Sets margin.
    ///
    /// Visibility: **private**.
    #[doc = "Sets margin. Visibility: private/internal."]
    fn set_margin(&self, v: Edges<Length>) {
        self.update_style(|s| {
            s.margin = Some(v);
        })
    }
    /// Sets margin top.
    ///
    /// Visibility: **private**.
    #[doc = "Sets margin top. Visibility: private/internal."]
    fn set_margin_top(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.margin, 0, v));
        Ok(())
    }
    /// Sets margin right.
    ///
    /// Visibility: **private**.
    #[doc = "Sets margin right. Visibility: private/internal."]
    fn set_margin_right(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.margin, 1, v));
        Ok(())
    }
    /// Sets margin bottom.
    ///
    /// Visibility: **private**.
    #[doc = "Sets margin bottom. Visibility: private/internal."]
    fn set_margin_bottom(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.margin, 2, v));
        Ok(())
    }
    /// Sets margin left.
    ///
    /// Visibility: **private**.
    #[doc = "Sets margin left. Visibility: private/internal."]
    fn set_margin_left(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.margin, 3, v));
        Ok(())
    }
    /// Sets padding.
    ///
    /// Visibility: **private**.
    #[doc = "Sets padding. Visibility: private/internal."]
    fn set_padding(&self, v: Edges<Length>) {
        self.update_style(|s| {
            s.padding = Some(v);
        })
    }
    /// Sets padding top.
    ///
    /// Visibility: **private**.
    #[doc = "Sets padding top. Visibility: private/internal."]
    fn set_padding_top(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.padding, 0, v));
        Ok(())
    }
    /// Sets padding right.
    ///
    /// Visibility: **private**.
    #[doc = "Sets padding right. Visibility: private/internal."]
    fn set_padding_right(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.padding, 1, v));
        Ok(())
    }
    /// Sets padding bottom.
    ///
    /// Visibility: **private**.
    #[doc = "Sets padding bottom. Visibility: private/internal."]
    fn set_padding_bottom(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.padding, 2, v));
        Ok(())
    }
    /// Sets padding left.
    ///
    /// Visibility: **private**.
    #[doc = "Sets padding left. Visibility: private/internal."]
    fn set_padding_left(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| set_edge(&mut s.padding, 3, v));
        Ok(())
    }

    /// Sets border radius.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border radius. Visibility: private/internal."]
    fn set_border_radius(&self, v: BorderRadius<Length>) {
        self.update_style(|s| {
            s.border_radius = Some(v);
        })
    }
    /// Sets border radius all.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border radius all. Visibility: private/internal."]
    fn set_border_radius_all(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.set_border_radius(BorderRadius::all(v));
        Ok(())
    }
    /// Sets border top left radius.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border top left radius. Visibility: private/internal."]
    fn set_border_top_left_radius(&self, v: CornerRadius<Length>) {
        self.update_style(|s| {
            let r = s.border_radius.get_or_insert_with(BorderRadius::default);
            r.top_left = v;
        })
    }
    /// Sets border top right radius.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border top right radius. Visibility: private/internal."]
    fn set_border_top_right_radius(&self, v: CornerRadius<Length>) {
        self.update_style(|s| {
            let r = s.border_radius.get_or_insert_with(BorderRadius::default);
            r.top_right = v;
        })
    }
    /// Sets border bottom right radius.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border bottom right radius. Visibility: private/internal."]
    fn set_border_bottom_right_radius(&self, v: CornerRadius<Length>) {
        self.update_style(|s| {
            let r = s.border_radius.get_or_insert_with(BorderRadius::default);
            r.bottom_right = v;
        })
    }
    /// Sets border bottom left radius.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border bottom left radius. Visibility: private/internal."]
    fn set_border_bottom_left_radius(&self, v: CornerRadius<Length>) {
        self.update_style(|s| {
            let r = s.border_radius.get_or_insert_with(BorderRadius::default);
            r.bottom_left = v;
        })
    }

    /// Sets font family.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font family. Visibility: private/internal."]
    fn set_font_family(&self, f: impl Into<String>) {
        let f = f.into();
        self.update_style(|s| {
            s.font.family = Some(parse_family(&f));
        })
    }
    /// Sets font families.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font families. Visibility: private/internal."]
    fn set_font_families(&self, families: Vec<String>) {
        self.update_style(|s| {
            s.font.family = Some(families);
        })
    }
    /// Sets font size.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font size. Visibility: private/internal."]
    fn set_font_size(&self, v: impl IntoLength) -> Result<(), String> {
        let v = v.into_length()?;
        self.update_style(|s| {
            s.font.size = Some(v);
        });
        Ok(())
    }
    /// Sets font style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font style. Visibility: private/internal."]
    fn set_font_style(&self, v: FontStyle) {
        self.update_style(|s| {
            s.font.style = Some(v);
        })
    }
    /// Sets font variant.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font variant. Visibility: private/internal."]
    fn set_font_variant(&self, v: FontVariant) {
        self.update_style(|s| {
            s.font.variant = Some(v);
        })
    }
    /// Sets font weight.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font weight. Visibility: private/internal."]
    fn set_font_weight(&self, v: FontWeight) {
        self.update_style(|s| {
            s.font.weight = Some(v);
        })
    }
    /// Sets font stretch.
    ///
    /// Visibility: **private**.
    #[doc = "Sets font stretch. Visibility: private/internal."]
    fn set_font_stretch(&self, v: FontStretch) {
        self.update_style(|s| {
            s.font.stretch = Some(v);
        })
    }
    /// Sets line height.
    ///
    /// Visibility: **private**.
    #[doc = "Sets line height. Visibility: private/internal."]
    fn set_line_height(&self, v: Option<Length>) {
        self.update_style(|s| {
            s.font.line_height = Some(v);
        })
    }

    /// Sets opacity.
    ///
    /// Visibility: **private**.
    #[doc = "Sets opacity. Visibility: private/internal."]
    fn set_opacity(&self, v: f32) {
        self.update_style(|s| {
            s.opacity = Some(v.clamp(0.0, 1.0));
        })
    }
    /// Sets transparency.
    ///
    /// Visibility: **private**.
    #[doc = "Sets transparency. Visibility: private/internal."]
    fn set_transparency(&self, v: f32) {
        self.set_opacity(1.0 - v.clamp(0.0, 1.0))
    }
    /// Sets background color.
    ///
    /// Visibility: **private**.
    #[doc = "Sets background color. Visibility: private/internal."]
    fn set_background_color(&self, v: u32) {
        self.update_style(|s| {
            s.background_color = Some(v);
        })
    }
    /// Sets foreground color.
    ///
    /// Visibility: **private**.
    #[doc = "Sets foreground color. Visibility: private/internal."]
    fn set_foreground_color(&self, v: u32) {
        self.update_style(|s| {
            s.foreground_color = Some(v);
        })
    }
    /// Sets border color.
    ///
    /// Visibility: **private**.
    #[doc = "Sets border color. Visibility: private/internal."]
    fn set_border_color(&self, v: u32) {
        self.update_style(|s| {
            s.border_color = Some(v);
        })
    }

    /// Adds style rule.
    ///
    /// Visibility: **private**.
    #[doc = "Adds style rule. Visibility: private/internal."]
    fn add_style_rule(&self, rule: StyleRule) {
        self.style_handle().update(|s| s.rules.push(rule))
    }
    /// Sets pseudo style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets pseudo style. Visibility: private/internal."]
    fn set_pseudo_style(&self, pseudo: PseudoSelector, properties: StyleProperties) {
        let h = self.style_handle();
        h.update(|s| {
            if
                let Some(rule) = s.rules
                    .iter_mut()
                    .find(|r| r.media.is_none() && r.pseudo == pseudo)
            {
                rule.style = properties;
            } else {
                s.rules.push(StyleRule::new(properties).when_pseudo(pseudo))
            }
        })
    }
    /// Sets hover style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets hover style. Visibility: private/internal."]
    fn set_hover_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::Hover]), properties)
    }
    /// Sets focus style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets focus style. Visibility: private/internal."]
    fn set_focus_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::Focus]), properties)
    }
    /// Sets active style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets active style. Visibility: private/internal."]
    fn set_active_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::Active]), properties)
    }
    /// Sets link style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets link style. Visibility: private/internal."]
    fn set_link_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::Link]), properties)
    }
    /// Sets visited style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets visited style. Visibility: private/internal."]
    fn set_visited_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::Visited]), properties)
    }
    /// Sets first child style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets first child style. Visibility: private/internal."]
    fn set_first_child_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::FirstChild]), properties)
    }
    /// Sets last child style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets last child style. Visibility: private/internal."]
    fn set_last_child_style(&self, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::LastChild]), properties)
    }
    /// Sets nth child style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets nth child style. Visibility: private/internal."]
    fn set_nth_child_style(&self, n: NthChild, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::NthChild(n)]), properties)
    }
    /// Sets lang style.
    ///
    /// Visibility: **private**.
    #[doc = "Sets lang style. Visibility: private/internal."]
    fn set_lang_style(&self, lang: impl Into<String>, properties: StyleProperties) {
        self.set_pseudo_style(PseudoSelector::new([PseudoClass::Lang(lang.into())]), properties)
    }
    /// Adds media style.
    ///
    /// Visibility: **private**.
    #[doc = "Adds media style. Visibility: private/internal."]
    fn add_media_style(&self, query: MediaQuery, properties: StyleProperties) {
        self.add_style_rule(StyleRule::new(properties).when_media(query))
    }
    /// Adds conditional style.
    ///
    /// Visibility: **private**.
    #[doc = "Adds conditional style. Visibility: private/internal."]
    fn add_conditional_style(
        &self,
        pseudo: PseudoSelector,
        query: MediaQuery,
        properties: StyleProperties
    ) {
        self.add_style_rule(StyleRule::new(properties).when_pseudo(pseudo).when_media(query))
    }

    /// Adds keyframes.
    ///
    /// Visibility: **private**.
    #[doc = "Adds keyframes. Visibility: private/internal."]
    fn add_keyframes(&self, k: Keyframes) {
        self.style_handle().update(|s| {
            s.keyframes.retain(|x| x.name != k.name);
            s.keyframes.push(k)
        })
    }
    /// Sets animation.
    ///
    /// Visibility: **private**.
    #[doc = "Sets animation. Visibility: private/internal."]
    fn set_animation(&self, a: AnimationSpec) {
        let h = self.style_handle();
        h.update(|s| {
            s.animation = Some(a);
        });
        h.restart_animation()
    }
    /// Clears animation.
    ///
    /// Visibility: **private**.
    #[doc = "Clears animation. Visibility: private/internal."]
    fn clear_animation(&self) {
        self.style_handle().update(|s| {
            s.animation = None;
        })
    }
    /// Sets lang.
    ///
    /// Visibility: **private**.
    #[doc = "Sets lang. Visibility: private/internal."]
    fn set_lang(&self, lang: impl Into<String>) {
        self.style_handle().set_lang(lang)
    }
    /// Sets hovered.
    ///
    /// Visibility: **private**.
    #[doc = "Sets hovered. Visibility: private/internal."]
    fn set_hovered(&self, value: bool) {
        let h = self.style_handle();
        let mut i = h.interaction();
        i.hover = value;
        h.set_interaction(i)
    }
    /// Sets focused.
    ///
    /// Visibility: **private**.
    #[doc = "Sets focused. Visibility: private/internal."]
    fn set_focused(&self, value: bool) {
        let h = self.style_handle();
        let mut i = h.interaction();
        i.focus = value;
        h.set_interaction(i)
    }
    /// Sets active.
    ///
    /// Visibility: **private**.
    #[doc = "Sets active. Visibility: private/internal."]
    fn set_active(&self, value: bool) {
        let h = self.style_handle();
        let mut i = h.interaction();
        i.active = value;
        h.set_interaction(i)
    }
    /// Sets link state.
    ///
    /// Visibility: **private**.
    #[doc = "Sets link state. Visibility: private/internal."]
    fn set_link_state(&self, is_link: bool, visited: bool) {
        let h = self.style_handle();
        let mut i = h.interaction();
        i.link = is_link;
        i.visited = visited;
        h.set_interaction(i)
    }
    /// Sets visited.
    ///
    /// Visibility: **private**.
    #[doc = "Sets visited. Visibility: private/internal."]
    fn set_visited(&self, value: bool) {
        let h = self.style_handle();
        let mut i = h.interaction();
        i.visited = value;
        h.set_interaction(i)
    }
}

/// Implements compute style.
///
/// Visibility: **private**.
#[doc = "Implements the compute style operation. Visibility: private/internal."]
fn compute_style(
    style: &Style,
    interaction: InteractionState,
    tree: &TreeState,
    elapsed: Duration,
    base: Rect,
    viewport: (u32, u32),
    reference: Option<Rect>
) -> ComputedStyle {
    let mut p = style.base.clone();
    for r in &style.rules {
        if
            r.pseudo.matches(interaction, tree) &&
            r.media.as_ref().is_none_or(|m| m.matches(viewport, style.root_font_size))
        {
            p.merge_from(&r.style);
        }
    }
    if let Some(spec) = &style.animation {
        if let Some(k) = style.keyframes.iter().find(|k| k.name == spec.name) {
            if let Some(sample) = animation::sample(spec, k, elapsed) {
                p.merge_from(&sample);
            }
        }
    }
    let containing = reference.unwrap_or(Rect::new(0, 0, viewport.0, viewport.1));
    let base_ctx = LengthContext::new(
        viewport,
        (containing.width, containing.height),
        style.root_font_size,
        style.root_font_size
    );
    let font_size = p.font.size
        .map(|v| resolve_font_size(v, base_ctx, style.root_font_size))
        .unwrap_or(style.root_font_size)
        .max(1.0);
    let ctx = LengthContext::new(
        viewport,
        (containing.width, containing.height),
        font_size,
        style.root_font_size
    );
    let margin = p.margin.unwrap_or_default().resolve(ctx);
    let padding = p.padding.unwrap_or_default().resolve(ctx);
    let mut w = p.width.and_then(|v| v.resolve(Axis::Horizontal, ctx)).unwrap_or(base.width as f32);
    let mut h = p.height.and_then(|v| v.resolve(Axis::Vertical, ctx)).unwrap_or(base.height as f32);
    if let Some(v) = p.min_width.and_then(|v| v.resolve(Axis::Horizontal, ctx)) {
        w = w.max(v);
    }
    if let Some(v) = p.max_width.and_then(|v| v.resolve(Axis::Horizontal, ctx)) {
        w = w.min(v);
    }
    if let Some(v) = p.min_height.and_then(|v| v.resolve(Axis::Vertical, ctx)) {
        h = h.max(v);
    }
    if let Some(v) = p.max_height.and_then(|v| v.resolve(Axis::Vertical, ctx)) {
        h = h.min(v);
    }
    let mode = p.position.unwrap_or(PositionMode::Static);
    let left = p.left.and_then(|v| v.resolve(Axis::Horizontal, ctx));
    let right = p.right.and_then(|v| v.resolve(Axis::Horizontal, ctx));
    let top = p.top.and_then(|v| v.resolve(Axis::Vertical, ctx));
    let bottom = p.bottom.and_then(|v| v.resolve(Axis::Vertical, ctx));
    let (mut x, mut y) = match mode {
        PositionMode::Static => (base.x as f32, base.y as f32),
        PositionMode::Relative =>
            (
                (if reference.is_some() { containing.x as f32 } else { base.x as f32 }) +
                    left.unwrap_or(0.0) -
                    right.unwrap_or(0.0),
                (if reference.is_some() { containing.y as f32 } else { base.y as f32 }) +
                    top.unwrap_or(0.0) -
                    bottom.unwrap_or(0.0),
            ),
        PositionMode::Absolute => {
            let x = if let Some(l) = left {
                (containing.x as f32) + l
            } else if let Some(r) = right {
                (containing.x as f32) + (containing.width as f32) - r - w
            } else {
                base.x as f32
            };
            let y = if let Some(t) = top {
                (containing.y as f32) + t
            } else if let Some(b) = bottom {
                (containing.y as f32) + (containing.height as f32) - b - h
            } else {
                base.y as f32
            };
            (x, y)
        }
    };
    x += margin.left;
    y += margin.top;
    if p.width.is_none() {
        w = (w - margin.left - margin.right).max(0.0);
    }
    if p.height.is_none() {
        h = (h - margin.top - margin.bottom).max(0.0);
    }
    let bounds = Rect::new(round_i32(x), round_i32(y), round_u32(w), round_u32(h));
    let cx = x + padding.left;
    let cy = y + padding.top;
    let cw = (w - padding.left - padding.right).max(0.0);
    let ch = (h - padding.top - padding.bottom).max(0.0);
    let content_bounds = Rect::new(round_i32(cx), round_i32(cy), round_u32(cw), round_u32(ch));
    let radius = p.border_radius
        .unwrap_or_default()
        .resolve(
            LengthContext::new(
                viewport,
                (bounds.width, bounds.height),
                font_size,
                style.root_font_size
            )
        );
    let mut font = FontProperties::default();
    if let Some(v) = p.font.family.clone() {
        font.family = v;
    }
    if let Some(v) = p.font.size {
        font.size = v;
    }
    if let Some(v) = p.font.style {
        font.style = v;
    }
    if let Some(v) = p.font.variant {
        font.variant = v;
    }
    if let Some(v) = p.font.weight {
        font.weight = v;
    }
    if let Some(v) = p.font.stretch {
        font.stretch = v;
    }
    if let Some(v) = p.font.line_height {
        font.line_height = v;
    }
    let line_height_px = p.font.line_height
        .flatten()
        .and_then(|v|
            v.resolve(
                Axis::Vertical,
                LengthContext::new(
                    viewport,
                    (font_size.round().max(1.0) as u32, font_size.round().max(1.0) as u32),
                    font_size,
                    style.root_font_size
                )
            )
        )
        .unwrap_or(font_size * 1.2)
        .max(1.0);
    ComputedStyle {
        bounds,
        content_bounds,
        margin,
        padding,
        border_radius: radius,
        z_index: p.z_index.unwrap_or(0),
        opacity: p.opacity.unwrap_or(1.0).clamp(0.0, 1.0),
        font,
        font_size_px: font_size,
        line_height_px,
        background_color: p.background_color,
        foreground_color: p.foreground_color,
        border_color: p.border_color,
    }
}

/// Implements round i32.
///
/// Visibility: **private**.
#[doc = "Implements the round i32 operation. Visibility: private/internal."]
fn round_i32(v: f32) -> i32 {
    v.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}
#[doc = "Implements the round u32 operation. Visibility: private/internal."]
fn round_u32(v: f32) -> u32 {
    v.round().clamp(0.0, u32::MAX as f32) as u32
}
/// Implements interp opt len.
///
/// Visibility: **private**.
#[doc = "Implements the interp opt len operation. Visibility: private/internal."]
fn interp_opt_len(a: Option<Length>, b: Option<Length>, t: f32) -> Option<Length> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.interpolate(b, t)),
        (_, Some(b)) if t >= 1.0 => Some(b),
        (Some(a), _) => Some(a),
        _ => None,
    }
}
/// Implements interp opt f32.
///
/// Visibility: **private**.
#[doc = "Implements the interp opt f32 operation. Visibility: private/internal."]
fn interp_opt_f32(a: Option<f32>, b: Option<f32>, t: f32) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * t),
        (_, Some(b)) if t >= 1.0 => Some(b),
        (Some(a), _) => Some(a),
        _ => None,
    }
}
/// Implements interp color.
///
/// Visibility: **private**.
#[doc = "Implements the interp color operation. Visibility: private/internal."]
fn interp_color(a: Option<u32>, b: Option<u32>, t: f32) -> Option<u32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(lerp_color(a, b, t)),
        (_, Some(b)) if t >= 1.0 => Some(b),
        (Some(a), _) => Some(a),
        _ => None,
    }
}
/// Implements lerp color.
///
/// Visibility: **private**.
#[doc = "Implements the lerp color operation. Visibility: private/internal."]
fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let mut out = 0u32;
    for shift in [0, 8, 16, 24] {
        let x = ((a >> shift) & 255) as f32;
        let y = ((b >> shift) & 255) as f32;
        out |= ((x + (y - x) * t).round().clamp(0.0, 255.0) as u32) << shift;
    }
    out
}
/// Sets edge.
///
/// Visibility: **private**.
#[doc = "Sets edge. Visibility: private/internal."]
fn set_edge(slot: &mut Option<Edges<Length>>, which: u8, value: Length) {
    let e = slot.get_or_insert_with(Edges::default);
    match which {
        0 => {
            e.top = value;
        }
        1 => {
            e.right = value;
        }
        2 => {
            e.bottom = value;
        }
        _ => {
            e.left = value;
        }
    }
}
/// Sets corner.
///
/// Visibility: **private**.
#[doc = "Sets corner. Visibility: private/internal."]
fn set_corner(
    slot: &mut Option<BorderRadius<Length>>,
    which: u8,
    value: &str
) -> Result<(), String> {
    let vals: Vec<_> = value.split_whitespace().collect();
    if vals.is_empty() || vals.len() > 2 {
        return Err(format!("invalid corner radius: {value}"));
    }
    let x = Length::parse(vals[0])?;
    let y = if vals.len() == 2 { Length::parse(vals[1])? } else { x };
    let r = slot.get_or_insert_with(BorderRadius::default);
    let c = CornerRadius { x, y };
    match which {
        0 => {
            r.top_left = c;
        }
        1 => {
            r.top_right = c;
        }
        2 => {
            r.bottom_right = c;
        }
        _ => {
            r.bottom_left = c;
        }
    }
    Ok(())
}
/// Parses color from the supplied representation.
///
/// Visibility: **private**.
#[doc = "Parses color. Visibility: private/internal."]
fn parse_color(v: &str) -> Result<u32, String> {
    let s = v.trim().to_ascii_lowercase();
    if s == "transparent" {
        return Ok(0);
    }
    let Some(hex) = s.strip_prefix('#') else {
        return Err(format!("only #rgb/#rrggbb/#aarrggbb/transparent colors are supported: {v}"));
    };
    let parse = |s: &str| u32::from_str_radix(s, 16).map_err(|_| format!("invalid color: {v}"));
    match hex.len() {
        3 => {
            let mut rgb = 0u32;
            for c in hex.chars() {
                let n = c.to_digit(16).ok_or_else(|| format!("invalid color: {v}"))?;
                rgb = (rgb << 8) | (n * 17);
            }
            Ok(0xff00_0000 | rgb)
        }
        6 => Ok(0xff00_0000 | parse(hex)?),
        8 => parse(hex),
        _ => Err(format!("invalid color: {v}")),
    }
}
/// Parses stretch from the supplied representation.
///
/// Visibility: **private**.
#[doc = "Parses stretch. Visibility: private/internal."]
fn parse_stretch(v: &str) -> Result<FontStretch, String> {
    match v.trim().to_ascii_lowercase().as_str() {
        "ultra-condensed" => Ok(FontStretch::UltraCondensed),
        "extra-condensed" => Ok(FontStretch::ExtraCondensed),
        "condensed" => Ok(FontStretch::Condensed),
        "semi-condensed" => Ok(FontStretch::SemiCondensed),
        "normal" => Ok(FontStretch::Normal),
        "semi-expanded" => Ok(FontStretch::SemiExpanded),
        "expanded" => Ok(FontStretch::Expanded),
        "extra-expanded" => Ok(FontStretch::ExtraExpanded),
        "ultra-expanded" => Ok(FontStretch::UltraExpanded),
        _ => Err(format!("invalid font-stretch: {v}")),
    }
}

/// Resolves font size from the current state.
///
/// Visibility: **private**.
#[doc = "Resolves font size. Visibility: private/internal."]
fn resolve_font_size(v: Length, ctx: LengthContext, parent: f32) -> f32 {
    match v {
        Length::Percent(n) => (parent * n) / 100.0,
        Length::Em(n) => parent * n,
        Length::Rem(n) => ctx.root_font_size * n,
        other => other.resolve(Axis::Vertical, ctx).unwrap_or(parent),
    }
}
