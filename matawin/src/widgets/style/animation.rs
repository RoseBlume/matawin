//! Keyframe animation definitions.

use std::time::Duration;
use super::StyleProperties;

#[derive(Debug, Clone, Copy, PartialEq)]
/// Defines the available animation iteration count choices.
///
/// Visibility: **public**.
pub enum AnimationIterationCount {
    /// Represents the count variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Count(f32),
    /// Represents the infinite variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Infinite,
}
impl Default for AnimationIterationCount {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::Count(1.0)
    }
}
impl AnimationIterationCount {
    /// Parses parse from the supplied representation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(v: &str) -> Result<Self, String> {
        let s = v.trim();
        if s.eq_ignore_ascii_case("infinite") {
            return Ok(Self::Infinite);
        }
        let n = s.parse::<f32>().map_err(|_| format!("invalid animation-iteration-count: {v}"))?;
        Ok(Self::Count(n.max(0.0)))
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores the state and data for animation spec.
///
/// Visibility: **public**.
pub struct AnimationSpec {
    /// Stores name.
    ///
    /// Visibility: **public**.
    pub name: String,
    /// Stores duration.
    ///
    /// Visibility: **public**.
    pub duration: Duration,
    /// Stores delay.
    ///
    /// Visibility: **public**.
    pub delay: Duration,
    /// Stores iteration count.
    ///
    /// Visibility: **public**.
    pub iteration_count: AnimationIterationCount,
}
impl Default for AnimationSpec {
    /// Performs the default operation.
    ///
    /// Visibility: **private**.
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self {
            name: String::new(),
            duration: Duration::ZERO,
            delay: Duration::ZERO,
            iteration_count: AnimationIterationCount::Count(1.0),
        }
    }
}
impl AnimationSpec {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(name: impl Into<String>, duration: Duration) -> Self {
        Self { name: name.into(), duration, ..Self::default() }
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores the state and data for keyframe.
///
/// Visibility: **public**.
pub struct Keyframe {
    /// Stores offset.
    ///
    /// Visibility: **public**.
    pub offset: f32,
    /// Stores style.
    ///
    /// Visibility: **public**.
    pub style: StyleProperties,
}
impl Keyframe {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(offset: f32, style: StyleProperties) -> Self {
        Self { offset: offset.clamp(0.0, 1.0), style }
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores the state and data for keyframes.
///
/// Visibility: **public**.
pub struct Keyframes {
    /// Stores name.
    ///
    /// Visibility: **public**.
    pub name: String,
    /// Stores frames.
    ///
    /// Visibility: **public**.
    pub frames: Vec<Keyframe>,
}
impl Keyframes {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), frames: Vec::new() }
    }
    /// Performs the push operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the push operation. Visibility: public."]
    pub fn push(&mut self, frame: Keyframe) {
        self.frames.push(frame);
        self.frames.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    }
    /// Returns the value configured with frame.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with frame. Visibility: public."]
    pub fn with_frame(mut self, offset: f32, style: StyleProperties) -> Self {
        self.push(Keyframe::new(offset, style));
        self
    }
    /// Parses the body of a CSS-like `@keyframes` rule.
    /// Example: `from { opacity: 0; } 50% { left: 2vw; } to { opacity: 1; }`.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(name: impl Into<String>, raw: &str) -> Result<Self, String> {
        let mut out = Self::new(name);
        let bytes = raw.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let selector_start = i;
            while i < bytes.len() && bytes[i] != b'{' {
                i += 1;
            }
            if i >= bytes.len() {
                return Err("keyframe block missing '{'".into());
            }
            let selector = raw[selector_start..i].trim();
            i += 1;
            let body_start = i;
            let mut depth = 1usize;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                i += 1;
            }
            if i >= bytes.len() {
                return Err("keyframe block missing '}'".into());
            }
            let body = &raw[body_start..i];
            i += 1;
            let mut patch = StyleProperties::default();
            for declaration in body.split(';') {
                let d = declaration.trim();
                if d.is_empty() {
                    continue;
                }
                let Some((property, value)) = d.split_once(':') else {
                    return Err(format!("invalid keyframe declaration: {d}"));
                };
                patch.set_property(property, value)?;
            }
            for sel in selector.split(',') {
                let s = sel.trim().to_ascii_lowercase();
                let offset = if s == "from" {
                    0.0
                } else if s == "to" {
                    1.0
                } else if let Some(n) = s.strip_suffix('%') {
                    n
                        .trim()
                        .parse::<f32>()
                        .map_err(|_| format!("invalid keyframe selector: {sel}"))? / 100.0
                } else {
                    return Err(format!("invalid keyframe selector: {sel}"));
                };
                out.push(Keyframe::new(offset, patch.clone()));
            }
        }
        Ok(out)
    }
}

/// Returns whether running.
///
/// Visibility: **crate-visible**.
#[doc = "Returns whether running. Visibility: crate-visible."]
pub(crate) fn is_running(spec: &AnimationSpec, elapsed: Duration) -> bool {
    if spec.name.is_empty() || spec.duration.is_zero() || elapsed < spec.delay {
        return false;
    }
    let active = elapsed - spec.delay;
    match spec.iteration_count {
        AnimationIterationCount::Infinite => true,
        AnimationIterationCount::Count(n) => n > 0.0 && active < spec.duration.mul_f32(n),
    }
}

/// Performs the sample operation.
///
/// Visibility: **crate-visible**.
#[doc = "Implements the sample operation. Visibility: crate-visible."]
pub(crate) fn sample(
    spec: &AnimationSpec,
    keyframes: &Keyframes,
    elapsed: Duration
) -> Option<StyleProperties> {
    if
        spec.name.is_empty() ||
        spec.duration.is_zero() ||
        elapsed < spec.delay ||
        keyframes.frames.is_empty()
    {
        return None;
    }
    let elapsed = elapsed - spec.delay;
    let total = elapsed.as_secs_f32() / spec.duration.as_secs_f32();
    let progress = match spec.iteration_count {
        AnimationIterationCount::Infinite => total.fract(),
        AnimationIterationCount::Count(n) => {
            if n <= 0.0 {
                return None;
            }
            if total >= n {
                1.0
            } else {
                total.fract()
            }
        }
    };
    let mut before = &keyframes.frames[0];
    let mut after = &keyframes.frames[keyframes.frames.len() - 1];
    for frame in &keyframes.frames {
        if frame.offset <= progress {
            before = frame;
        }
        if frame.offset >= progress {
            after = frame;
            break;
        }
    }
    if std::ptr::eq(before, after) || after.offset <= before.offset {
        return Some(before.style.clone());
    }
    let t = (progress - before.offset) / (after.offset - before.offset);
    Some(before.style.interpolate(&after.style, t))
}

/// Parses duration from the supplied representation.
///
/// Visibility: **public**.
#[doc = "Parses duration. Visibility: public."]
pub fn parse_duration(v: &str) -> Result<Duration, String> {
    let s = v.trim().to_ascii_lowercase();
    if let Some(n) = s.strip_suffix("ms") {
        let n: f32 = n
            .trim()
            .parse()
            .map_err(|_| format!("invalid duration: {v}"))?;
        return Ok(Duration::from_secs_f32((n / 1000.0).max(0.0)));
    }
    if let Some(n) = s.strip_suffix('s') {
        let n: f32 = n
            .trim()
            .parse()
            .map_err(|_| format!("invalid duration: {v}"))?;
        return Ok(Duration::from_secs_f32(n.max(0.0)));
    }
    Err(format!("duration requires ms or s: {v}"))
}
