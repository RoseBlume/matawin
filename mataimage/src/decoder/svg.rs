//! SVG image decoding implementation.
//!
//! This module parses SVG input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::pixels::pack_rgba;
use std::io::{Read, Seek};

use crate::ImageError;

// ============================================================================
// Public SVG type
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
/// Represents an SVG document together with its decoded raster dimensions and pixel data.
pub struct Svg {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// Original encoded SVG file.
    pub data: Vec<u8>,

    /// Rasterized pixels in 0xRRGGBBAA format.
    pixels: Vec<u32>,
}

impl Svg {
    /// Constructs a rich SVG object from borrowed input.
    ///
    /// The XML bytes are copied once because the rich object retains them. Use
    /// [`crate::Image::from_bytes`] for raster-only decoding without retention.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::from_bytes(bytes.to_vec())
    }

    /// Constructs this rich value from owned SVG bytes without copying them.
    ///
    /// The source XML is intentionally retained in [`Self::data`]. Use
    /// [`crate::Image::from_bytes`] when only the rasterized result is needed.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, ImageError> {
        let text = std::str::from_utf8(&data)
            .map_err(|_| ImageError::new("SVG is not valid UTF-8"))?;
        let decoded = decode_svg(text)?;
        Ok(Self {
            width: decoded.width,
            height: decoded.height,
            data,
            pixels: decoded.pixels,
        })
    }

    /// Explicit owned-source constructor for callers that need the rich SVG
    /// object and retained XML source.
    #[inline]
    pub fn parse_owned(data: Vec<u8>) -> Result<Self, ImageError> {
        Self::from_bytes(data)
    }

    /// Parses the supplied encoded input into this format-specific representation.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .map_err(|e| ImageError::new(format!("Failed to read SVG: {e}")))?;

        Self::from_bytes(data)
    }

    /// Borrows rasterized packed pixels without cloning them.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        &self.pixels
    }

    /// Moves rasterized packed pixels out of the rich SVG object.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.pixels
    }

    /// Borrows the retained SVG source XML bytes.
    #[inline]
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Rasterizes SVG directly from borrowed UTF-8 input without retaining the XML bytes.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ImageError::new("SVG is not valid UTF-8"))?;
    let decoded = decode_svg(text)?;
    options.checked_rgba_bytes(decoded.width, decoded.height)?;
    crate::Image::from_packed_pixels(decoded.width, decoded.height, decoded.pixels, options)
}

impl super::Decoder for Svg {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        crate::Image::from_packed_pixels(
            this.width,
            this.height,
            this.pixels,
            &crate::DecodeOptions::default(),
        )
    }
}

// ============================================================================
// Core geometry / paint types
// ============================================================================

#[derive(Debug, Clone, Copy, Default)]
/// Represents a two-dimensional point in SVG user-space coordinates.
struct Point {
    /// Stores the horizontal coordinate or offset.
    x: f64,
    /// Stores the vertical coordinate or offset.
    y: f64,
}

impl Point {
    #[inline]
    /// Constructs and initializes a new instance of this type.
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy)]
/// Represents a two-dimensional affine transformation matrix.
struct Affine {
    /// Stores the affine matrix coefficient multiplying X in the transformed X coordinate.
    a: f64,
    /// Stores the affine matrix coefficient multiplying X in the transformed Y coordinate.
    b: f64,
    /// Stores the affine matrix coefficient multiplying Y in the transformed X coordinate.
    c: f64,
    /// Stores the affine matrix coefficient multiplying Y in the transformed Y coordinate.
    d: f64,
    /// Stores the horizontal translation component of the affine transform.
    e: f64,
    /// Stores the vertical translation component of the affine transform.
    f: f64,
}

impl Default for Affine {
    /// Returns the default value used to initialize this decoding or rendering state.
    fn default() -> Self {
        Self::identity()
    }
}

