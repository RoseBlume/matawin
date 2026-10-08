//! Media-query predicates based on the widget host viewport.

use super::{ Axis, Length, LengthContext };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Defines the available media orientation choices.
///
/// Visibility: **public**.
pub enum MediaOrientation {
    /// Represents the portrait variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Portrait,
    /// Represents the landscape variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Landscape,
}
#[derive(Debug, Clone, Default, PartialEq)]
/// Stores the state and data for media query.
///
/// Visibility: **public**.
pub struct MediaQuery {
    /// Stores min width.
    ///
    /// Visibility: **public**.
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    /// Stores min height.
    ///
    /// Visibility: **public**.
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
    /// Stores orientation.
    ///
    /// Visibility: **public**.
    pub orientation: Option<MediaOrientation>,
    /// Stores min aspect ratio.
    ///
    /// Visibility: **public**.
    pub min_aspect_ratio: Option<f32>,
    pub max_aspect_ratio: Option<f32>,
}
impl MediaQuery {
    /// Performs the matches operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the matches operation. Visibility: public."]
    pub fn matches(&self, viewport: (u32, u32), root_font_size: f32) -> bool {
        let ctx = LengthContext::new(viewport, viewport, root_font_size, root_font_size);
        let w = viewport.0 as f32;
        let h = viewport.1 as f32;
        if self.min_width.and_then(|v| v.resolve(Axis::Horizontal, ctx)).is_some_and(|v| w < v) {
            return false;
        }
        if self.max_width.and_then(|v| v.resolve(Axis::Horizontal, ctx)).is_some_and(|v| w > v) {
            return false;
        }
        if self.min_height.and_then(|v| v.resolve(Axis::Vertical, ctx)).is_some_and(|v| h < v) {
            return false;
        }
        if self.max_height.and_then(|v| v.resolve(Axis::Vertical, ctx)).is_some_and(|v| h > v) {
            return false;
        }
        if let Some(o) = self.orientation {
            let actual = if w >= h {
                MediaOrientation::Landscape
            } else {
                MediaOrientation::Portrait
            };
            if actual != o {
                return false;
            }
        }
        let ratio = if h <= 0.0 { f32::INFINITY } else { w / h };
        if self.min_aspect_ratio.is_some_and(|v| ratio < v) {
            return false;
        }
        if self.max_aspect_ratio.is_some_and(|v| ratio > v) {
            return false;
        }
        true
    }
    /// Parses parse from the supplied representation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut q = Self::default();
        let conditions = media_conditions(raw);
        for p in conditions {
            let p = p.trim();
            if p.is_empty() || p.eq_ignore_ascii_case("screen") || p.eq_ignore_ascii_case("all") {
                continue;
            }
            let Some((name, value)) = p.split_once(':') else {
                return Err(format!("unsupported media condition: {p}"));
            };
            match name.trim().to_ascii_lowercase().as_str() {
                "min-width" => {
                    q.min_width = Some(Length::parse(value)?);
                }
                "max-width" => {
                    q.max_width = Some(Length::parse(value)?);
                }
                "min-height" => {
                    q.min_height = Some(Length::parse(value)?);
                }
                "max-height" => {
                    q.max_height = Some(Length::parse(value)?);
                }
                "orientation" => {
                    q.orientation = Some(match value.trim().to_ascii_lowercase().as_str() {
                        "portrait" => MediaOrientation::Portrait,
                        "landscape" => MediaOrientation::Landscape,
                        _ => {
                            return Err(format!("invalid orientation: {value}"));
                        }
                    });
                }
                "min-aspect-ratio" => {
                    q.min_aspect_ratio = Some(parse_ratio(value)?);
                }
                "max-aspect-ratio" => {
                    q.max_aspect_ratio = Some(parse_ratio(value)?);
                }
                other => {
                    return Err(format!("unsupported media feature: {other}"));
                }
            }
        }
        Ok(q)
    }
}

/// Implements media conditions.
///
/// Visibility: **private**.
#[doc = "Implements the media conditions operation. Visibility: private/internal."]
fn media_conditions(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let chars: Vec<_> = raw.char_indices().collect();
    for (i, c) in chars.iter().copied() {
        if c == '(' {
            if depth == 0 {
                start = Some(i + 1);
            }
            depth += 1;
        } else if c == ')' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start.take() {
                    out.push(raw[s..i].trim().to_string());
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    let words: Vec<&str> = raw.split_whitespace().collect();
    words
        .split(|token| (*token).eq_ignore_ascii_case("and"))
        .filter_map(|part| {
            let s = part.join(" ");
            (!s.is_empty()).then_some(s)
        })
        .collect()
}
/// Parses ratio from the supplied representation.
///
/// Visibility: **private**.
#[doc = "Parses ratio. Visibility: private/internal."]
fn parse_ratio(v: &str) -> Result<f32, String> {
    let v = v.trim();
    if let Some((a, b)) = v.split_once('/') {
        let a: f32 = a
            .trim()
            .parse()
            .map_err(|_| format!("invalid ratio: {v}"))?;
        let b: f32 = b
            .trim()
            .parse()
            .map_err(|_| format!("invalid ratio: {v}"))?;
        if b == 0.0 {
            return Err("aspect-ratio denominator cannot be zero".into());
        }
        Ok(a / b)
    } else {
        v.parse().map_err(|_| format!("invalid ratio: {v}"))
    }
}
