# MataWin typed widget styling

MataWin now exposes widget styling as typed Rust data rather than a public CSS-declaration API. `StyleProperties` contains the visual/layout properties, `AnimationSpec` contains animation properties, `MediaQuery` contains media conditions, and `StyleRule`/the `Stylable` helpers apply conditional properties.

All catalog widgets implementing `WidgetExt`, plus live `Button` and `TextField`, implement `Stylable`. `WebView` does as well when the opt-in `webview` feature is enabled.

## Typed lengths

Use `Length` directly:

```rust
use matawin::widgets::{Length, Stylable};

button.set_width(Length::Vw(40.0))?;
button.set_height(Length::Rem(3.0))?;
button.set_left(Length::Percent(5.0))?;
button.set_font_size(Length::Em(1.1))?;
```

Supported units are `Px`, `Percent`, `Vw`, `Vh`, `Vmin`, `Vmax`, `Em`, `Rem`, and `Auto` where applicable.

## Set several properties at once

`StyleProperties` stores the properties as fields. Core controls also expose `set_style_properties` and `update_style` as inherent methods.

```rust
use matawin::widgets::{Edges, FontPatch, Length, StyleProperties};

field.set_style_properties(StyleProperties {
    width: Some(Length::Vw(48.0)),
    height: Some(Length::Rem(3.0)),
    padding: Some(Edges::vertical_horizontal(Length::Rem(0.45), Length::Rem(0.7))),
    font: FontPatch { size: Some(Length::Rem(1.05)), ..FontPatch::default() },
    background_color: Some(0xff17_1c24),
    ..StyleProperties::default()
});

field.update_style(|style| {
    style.opacity = Some(0.9);
    style.z_index = Some(4);
});
```

The older declaration-string setters are no longer part of the public `Stylable` API. Parsing remains internal for compatibility helpers such as the optional keyframe parser.

## Margin and padding

`Edges` provides typed shorthand equivalents:

```rust
style.margin = Some(Edges::all(Length::Px(8.0)));
style.padding = Some(Edges::vertical_horizontal(Length::Rem(0.5), Length::Rem(1.0)));
style.margin = Some(Edges::trbl(
    Length::Px(4.0), Length::Vw(1.0), Length::Px(8.0), Length::Vw(1.0),
));
```

`Stylable` also provides `set_margin_top`, `set_margin_right`, `set_margin_bottom`, `set_margin_left`, and matching padding methods.

## Border radii

`BorderRadius` supports all four corners, the usual 1-4 value patterns, and elliptical radii without string parsing:

```rust
use matawin::widgets::{BorderRadius, Length};

let uniform = BorderRadius::all(Length::Px(12.0));
let two = BorderRadius::two(Length::Px(14.0), Length::Px(5.0));
let four = BorderRadius::four(
    Length::Px(14.0), Length::Px(5.0), Length::Px(10.0), Length::Px(2.0),
);
let elliptical = BorderRadius::elliptical(
    [Length::Px(14.0), Length::Px(5.0), Length::Px(14.0), Length::Px(5.0)],
    [Length::Px(10.0), Length::Px(5.0), Length::Px(10.0), Length::Px(5.0)],
);
```

Individual corners can be changed with the `set_border_*_radius` methods or by editing the `border_radius` field.

## Positioning and z-index

`StyleProperties` contains `position`, `top`, `right`, `bottom`, `left`, `relative_to`, and `z_index`. `relative_to` stores another `WidgetId` and is recomputed during responsive layout.

```rust
button.set_style_properties(StyleProperties {
    position: Some(PositionMode::Absolute),
    relative_to: Some(Some(field.id())),
    left: Some(Length::Percent(100.0)),
    top: Some(Length::Px(0.0)),
    margin: Some(Edges::trbl(
        Length::Px(0.0), Length::Px(0.0), Length::Px(0.0), Length::Vw(1.0),
    )),
    z_index: Some(5),
    ..StyleProperties::default()
});
```

For live controls, z-index is used for both paint order and hit testing.

## Typography

`FontPatch` exposes `family`, `size`, `style`, `variant`, `weight`, `stretch`, and `line_height`. Matching `Stylable` methods are available for changing each property individually.

## Opacity and transparency

Use `StyleProperties::opacity` or `set_opacity`. `set_transparency(v)` stores `1.0 - v`. Values are clamped to `0..=1`.

## Pseudo-classes

Interactive and structural states are configured with typed property patches:

```rust
button.set_hover_style(StyleProperties {
    background_color: Some(0xff45_5a72),
    opacity: Some(0.92),
    ..StyleProperties::default()
});

field.set_focus_style(StyleProperties {
    border_color: Some(0xff7f_b7ff),
    border_radius: Some(BorderRadius::all(Length::Px(18.0))),
    ..StyleProperties::default()
});
```

Typed helpers exist for hover, focus, active, link, visited, first-child, last-child, nth-child, and language styles. More complex combinations can use `set_pseudo_style(PseudoSelector, StyleProperties)`.

## Media queries

`MediaQuery` is also a normal Rust struct:

```rust
button.add_media_style(
    MediaQuery {
        max_width: Some(Length::Px(700.0)),
        orientation: Some(MediaOrientation::Portrait),
        ..MediaQuery::default()
    },
    StyleProperties {
        width: Some(Length::Vw(90.0)),
        font: FontPatch { size: Some(Length::Vw(4.0)), ..FontPatch::default() },
        ..StyleProperties::default()
    },
);
```

## Keyframe animations

`AnimationSpec` directly stores `name`, `duration`, `delay`, and `iteration_count`. Keyframes accept `StyleProperties`:

```rust
let pulse = Keyframes::new("pulse")
    .with_frame(0.0, StyleProperties { opacity: Some(0.55), ..StyleProperties::default() })
    .with_frame(0.5, StyleProperties { opacity: Some(1.0), ..StyleProperties::default() })
    .with_frame(1.0, StyleProperties { opacity: Some(0.55), ..StyleProperties::default() });

button.add_keyframes(pulse);
button.set_animation(AnimationSpec {
    name: "pulse".into(),
    duration: Duration::from_millis(1200),
    delay: Duration::from_millis(100),
    iteration_count: AnimationIterationCount::Count(1.0),
});
```

## Rendering and performance

`WidgetWindow::present()` is invalidation-driven. A static focused window is not rerasterized or uploaded to the GPU merely because the application keeps calling `present()` in its loop. Hover, focus, typing, setters, resize, and explicit `request_redraw()` invalidate the surface. Active animations are capped to roughly 60 presentation frames per second.

The render path also reuses z-order/reference scratch allocations instead of cloning the complete control list and reference map each frame. Glyph drawing no longer clones rasterized glyph pixel buffers for every character, responsive-font glyph caching is bounded, and a framebuffer releases oversized retained capacity after a large downsize.

On Windows Direct2D presentation is compositor-paced instead of forcing `PRESENT_OPTIONS_IMMEDIATELY`, while the 96-DPI 1:1 native-pixel path is retained for sharp resize/maximize rendering and correct input coordinates.

If direct framebuffer access is needed, prefer `WidgetWindow::with_framebuffer` to `framebuffer()`: the latter intentionally returns an owned copy and therefore duplicates the full pixel buffer.

## WebView feature

WebView support remains disabled by default. Enable it explicitly with:

```toml
matawin = { path = "../matawin", features = ["webview"] }
```