impl Affine {
    /// Returns the identity transform, which leaves input coordinates unchanged.
    const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Builds a translation transform from the supplied horizontal and vertical offsets.
    const fn translate(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    /// Builds a scale transform from the supplied horizontal and vertical scale factors.
    const fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Builds a rotation transform for the supplied angle in radians.
    fn rotate(radians: f64) -> Self {
        let c = radians.cos();
        let s = radians.sin();

        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Matrix multiplication: self * rhs.
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    #[inline]
    /// Applies this transform or operation to the supplied value and returns the transformed result.
    fn apply(self, p: Point) -> Point {
        Point {
            x: self.a * p.x + self.c * p.y + self.e,
            y: self.b * p.x + self.d * p.y + self.f,
        }
    }

    #[inline]
    /// Computes an approximate uniform scale factor represented by this affine transform.
    fn average_scale(self) -> f64 {
        let sx = (self.a * self.a + self.b * self.b).sqrt();
        let sy = (self.c * self.c + self.d * self.d).sqrt();
        (sx + sy) * 0.5
    }
}

#[derive(Debug, Clone, Copy)]
/// Represents an RGBA color used by the SVG rasterizer.
struct Color {
    /// Stores the 8-bit red color component.
    r: u8,
    /// Stores the 8-bit green color component.
    g: u8,
    /// Stores the 8-bit blue color component.
    b: u8,
    /// Stores the 8-bit alpha component.
    a: u8,
}

impl Color {
    const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    /// Returns this color with its alpha channel adjusted by the supplied opacity multiplier.
    fn with_opacity(self, opacity: f64) -> Self {
        Self {
            a: ((self.a as f64 * opacity.clamp(0.0, 1.0)).round())
                .clamp(0.0, 255.0) as u8,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Specifies how SVG path winding determines whether a point lies inside a filled shape.
enum FillRule {
    /// Uses the non-zero winding rule when determining whether a point is inside an SVG fill.
    NonZero,
    /// Uses even/odd crossing parity when determining whether a point is inside an SVG fill.
    EvenOdd,
}

#[derive(Debug, Clone, Copy)]
/// Collects the SVG painting properties that affect shape rendering.
struct Style {
    /// Stores the optional SVG fill paint; `None` disables filling.
    fill: Option<Color>,
    /// Stores the optional SVG stroke paint; `None` disables stroking.
    stroke: Option<Color>,
    /// Stores the SVG stroke width in current user-space units.
    stroke_width: f64,
    /// Stores the overall SVG opacity multiplier for the current style.
    opacity: f64,
    /// Stores the opacity multiplier applied specifically to fill paint.
    fill_opacity: f64,
    /// Stores the opacity multiplier applied specifically to stroke paint.
    stroke_opacity: f64,
    /// Stores the SVG winding rule used when filling paths.
    fill_rule: FillRule,
}

impl Default for Style {
    /// Returns the default value used to initialize this decoding or rendering state.
    fn default() -> Self {
        Self {
            fill: Some(Color::BLACK),
            stroke: None,
            stroke_width: 1.0,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            fill_rule: FillRule::NonZero,
        }
    }
}

#[derive(Debug, Clone, Copy)]
/// Tracks inherited SVG rendering state while traversing document elements.
struct State {
    /// Stores the inherited SVG painting style active at the current element.
    style: Style,
    /// Stores the affine transform active for this rendering state.
    transform: Affine,
}

impl Default for State {
    /// Returns the default value used to initialize this decoding or rendering state.
    fn default() -> Self {
        Self {
            style: Style::default(),
            transform: Affine::identity(),
        }
    }
}

/// Contains the rasterized result of decoding an SVG document.
struct DecodedSvg {
    /// Stores the image, frame, or plane width in pixels.
    width: u32,
    /// Stores the image, frame, or plane height in pixels.
    height: u32,
    /// Stores the decoded packed pixel values produced by the decoder.
    pixels: Vec<u32>,
}

// ============================================================================
// SVG decode
// ============================================================================

/// Decodes svg data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_svg(text: &str) -> Result<DecodedSvg, ImageError> {
    let root = find_root_svg(text)
        .ok_or_else(|| ImageError::new("SVG does not contain an <svg> root element"))?;

    let root_attrs = parse_attributes(root)?;

    let view_box = root_attrs
        .get("viewbox")
        .and_then(|s| _parse_number_list(s).ok())
        .and_then(|v| {
            if v.len() == 4 && v[2] > 0.0 && v[3] > 0.0 {
                Some((v[0], v[1], v[2], v[3]))
            } else {
                None
            }
        });

    let width = root_attrs
        .get("width")
        .and_then(|s| _parse_length(s))
        .or_else(|| view_box.map(|v| v.2))
        .unwrap_or(300.0)
        .round()
        .max(1.0) as u32;

    let height = root_attrs
        .get("height")
        .and_then(|s| _parse_length(s))
        .or_else(|| view_box.map(|v| v.3))
        .unwrap_or(150.0)
        .round()
        .max(1.0) as u32;

    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| ImageError::new("SVG pixel count overflow"))?;

    let mut canvas = Canvas {
        width: width as usize,
        height: height as usize,
        pixels: vec![0u32; pixel_count],
    };

    let mut root_transform = Affine::identity();

    if let Some((min_x, min_y, vb_w, vb_h)) = view_box {
        let sx = width as f64 / vb_w;
        let sy = height as f64 / vb_h;

        // This intentionally maps the viewBox directly to the output bitmap.
        // preserveAspectRatio is not currently implemented.
        root_transform = Affine::scale(sx, sy)
            .mul(Affine::translate(-min_x, -min_y));
    }

    if let Some(t) = root_attrs.get("transform") {
        root_transform = root_transform.mul(parse_transform(t)?);
    }

    let mut root_style = Style::default();
    apply_style_attributes(&mut root_style, &root_attrs)?;

    let mut stack = vec![State {
        style: root_style,
        transform: root_transform,
    }];

    let mut saw_root_svg = false;
    let mut pos = 0usize;
    while let Some(open_rel) = text[pos..].find('<') {
        let open = pos + open_rel;

        // Comments.
        if text[open..].starts_with("<!--") {
            if let Some(end_rel) = text[open + 4..].find("-->") {
                pos = open + 4 + end_rel + 3;
                continue;
            }
            return Err(ImageError::new("Unterminated SVG comment"));
        }

        // Processing instructions.
        if text[open..].starts_with("<?") {
            if let Some(end_rel) = text[open + 2..].find("?>") {
                pos = open + 2 + end_rel + 2;
                continue;
            }
            return Err(ImageError::new("Unterminated SVG processing instruction"));
        }

        // <!DOCTYPE ...> and other declarations.
        if text[open..].starts_with("<!") {
            let end = find_tag_end(text, open + 2)
                .ok_or_else(|| ImageError::new("Unterminated SVG declaration"))?;
            pos = end + 1;
            continue;
        }

        let end = find_tag_end(text, open + 1)
            .ok_or_else(|| ImageError::new("Unterminated SVG tag"))?;

        let raw = text[open + 1..end].trim();
        pos = end + 1;

        if raw.is_empty() {
            continue;
        }

        if raw.starts_with('/') {
            let closing_name = raw[1..]
                .trim()
                .split_ascii_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();

            if matches!(closing_name.as_str(), "g" | "svg" | "a" | "symbol")
                && stack.len() > 1
            {
                stack.pop();
            }

            continue;
        }

        let self_closing = raw.ends_with('/');
        let content = raw.trim_end_matches('/').trim();

        let (name, _) = split_tag_name(content);
        let name = name.to_ascii_lowercase();

        // The root <svg> is already represented by stack[0].
        if name == "svg" && !saw_root_svg {
            saw_root_svg = true;
            continue;
        }

        let attrs = parse_attributes(content)?;
        let parent = stack.last().copied().unwrap_or_default();

        let mut state = parent;
        apply_style_attributes(&mut state.style, &attrs)?;

        if let Some(transform) = attrs.get("transform") {
            state.transform = parent.transform.mul(parse_transform(transform)?);
        }

        match name.as_str() {
            "g" | "svg" | "a" | "symbol" => {
                if !self_closing {
                    stack.push(state);
                }
            }

            "rect" => render_rect(&mut canvas, &attrs, &state)?,
            "circle" => render_circle(&mut canvas, &attrs, &state)?,
            "ellipse" => render_ellipse(&mut canvas, &attrs, &state)?,
            "line" => render_line(&mut canvas, &attrs, &state)?,
            "polyline" => render_poly(&mut canvas, &attrs, &state, false)?,
            "polygon" => render_poly(&mut canvas, &attrs, &state, true)?,
            "path" => render_path(&mut canvas, &attrs, &state)?,

            // Non-rasterized SVG elements are intentionally ignored:
            // defs, gradients, clipPath, mask, pattern, text, image, use, etc.
            _ => {}
        }
    }

    Ok(DecodedSvg {
        width,
        height,
        pixels: canvas.pixels,
    })
}

// ============================================================================
// Primitive rendering
// ============================================================================

/// Rasterizes rect onto the active SVG canvas.
///
/// Returns an error when the supplied data cannot be processed as required.
fn render_rect(
    canvas: &mut Canvas,
    attrs: &Attributes,
    state: &State,
) -> Result<(), ImageError> {
    let x = _attr_num(attrs, "x", 0.0)?;
    let y = _attr_num(attrs, "y", 0.0)?;
    let w = _attr_num(attrs, "width", 0.0)?;
    let h = _attr_num(attrs, "height", 0.0)?;

    if w <= 0.0 || h <= 0.0 {
        return Ok(());
    }

    let rx = _attr_num(attrs, "rx", 0.0)?.max(0.0);
    let ry = _attr_num(attrs, "ry", 0.0)?.max(0.0);

    let points = if rx > 0.0 || ry > 0.0 {
        rounded_rect_points(
            x,
            y,
            w,
            h,
            if rx > 0.0 { rx.min(w * 0.5) } else { ry.min(w * 0.5) },
            if ry > 0.0 { ry.min(h * 0.5) } else { rx.min(h * 0.5) },
        )
    } else {
        vec![
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
            Point::new(x, y),
        ]
    };

    render_subpaths(canvas, vec![transform_points(points, state.transform)], state, true);
    Ok(())
}

/// Rasterizes circle onto the active SVG canvas.
///
/// Returns an error when the supplied data cannot be processed as required.
fn render_circle(
    canvas: &mut Canvas,
    attrs: &Attributes,
    state: &State,
) -> Result<(), ImageError> {
    let cx = _attr_num(attrs, "cx", 0.0)?;
    let cy = _attr_num(attrs, "cy", 0.0)?;
    let r = _attr_num(attrs, "r", 0.0)?;

    if r <= 0.0 {
        return Ok(());
    }

    let points = ellipse_points(cx, cy, r, r);
    render_subpaths(canvas, vec![transform_points(points, state.transform)], state, true);
    Ok(())
}

/// Rasterizes ellipse onto the active SVG canvas.
///
/// Returns an error when the supplied data cannot be processed as required.
fn render_ellipse(
    canvas: &mut Canvas,
    attrs: &Attributes,
    state: &State,
) -> Result<(), ImageError> {
    let cx = _attr_num(attrs, "cx", 0.0)?;
    let cy = _attr_num(attrs, "cy", 0.0)?;
    let rx = _attr_num(attrs, "rx", 0.0)?;
    let ry = _attr_num(attrs, "ry", 0.0)?;

    if rx <= 0.0 || ry <= 0.0 {
        return Ok(());
    }

    let points = ellipse_points(cx, cy, rx, ry);
    render_subpaths(canvas, vec![transform_points(points, state.transform)], state, true);
    Ok(())
}

/// Rasterizes line onto the active SVG canvas.
///
/// Returns an error when the supplied data cannot be processed as required.
fn render_line(
    canvas: &mut Canvas,
    attrs: &Attributes,
    state: &State,
) -> Result<(), ImageError> {
    let p1 = state.transform.apply(Point::new(
        _attr_num(attrs, "x1", 0.0)?,
        _attr_num(attrs, "y1", 0.0)?,
    ));

    let p2 = state.transform.apply(Point::new(
        _attr_num(attrs, "x2", 0.0)?,
        _attr_num(attrs, "y2", 0.0)?,
    ));

    if let Some(stroke) = state.style.stroke {
        let opacity = state.style.opacity * state.style.stroke_opacity;
        canvas.stroke_polyline(
            &[p1, p2],
            false,
            state.style.stroke_width * state.transform.average_scale(),
            stroke.with_opacity(opacity),
        );
    }

    Ok(())
}

/// Rasterizes poly onto the active SVG canvas.
///
/// Returns an error when the supplied data cannot be processed as required.
fn render_poly(
    canvas: &mut Canvas,
    attrs: &Attributes,
    state: &State,
    closed: bool,
) -> Result<(), ImageError> {
    let Some(raw) = attrs.get("points") else {
        return Ok(());
    };

    let values = _parse_number_list(raw)?;
    if values.len() < 4 || values.len() % 2 != 0 {
        return Err(ImageError::new("Invalid SVG points list"));
    }

    let mut points = Vec::with_capacity(values.len() / 2 + usize::from(closed));

    for pair in values.chunks_exact(2) {
        points.push(state.transform.apply(Point::new(pair[0], pair[1])));
    }

    if closed {
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
    }

    render_subpaths(canvas, vec![points], state, closed);
    Ok(())
}

/// Rasterizes path onto the active SVG canvas.
///
/// Returns an error when the supplied data cannot be processed as required.
fn render_path(
    canvas: &mut Canvas,
    attrs: &Attributes,
    state: &State,
) -> Result<(), ImageError> {
    let Some(d) = attrs.get("d") else {
        return Ok(());
    };

    let local = parse_path(d)?;
    let mut transformed = Vec::with_capacity(local.len());

    for sub in local {
        transformed.push(transform_points(sub, state.transform));
    }

    render_subpaths(canvas, transformed, state, false);
    Ok(())
}

/// Rasterizes subpaths onto the active SVG canvas.
fn render_subpaths(
    canvas: &mut Canvas,
    subpaths: Vec<Vec<Point>>,
    state: &State,
    assume_closed: bool,
) {
    if subpaths.is_empty() {
        return;
    }

    if let Some(fill) = state.style.fill {
        let opacity = state.style.opacity * state.style.fill_opacity;
        canvas.fill_subpaths(
            &subpaths,
            fill.with_opacity(opacity),
            state.style.fill_rule,
        );
    }

    if let Some(stroke) = state.style.stroke {
        let opacity = state.style.opacity * state.style.stroke_opacity;
        let width = state.style.stroke_width * state.transform.average_scale();

        for sub in &subpaths {
            let closed = assume_closed
                || (sub.len() >= 2
                    && _nearly_equal(sub[0].x, sub[sub.len() - 1].x)
                    && _nearly_equal(sub[0].y, sub[sub.len() - 1].y));

            canvas.stroke_polyline(
                sub,
                closed,
                width,
                stroke.with_opacity(opacity),
            );
        }
    }
}

// ============================================================================
// Canvas
// ============================================================================

/// Provides the mutable pixel surface used while rasterizing SVG geometry.
struct Canvas {
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the image, frame, or plane height in pixels.
    height: usize,
    /// Stores the decoded packed pixel values produced by the decoder.
    pixels: Vec<u32>,
}

impl Canvas {
    /// Fills subpaths in the destination image according to the active decoding or rendering state.
    fn fill_subpaths(
        &mut self,
        paths: &[Vec<Point>],
        color: Color,
        rule: FillRule,
    ) {
        if color.a == 0 {
            return;
        }

        let (min_y, max_y) = path_y_bounds(paths);
        let y0 = min_y.floor().max(0.0) as i32;
        let y1 = max_y.ceil().min(self.height as f64) as i32;

        for yi in y0..y1 {
            let scan_y = yi as f64 + 0.5;
            let mut events: Vec<(f64, i32)> = Vec::new();

            for path in paths {
                if path.len() < 2 {
                    continue;
                }

                for edge in path.windows(2) {
                    add_scan_event(edge[0], edge[1], scan_y, &mut events);
                }

                if !_same_point(path[0], path[path.len() - 1]) {
                    add_scan_event(path[path.len() - 1], path[0], scan_y, &mut events);
                }
            }

            if events.len() < 2 {
                continue;
            }

            events.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            match rule {
                FillRule::EvenOdd => {
                    let mut i = 0usize;
                    while i + 1 < events.len() {
                        self.fill_span(yi, events[i].0, events[i + 1].0, color);
                        i += 2;
                    }
                }

                FillRule::NonZero => {
                    let mut winding = 0i32;
                    let mut previous_x = events[0].0;
                    let mut i = 0usize;

                    while i < events.len() {
                        let x = events[i].0;

                        if winding != 0 {
                            self.fill_span(yi, previous_x, x, color);
                        }

                        let mut delta = 0i32;
                        while i < events.len() && (events[i].0 - x).abs() < 1e-9 {
                            delta += events[i].1;
                            i += 1;
                        }

                        winding += delta;
                        previous_x = x;
                    }
                }
            }
        }
    }

    /// Rasterizes the stroke for polyline using the active SVG style.
    fn stroke_polyline(
        &mut self,
        points: &[Point],
        closed: bool,
        width: f64,
        color: Color,
    ) {
        if points.len() < 2 || width <= 0.0 || color.a == 0 {
            return;
        }

        for edge in points.windows(2) {
            self.stroke_segment(edge[0], edge[1], width, color);
        }

        if closed && !_same_point(points[0], points[points.len() - 1]) {
            self.stroke_segment(points[points.len() - 1], points[0], width, color);
        }
    }

    /// Rasterizes the stroke for segment using the active SVG style.
    fn stroke_segment(
        &mut self,
        a: Point,
        b: Point,
        width: f64,
        color: Color,
    ) {
        let radius = width * 0.5;

        let min_x = (a.x.min(b.x) - radius - 1.0)
            .floor()
            .max(0.0) as i32;

        let max_x = (a.x.max(b.x) + radius + 1.0)
            .ceil()
            .min(self.width as f64) as i32;

        let min_y = (a.y.min(b.y) - radius - 1.0)
            .floor()
            .max(0.0) as i32;

        let max_y = (a.y.max(b.y) + radius + 1.0)
            .ceil()
            .min(self.height as f64) as i32;

        let ab_x = b.x - a.x;
        let ab_y = b.y - a.y;
        let len_sq = ab_x * ab_x + ab_y * ab_y;

        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;

                let t = if len_sq > 1e-18 {
                    (((px - a.x) * ab_x + (py - a.y) * ab_y) / len_sq)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let qx = a.x + ab_x * t;
                let qy = a.y + ab_y * t;

                let dx = px - qx;
                let dy = py - qy;

                if dx * dx + dy * dy <= radius * radius {
                    self.blend(x as usize, y as usize, color);
                }
            }
        }
    }

    /// Fills span in the destination image according to the active decoding or rendering state.
    fn fill_span(
        &mut self,
        y: i32,
        x0: f64,
        x1: f64,
        color: Color,
    ) {
        let left = x0.min(x1);
        let right = x0.max(x1);

        let start = left.ceil().max(0.0) as i32;
        let end = right.ceil().min(self.width as f64) as i32;

        for x in start..end {
            self.blend(x as usize, y as usize, color);
        }
    }

    #[inline]
    /// Blends the supplied source color or frame data into the destination image.
    fn blend(&mut self, x: usize, y: usize, src: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let index = y * self.width + x;
        let dst = _unpack_rgba(self.pixels[index]);

        let sa = src.a as u32;
        if sa == 255 {
            self.pixels[index] = pack_rgba(src.r, src.g, src.b, 255);
            return;
        }
        if sa == 0 {
            return;
        }

        let da = dst.a as u32;
        let inv_sa = 255 - sa;

        let out_a = sa + (da * inv_sa + 127) / 255;
        if out_a == 0 {
            self.pixels[index] = 0;
            return;
        }

        let src_r_p = src.r as u32 * sa;
        let src_g_p = src.g as u32 * sa;
        let src_b_p = src.b as u32 * sa;

        let dst_r_p = dst.r as u32 * da;
        let dst_g_p = dst.g as u32 * da;
        let dst_b_p = dst.b as u32 * da;

        let out_r_p = src_r_p + (dst_r_p * inv_sa + 127) / 255;
        let out_g_p = src_g_p + (dst_g_p * inv_sa + 127) / 255;
        let out_b_p = src_b_p + (dst_b_p * inv_sa + 127) / 255;

        self.pixels[index] = pack_rgba(
            ((out_r_p + out_a / 2) / out_a).min(255) as u8,
            ((out_g_p + out_a / 2) / out_a).min(255) as u8,
            ((out_b_p + out_a / 2) / out_a).min(255) as u8,
            out_a.min(255) as u8,
        );
    }
}

/// Adds one edge crossing event to the scanline used by the SVG fill rasterizer.
fn add_scan_event(
    a: Point,
    b: Point,
    y: f64,
    out: &mut Vec<(f64, i32)>,
) {
    if _nearly_equal(a.y, b.y) {
        return;
    }

    let (low, high, winding) = if a.y < b.y {
        (a, b, 1)
    } else {
        (b, a, -1)
    };

    // Half-open interval avoids double-counting shared vertices.
    if y < low.y || y >= high.y {
        return;
    }

    let t = (y - low.y) / (high.y - low.y);
    let x = low.x + (high.x - low.x) * t;
    out.push((x, winding));
}

/// Computes the minimum and maximum Y coordinates covered by the supplied paths.
fn path_y_bounds(paths: &[Vec<Point>]) -> (f64, f64) {
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths {
        for p in path {
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
    }

    if !min_y.is_finite() || !max_y.is_finite() {
        (0.0, 0.0)
    } else {
        (min_y, max_y)
    }
}

// ============================================================================
// Path parser / flattener
// ============================================================================

/// Parses SVG path-data commands and numeric operands.
struct PathParser<'a> {
    /// Stores the raw SVG path-data bytes being parsed.
    bytes: &'a [u8],
    /// Stores the current byte position within the SVG path-data input.
    pos: usize,
}

impl<'a> PathParser<'a> {
    /// Constructs and initializes a new instance of this type.
    fn new(data: &'a str) -> Self {
        Self {
            bytes: data.as_bytes(),
            pos: 0,
        }
    }

    /// Advances the SVG path parser past whitespace and comma separators.
    fn skip_separators(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => self.pos += 1,
                _ => break,
            }
        }
    }

    /// Returns the next SVG path command without consuming it.
    fn peek_command(&mut self) -> Option<char> {
        self.skip_separators();
        let b = *self.bytes.get(self.pos)?;
        if b.is_ascii_alphabetic() {
            Some(b as char)
        } else {
            None
        }
    }

    /// Consumes and returns the next SVG path command when one is present.
    fn consume_command(&mut self) -> Option<char> {
        let cmd = self.peek_command()?;
        self.pos += 1;
        Some(cmd)
    }

    /// Returns whether the parser currently has number available.
    fn has_number(&mut self) -> bool {
        self.skip_separators();
        matches!(
            self.bytes.get(self.pos).copied(),
            Some(b'+') | Some(b'-') | Some(b'.') | Some(b'0'..=b'9')
        )
    }

    /// Parses and returns the next numeric operand from SVG path data.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn number(&mut self) -> Result<f64, ImageError> {
        self.skip_separators();

        let start = self.pos;

        if matches!(self.bytes.get(self.pos), Some(b'+') | Some(b'-')) {
            self.pos += 1;
        }

        let mut digits = false;

        while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
            digits = true;
        }

        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
                digits = true;
            }
        }

        if !digits {
            return Err(ImageError::new("Invalid number in SVG path"));
        }

        if matches!(self.bytes.get(self.pos), Some(b'e') | Some(b'E')) {
            self.pos += 1;

            if matches!(self.bytes.get(self.pos), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }

            let exponent_start = self.pos;
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }

            if self.pos == exponent_start {
                return Err(ImageError::new("Invalid SVG path exponent"));
            }
        }

        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| ImageError::new("Invalid UTF-8 in SVG path"))?;

        text.parse::<f64>()
            .map_err(|_| ImageError::new("Invalid number in SVG path"))
    }

    /// Parses an SVG arc flag and converts its numeric representation to a boolean.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn flag(&mut self) -> Result<bool, ImageError> {
        let v = self.number()?;
        if _nearly_equal(v, 0.0) {
            Ok(false)
        } else if _nearly_equal(v, 1.0) {
            Ok(true)
        } else {
            Err(ImageError::new("SVG arc flag must be 0 or 1"))
        }
    }
}

/// Parses path from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_path(data: &str) -> Result<Vec<Vec<Point>>, ImageError> {
    let mut p = PathParser::new(data);

    let mut command: Option<char> = None;
    let mut current = Point::default();
    let mut sub_start = Point::default();

    let mut previous_cubic_control: Option<Point> = None;
    let mut previous_quad_control: Option<Point> = None;

    let mut paths: Vec<Vec<Point>> = Vec::new();
    let mut active: Vec<Point> = Vec::new();

    while p.pos < p.bytes.len() {
        if let Some(c) = p.consume_command() {
            command = Some(c);
        }

        let cmd = command.ok_or_else(|| {
            ImageError::new("SVG path data begins without a command")
        })?;

        let relative = cmd.is_ascii_lowercase();
        let upper = cmd.to_ascii_uppercase();

        match upper {
            'M' => {
                let mut first = true;

                while p.has_number() {
                    let x = p.number()?;
                    let y = p.number()?;

                    let next = if relative {
                        Point::new(current.x + x, current.y + y)
                    } else {
                        Point::new(x, y)
                    };

                    if first {
                        if !active.is_empty() {
                            paths.push(std::mem::take(&mut active));
                        }

                        current = next;
                        sub_start = current;
                        active.push(current);
                        first = false;
                    } else {
                        active.push(next);
                        current = next;
                    }

                    previous_cubic_control = None;
                    previous_quad_control = None;
                }

                if first {
                    return Err(ImageError::new("SVG M command is missing coordinates"));
                }

                command = Some(if relative { 'l' } else { 'L' });
            }

            'L' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let x = p.number()?;
                    let y = p.number()?;
                    current = if relative {
                        Point::new(current.x + x, current.y + y)
                    } else {
                        Point::new(x, y)
                    };
                    active.push(current);
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG L command is missing coordinates"));
                }

                previous_cubic_control = None;
                previous_quad_control = None;
            }

            'H' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let x = p.number()?;
                    current.x = if relative { current.x + x } else { x };
                    active.push(current);
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG H command is missing a coordinate"));
                }

                previous_cubic_control = None;
                previous_quad_control = None;
            }

            'V' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let y = p.number()?;
                    current.y = if relative { current.y + y } else { y };
                    active.push(current);
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG V command is missing a coordinate"));
                }

                previous_cubic_control = None;
                previous_quad_control = None;
            }

            'C' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let c1 = read_path_point(&mut p, current, relative)?;
                    let c2 = read_path_point(&mut p, current, relative)?;
                    let end = read_path_point(&mut p, current, relative)?;

                    flatten_cubic(current, c1, c2, end, &mut active);

                    current = end;
                    previous_cubic_control = Some(c2);
                    previous_quad_control = None;
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG C command is missing coordinates"));
                }
            }

            'S' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let c1 = previous_cubic_control
                        .map(|c| Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y))
                        .unwrap_or(current);

                    let c2 = read_path_point(&mut p, current, relative)?;
                    let end = read_path_point(&mut p, current, relative)?;

                    flatten_cubic(current, c1, c2, end, &mut active);

                    current = end;
                    previous_cubic_control = Some(c2);
                    previous_quad_control = None;
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG S command is missing coordinates"));
                }
            }

            'Q' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let control = read_path_point(&mut p, current, relative)?;
                    let end = read_path_point(&mut p, current, relative)?;

                    flatten_quadratic(current, control, end, &mut active);

                    current = end;
                    previous_quad_control = Some(control);
                    previous_cubic_control = None;
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG Q command is missing coordinates"));
                }
            }

            'T' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let control = previous_quad_control
                        .map(|c| Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y))
                        .unwrap_or(current);

                    let end = read_path_point(&mut p, current, relative)?;

                    flatten_quadratic(current, control, end, &mut active);

                    current = end;
                    previous_quad_control = Some(control);
                    previous_cubic_control = None;
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG T command is missing coordinates"));
                }
            }

            'A' => {
                require_active_path(&mut active, current);

                let mut any = false;
                while p.has_number() {
                    let rx = p.number()?.abs();
                    let ry = p.number()?.abs();
                    let rotation = p.number()?;
                    let large_arc = p.flag()?;
                    let sweep = p.flag()?;
                    let end = read_path_point(&mut p, current, relative)?;

                    flatten_arc(
                        current,
                        rx,
                        ry,
                        rotation,
                        large_arc,
                        sweep,
                        end,
                        &mut active,
                    );

                    current = end;
                    previous_cubic_control = None;
                    previous_quad_control = None;
                    any = true;
                }

                if !any {
                    return Err(ImageError::new("SVG A command is missing coordinates"));
                }
            }

            'Z' => {
                if !active.is_empty() {
                    if !_same_point(current, sub_start) {
                        active.push(sub_start);
                    }

                    current = sub_start;
                    paths.push(std::mem::take(&mut active));
                }

                previous_cubic_control = None;
                previous_quad_control = None;
                command = None;
            }

            _ => {
                return Err(ImageError::new(format!(
                    "Unsupported SVG path command '{cmd}'"
                )));
            }
        }
    }

    if !active.is_empty() {
        paths.push(active);
    }

    Ok(paths)
}

/// Reads path point from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_path_point(
    p: &mut PathParser<'_>,
    current: Point,
    relative: bool,
) -> Result<Point, ImageError> {
    let x = p.number()?;
    let y = p.number()?;

    Ok(if relative {
        Point::new(current.x + x, current.y + y)
    } else {
        Point::new(x, y)
    })
}

/// Ensures that the current SVG subpath contains an initial point before appending geometry.
fn require_active_path(path: &mut Vec<Point>, current: Point) {
    if path.is_empty() {
        path.push(current);
    }
}

/// Approximates cubic geometry with line segments suitable for rasterization.
fn flatten_cubic(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    out: &mut Vec<Point>,
) {
    let estimate =
        distance(p0, p1) + distance(p1, p2) + distance(p2, p3);

    let steps = ((estimate / 4.0).ceil() as usize).clamp(8, 96);

    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let mt = 1.0 - t;

        let x = mt * mt * mt * p0.x
            + 3.0 * mt * mt * t * p1.x
            + 3.0 * mt * t * t * p2.x
            + t * t * t * p3.x;

        let y = mt * mt * mt * p0.y
            + 3.0 * mt * mt * t * p1.y
            + 3.0 * mt * t * t * p2.y
            + t * t * t * p3.y;

        out.push(Point::new(x, y));
    }
}

/// Approximates quadratic geometry with line segments suitable for rasterization.
fn flatten_quadratic(
    p0: Point,
    p1: Point,
    p2: Point,
    out: &mut Vec<Point>,
) {
    let estimate = distance(p0, p1) + distance(p1, p2);
    let steps = ((estimate / 4.0).ceil() as usize).clamp(6, 64);

    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let mt = 1.0 - t;

        out.push(Point::new(
            mt * mt * p0.x + 2.0 * mt * t * p1.x + t * t * p2.x,
            mt * mt * p0.y + 2.0 * mt * t * p1.y + t * t * p2.y,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
/// Approximates arc geometry with line segments suitable for rasterization.
fn flatten_arc(
    start: Point,
    mut rx: f64,
    mut ry: f64,
    rotation_degrees: f64,
    large_arc: bool,
    sweep: bool,
    end: Point,
    out: &mut Vec<Point>,
) {
    if rx <= 1e-12
        || ry <= 1e-12
        || _same_point(start, end)
    {
        out.push(end);
        return;
    }

    let phi = rotation_degrees.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    let dx2 = (start.x - end.x) * 0.5;
    let dy2 = (start.y - end.y) * 0.5;

    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;

    let lambda = x1p * x1p / (rx * rx)
        + y1p * y1p / (ry * ry);

    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;

    let numerator = (rx2 * ry2
        - rx2 * y1p2
        - ry2 * x1p2)
        .max(0.0);

    let denominator = rx2 * y1p2 + ry2 * x1p2;

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let factor = if denominator <= 1e-30 {
        0.0
    } else {
        sign * (numerator / denominator).sqrt()
    };

    let cxp = factor * (rx * y1p / ry);
    let cyp = factor * (-ry * x1p / rx);

    let cx = cos_phi * cxp
        - sin_phi * cyp
        + (start.x + end.x) * 0.5;

    let cy = sin_phi * cxp
        + cos_phi * cyp
        + (start.y + end.y) * 0.5;

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;

    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;

    let theta1 = uy.atan2(ux);
    let mut delta = vector_angle(ux, uy, vx, vy);

    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }

    let arc_length_est = delta.abs() * rx.max(ry);
    let steps = ((arc_length_est / 4.0).ceil() as usize).clamp(8, 128);

    for i in 1..=steps {
        let theta = theta1 + delta * (i as f64 / steps as f64);

        let ct = theta.cos();
        let st = theta.sin();

        out.push(Point::new(
            cx + cos_phi * rx * ct - sin_phi * ry * st,
            cy + sin_phi * rx * ct + cos_phi * ry * st,
        ));
    }
}

/// Computes the signed angle between two two-dimensional vectors.
fn vector_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    let dot = ux * vx + uy * vy;
    let det = ux * vy - uy * vx;
    det.atan2(dot)
}

// ============================================================================
// XML-ish parsing
// ============================================================================

type Attributes = std::collections::HashMap<String, String>;

/// Searches the supplied input for root svg and returns its location or value when present.
fn find_root_svg(text: &str) -> Option<&str> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0usize;

    while let Some(rel) = lower[from..].find("<svg") {
        let start = from + rel;

        let after = lower.as_bytes().get(start + 4).copied();
        if !matches!(
            after,
            Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') | Some(b'>') | Some(b'/')
        ) {
            from = start + 4;
            continue;
        }

        let end = find_tag_end(text, start + 1)?;
        return Some(text[start + 1..end].trim());
    }

    None
}

/// Searches the supplied input for tag end and returns its location or value when present.
fn find_tag_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;

    for i in start..bytes.len() {
        let b = bytes[i];

        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'\'' || b == b'"' => quote = Some(b),
            None if b == b'>' => return Some(i),
            None => {}
        }
    }

    None
}

/// Splits tag name into the component parts required by the parser.
fn split_tag_name(content: &str) -> (&str, &str) {
    let end = content
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(content.len());

    (&content[..end], content[end..].trim_start())
}

/// Parses attributes from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_attributes(content: &str) -> Result<Attributes, ImageError> {
    let (_, mut rest) = split_tag_name(content);
    let mut attrs = Attributes::new();

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() || rest == "/" {
            break;
        }

        let name_end = rest
            .find(|c: char| {
                c.is_ascii_whitespace() || c == '=' || c == '/'
            })
            .unwrap_or(rest.len());

        if name_end == 0 {
            break;
        }

        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();

        if !rest.starts_with('=') {
            // XML requires '=', but silently ignore malformed boolean-ish attrs
            // instead of making the entire image unusable.
            attrs.insert(name, String::new());
            continue;
        }

        rest = rest[1..].trim_start();

        let quote = rest.as_bytes().first().copied();
        let value;

        if matches!(quote, Some(b'"') | Some(b'\'')) {
            let q = quote.unwrap();
            let body = &rest[1..];

            let end = body
                .as_bytes()
                .iter()
                .position(|&b| b == q)
                .ok_or_else(|| ImageError::new("Unterminated SVG attribute"))?;

            value = decode_xml_entities(&body[..end]);
            rest = &body[end + 1..];
        } else {
            let end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '/')
                .unwrap_or(rest.len());

            value = decode_xml_entities(&rest[..end]);
            rest = &rest[end..];
        }

        attrs.insert(name, value);
    }

    Ok(attrs)
}

/// Decodes xml entities data into the representation required by the surrounding decoder.
fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

// ============================================================================
// Style parsing
// ============================================================================

/// Applies style attributes to the supplied decoding or rendering state.
///
/// Returns an error when the supplied data cannot be processed as required.
fn apply_style_attributes(
    style: &mut Style,
    attrs: &Attributes,
) -> Result<(), ImageError> {
    // Apply presentation attributes without cloning the complete attribute map.
    // Inline `style="..."` declarations are processed second so CSS-style
    // declarations retain their expected precedence.
    for key in [
        "fill",
        "stroke",
        "stroke-width",
        "opacity",
        "fill-opacity",
        "stroke-opacity",
        "fill-rule",
    ] {
        if let Some(value) = attrs.get(key) {
            apply_style_property(style, key, value)?;
        }
    }

    if let Some(inline) = attrs.get("style") {
        for item in inline.split(';') {
            let Some((key, value)) = item.split_once(':') else {
                continue;
            };
            apply_style_property(style, key.trim(), value.trim())?;
        }
    }

    Ok(())
}

#[inline]
fn apply_style_property(
    style: &mut Style,
    key: &str,
    value: &str,
) -> Result<(), ImageError> {
    if key.eq_ignore_ascii_case("fill") {
        style.fill = parse_paint(value)?;
    } else if key.eq_ignore_ascii_case("stroke") {
        style.stroke = parse_paint(value)?;
    } else if key.eq_ignore_ascii_case("stroke-width") {
        if let Some(v) = _parse_length(value) {
            style.stroke_width = v.max(0.0);
        }
    } else if key.eq_ignore_ascii_case("opacity") {
        style.opacity *= parse_opacity(value)?;
    } else if key.eq_ignore_ascii_case("fill-opacity") {
        style.fill_opacity = parse_opacity(value)?;
    } else if key.eq_ignore_ascii_case("stroke-opacity") {
        style.stroke_opacity = parse_opacity(value)?;
    } else if key.eq_ignore_ascii_case("fill-rule") {
        style.fill_rule = if value.trim().eq_ignore_ascii_case("evenodd") {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        };
    }
    Ok(())
}

/// Parses opacity from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_opacity(value: &str) -> Result<f64, ImageError> {
    let value = value.trim();

    if let Some(percent) = value.strip_suffix('%') {
        let v = percent
            .trim()
            .parse::<f64>()
            .map_err(|_| ImageError::new("Invalid SVG opacity"))?;

        return Ok((v / 100.0).clamp(0.0, 1.0));
    }

    let v = value
        .parse::<f64>()
        .map_err(|_| ImageError::new("Invalid SVG opacity"))?;

    Ok(v.clamp(0.0, 1.0))
}

/// Parses paint from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_paint(value: &str) -> Result<Option<Color>, ImageError> {
    let value = value.trim().to_ascii_lowercase();

    if value == "none" {
        return Ok(None);
    }

    if value.starts_with("url(") {
        // Gradients and paint servers are not implemented yet.
        return Ok(None);
    }

    parse_color(&value)
        .map(Some)
        .ok_or_else(|| ImageError::new(format!("Unsupported SVG color '{value}'")))
}

/// Parses color from the supplied encoded or textual input.
fn parse_color(value: &str) -> Option<Color> {
    let v = value.trim();

    if let Some(hex) = v.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = hex_nibble(hex.as_bytes()[0])?;
                let g = hex_nibble(hex.as_bytes()[1])?;
                let b = hex_nibble(hex.as_bytes()[2])?;

                Some(Color {
                    r: r * 17,
                    g: g * 17,
                    b: b * 17,
                    a: 255,
                })
            }

            4 => {
                let r = hex_nibble(hex.as_bytes()[0])?;
                let g = hex_nibble(hex.as_bytes()[1])?;
                let b = hex_nibble(hex.as_bytes()[2])?;
                let a = hex_nibble(hex.as_bytes()[3])?;

                Some(Color {
                    r: r * 17,
                    g: g * 17,
                    b: b * 17,
                    a: a * 17,
                })
            }

            6 => Some(Color {
                r: u8::from_str_radix(&hex[0..2], 16).ok()?,
                g: u8::from_str_radix(&hex[2..4], 16).ok()?,
                b: u8::from_str_radix(&hex[4..6], 16).ok()?,
                a: 255,
            }),

            8 => Some(Color {
                r: u8::from_str_radix(&hex[0..2], 16).ok()?,
                g: u8::from_str_radix(&hex[2..4], 16).ok()?,
                b: u8::from_str_radix(&hex[4..6], 16).ok()?,
                a: u8::from_str_radix(&hex[6..8], 16).ok()?,
            }),

            _ => None,
        };
    }

    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() == 3 {
            return Some(Color {
                r: parse_color_component(parts[0])?,
                g: parse_color_component(parts[1])?,
                b: parse_color_component(parts[2])?,
                a: 255,
            });
        }
    }

    match v {
        "black" => rgb(0, 0, 0),
        "silver" => rgb(192, 192, 192),
        "gray" | "grey" => rgb(128, 128, 128),
        "white" => rgb(255, 255, 255),
        "maroon" => rgb(128, 0, 0),
        "red" => rgb(255, 0, 0),
        "purple" => rgb(128, 0, 128),
        "fuchsia" | "magenta" => rgb(255, 0, 255),
        "green" => rgb(0, 128, 0),
        "lime" => rgb(0, 255, 0),
        "olive" => rgb(128, 128, 0),
        "yellow" => rgb(255, 255, 0),
        "navy" => rgb(0, 0, 128),
        "blue" => rgb(0, 0, 255),
        "teal" => rgb(0, 128, 128),
        "aqua" | "cyan" => rgb(0, 255, 255),
        "orange" => rgb(255, 165, 0),
        "transparent" => Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }),
        _ => None,
    }
}

/// Parses color component from the supplied encoded or textual input.
fn parse_color_component(value: &str) -> Option<u8> {
    if let Some(percent) = value.strip_suffix('%') {
        let p = percent.trim().parse::<f64>().ok()?;
        return Some((p.clamp(0.0, 100.0) * 2.55).round() as u8);
    }

    let v = value.parse::<f64>().ok()?;
    Some(v.round().clamp(0.0, 255.0) as u8)
}

/// Constructs an opaque SVG color from red, green, and blue channel values.
fn rgb(r: u8, g: u8, b: u8) -> Option<Color> {
    Some(Color { r, g, b, a: 255 })
}

/// Converts one ASCII hexadecimal digit to its four-bit numeric value.
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ============================================================================
// Transform parsing
// ============================================================================

/// Parses transform from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_transform(value: &str) -> Result<Affine, ImageError> {
    let mut rest = value.trim();
    let mut result = Affine::identity();

    while !rest.is_empty() {
        rest = rest.trim_start_matches(|c: char| {
            c.is_ascii_whitespace() || c == ','
        });

        if rest.is_empty() {
            break;
        }

        let open = rest
            .find('(')
            .ok_or_else(|| ImageError::new("Invalid SVG transform"))?;

        let name = rest[..open].trim().to_ascii_lowercase();

        let close = rest[open + 1..]
            .find(')')
            .map(|v| open + 1 + v)
            .ok_or_else(|| ImageError::new("Unterminated SVG transform"))?;

        let args = _parse_number_list(&rest[open + 1..close])?;

        let t = match name.as_str() {
            "matrix" if args.len() == 6 => Affine {
                a: args[0],
                b: args[1],
                c: args[2],
                d: args[3],
                e: args[4],
                f: args[5],
            },

            "translate" if args.len() == 1 => {
                Affine::translate(args[0], 0.0)
            }

            "translate" if args.len() >= 2 => {
                Affine::translate(args[0], args[1])
            }

            "scale" if args.len() == 1 => {
                Affine::scale(args[0], args[0])
            }

            "scale" if args.len() >= 2 => {
                Affine::scale(args[0], args[1])
            }

            "rotate" if args.len() == 1 => {
                Affine::rotate(args[0].to_radians())
            }

            "rotate" if args.len() >= 3 => {
                Affine::translate(args[1], args[2])
                    .mul(Affine::rotate(args[0].to_radians()))
                    .mul(Affine::translate(-args[1], -args[2]))
            }

            "skewx" if args.len() == 1 => Affine {
                c: args[0].to_radians().tan(),
                ..Affine::identity()
            },

            "skewy" if args.len() == 1 => Affine {
                b: args[0].to_radians().tan(),
                ..Affine::identity()
            },

            _ => {
                return Err(ImageError::new(format!(
                    "Unsupported or invalid SVG transform '{name}'"
                )));
            }
        };

        result = result.mul(t);
        rest = &rest[close + 1..];
    }

    Ok(result)
}

// ============================================================================
// Geometry helpers
// ============================================================================

/// Generates polygon points that approximate an ellipse for rasterization.
fn ellipse_points(
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
) -> Vec<Point> {
    let circumference =
        std::f64::consts::TAU * ((rx * rx + ry * ry) * 0.5).sqrt();

    let segments = ((circumference / 4.0).ceil() as usize).clamp(24, 256);
    let mut out = Vec::with_capacity(segments + 1);

    for i in 0..=segments {
        let theta = std::f64::consts::TAU * i as f64 / segments as f64;
        out.push(Point::new(
            cx + rx * theta.cos(),
            cy + ry * theta.sin(),
        ));
    }

    out
}

/// Generates polygon points that approximate an SVG rectangle with rounded corners.
fn rounded_rect_points(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rx: f64,
    ry: f64,
) -> Vec<Point> {
    if rx <= 0.0 || ry <= 0.0 {
        return vec![
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
            Point::new(x, y),
        ];
    }

    let mut out = Vec::with_capacity(37);

    append_corner_arc(&mut out, x + w - rx, y + ry, rx, ry, -90.0, 0.0);
    append_corner_arc(&mut out, x + w - rx, y + h - ry, rx, ry, 0.0, 90.0);
    append_corner_arc(&mut out, x + rx, y + h - ry, rx, ry, 90.0, 180.0);
    append_corner_arc(&mut out, x + rx, y + ry, rx, ry, 180.0, 270.0);

    if let Some(first) = out.first().copied() {
        out.push(first);
    }

    out
}

/// Appends sampled points for one rounded-rectangle corner arc.
fn append_corner_arc(
    out: &mut Vec<Point>,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    start_deg: f64,
    end_deg: f64,
) {
    const STEPS: usize = 8;

    for i in 0..=STEPS {
        if !out.is_empty() && i == 0 {
            continue;
        }

        let t = i as f64 / STEPS as f64;
        let theta = (start_deg + (end_deg - start_deg) * t).to_radians();

        out.push(Point::new(
            cx + rx * theta.cos(),
            cy + ry * theta.sin(),
        ));
    }
}

/// Applies an affine transform to every point in the supplied point list.
fn transform_points(
    points: Vec<Point>,
    transform: Affine,
) -> Vec<Point> {
    points
        .into_iter()
        .map(|p| transform.apply(p))
        .collect()
}

#[inline]
/// Computes the Euclidean distance between two SVG points.
fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

#[inline]
/// Returns whether two SVG points are equal within the rasterizer's numeric tolerance.
fn _same_point(a: Point, b: Point) -> bool {
    _nearly_equal(a.x, b.x) && _nearly_equal(a.y, b.y)
}

#[inline]
/// Returns whether two floating-point values are equal within a small tolerance.
fn _nearly_equal(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

// ============================================================================
// Number / length helpers
// ============================================================================

/// Reads a numeric SVG attribute, applying the supplied default when the attribute is absent.
///
/// Returns an error when the supplied data cannot be processed as required.
fn _attr_num(
    attrs: &Attributes,
    name: &str,
    default: f64,
) -> Result<f64, ImageError> {
    match attrs.get(name) {
        Some(value) => _parse_length(value)
            .ok_or_else(|| ImageError::new(format!("Invalid SVG {name} value"))),
        None => Ok(default),
    }
}

/// Parses length from the supplied encoded or textual input.
fn _parse_length(value: &str) -> Option<f64> {
    let text = value.trim();

    if text.ends_with('%') {
        return None;
    }

    let mut end = 0usize;
    let bytes = text.as_bytes();

    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        end += 1;
    }

    let mut digits = false;

    while matches!(bytes.get(end), Some(b'0'..=b'9')) {
        end += 1;
        digits = true;
    }

    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while matches!(bytes.get(end), Some(b'0'..=b'9')) {
            end += 1;
            digits = true;
        }
    }

    if !digits {
        return None;
    }

    if matches!(bytes.get(end), Some(b'e') | Some(b'E')) {
        end += 1;

        if matches!(bytes.get(end), Some(b'+') | Some(b'-')) {
            end += 1;
        }

        let exponent_start = end;
        while matches!(bytes.get(end), Some(b'0'..=b'9')) {
            end += 1;
        }

        if end == exponent_start {
            return None;
        }
    }

    let number = text[..end].parse::<f64>().ok()?;
    let unit = text[end..].trim().to_ascii_lowercase();

    match unit.as_str() {
        "" | "px" => Some(number),
        "pt" => Some(number * 96.0 / 72.0),
        "pc" => Some(number * 16.0),
        "in" => Some(number * 96.0),
        "cm" => Some(number * 96.0 / 2.54),
        "mm" => Some(number * 96.0 / 25.4),
        "q" => Some(number * 96.0 / 101.6),
        _ => None,
    }
}

/// Parses number list from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn _parse_number_list(value: &str) -> Result<Vec<f64>, ImageError> {
    let mut p = PathParser::new(value);
    let mut values = Vec::new();

    while p.has_number() {
        values.push(p.number()?);
    }

    p.skip_separators();

    if p.pos != p.bytes.len() {
        return Err(ImageError::new("Invalid SVG number list"));
    }

    Ok(values)
}

// ============================================================================
// Pixel helpers
// ============================================================================

#[inline]
/// Unpacks rgba from the compact pixel representation into separate components.
fn _unpack_rgba(pixel: u32) -> Color {
    Color {
        r: (pixel >> 24) as u8,
        g: (pixel >> 16) as u8,
        b: (pixel >> 8) as u8,
        a: pixel as u8,
    }
}
