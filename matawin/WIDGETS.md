# MataWin widgets

The widget layer is now software-rendered by MataWin. GTK and native Win32 child controls are not used for ordinary widgets.

## Implementation boundary

Ordinary widgets use the Mata ecosystem:

- `matawin::Window` for native window/input handling
- `matawin::Framebuffer` for drawing/presentation
- `matafonts` for TrueType decoding, rasterization, advance widths, bearings, baseline and line height
- `mataimage` for image/SVG decoding
- `mata-audio` for streamed audio playback sources

`WebView` is the only platform-toolkit exception and is compiled only with the opt-in `webview` Cargo feature:

- Windows: Microsoft WebView2 through the `windows`-crate COM bridge
- Linux: WebKitGTK 4.1 and the minimal GTK host required by WebKitGTK

No ordinary `Button`, `TextField`, label, list, slider, image, media-control, or layout widget creates a GTK or Win32 native control.

## Button and text field

```rust
use matawin::{Window, widgets::{Anchors, Button, Rect, TextField, WidgetEvent, WidgetWindow}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native_window = Window::new("Mata widgets", (720, 480));
    let window = WidgetWindow::from_window(native_window)?;
    let field = TextField::new(&window, Rect::new(16, 16, 300, 36), "MataWin")?;
    let button = Button::new(&window, Rect::new(328, 16, 100, 36), "Submit")?;

    field.set_anchors(Anchors::TOP_HORIZONTAL);
    button.set_anchors(Anchors::TOP_RIGHT);
    field.focus();

    let mut events = Vec::new();
    loop {
        window.poll_events_into(&mut events);
        for event in &events {
            match event {
                WidgetEvent::Close => return Ok(()),
                WidgetEvent::ButtonClicked(id) if *id == button.id() => println!("{}", field.text()),
                _ => {}
            }
        }
        window.present();
    }
}
```


## Automatic resizing

`WidgetWindow` updates widget geometry automatically when its underlying `Window` is resized, maximized, or restored. `Button`, `TextField`, and `WebView` support `set_anchors(...)` and `set_resize_policy(...)`.

Anchors preserve the widget's original margins relative to its initial window size:

- `Anchors::TOP_LEFT` keeps a fixed-size control at the top-left.
- `Anchors::TOP_RIGHT` keeps a fixed-size control attached to the right edge.
- `Anchors::TOP_HORIZONTAL` stretches the control horizontally while keeping its top/left/right margins.
- `Anchors::LEFT_VERTICAL` stretches vertically.
- `Anchors::ALL` stretches in both dimensions while preserving all four margins.
- `ResizePolicy::Proportional` scales the widget's position and dimensions with the entire client area.

The resize policy is evaluated from a stable layout basis rather than repeatedly applying deltas to the most recent rectangle, so repeatedly maximizing/restoring a window does not accumulate rounding or drift. Calling `set_bounds(...)` establishes a new basis at the window's current size.

```rust
use matawin::widgets::{Anchors, ResizePolicy};

field.set_anchors(Anchors::TOP_HORIZONTAL);
button.set_anchors(Anchors::TOP_RIGHT);
webview.set_anchors(Anchors::ALL);

// Alternatively:
button.set_resize_policy(ResizePolicy::Proportional);
```

Applications still receive `WidgetEvent::Resized(width, height)` for custom layout work, but ordinary anchored controls do not require application-side resize calculations.

## Fonts

`FontRenderer` decodes TTF data with `matafonts::decoders::ttf::Ttf` and caches glyphs by `(point_size, char)`. Layout uses the decoded `advance_width`, `bearing_x`, `bearing_y`, `baseline`, and `line_height` values rather than treating the cropped glyph bitmap as its advance.

`WidgetWindow` tries common system TTF paths with normal filesystem access. Applications can provide a specific font without any platform font API:

```rust
window.set_font_file("assets/LiberationSans-Regular.ttf")?;
```

or use `set_font_bytes`.

The text-oriented catalog widgets (`Label`, `Entry`, `TextView`, `Inscription`, and related types) expose `blit(...)` so they can rasterize through the same `FontRenderer` into a MataWin framebuffer.

## Images

`Image`, `Picture`, and `SvgWidget` decode through `mataimage`. MataWin consumes `mataimage::Image::pixels` as the primary display frame (additional animation frames remain separate in mataimage), converts the pixels from logical `0xRRGGBBAA` to MataWin's framebuffer representation in place, and can blit/scale them without a GUI toolkit. The crate enables mataimage's `svg` feature because SVG is no longer part of mataimage's default stable-format feature set.

## Audio

`matawin::widgets` no longer imports CPAL directly. It works with `mata-audio::AudioSource`.

The supplied streamed mixer was moved behind the `mata-audio` API, preserving:

- one shared output stream
- incremental `mata-audio` decoding
- per-source play/pause
- absolute seek
- per-source volume
- looping
- playback position/duration state
- lock-free mixed PCM handoff to the output callback

CPAL remains an implementation detail of `mata-audio` because it is the requested audio-output backend. The widget crate itself only sees the Mata audio API.

## WebView

WebView support is disabled by default. Enable it with `features = ["webview"]`. The `browser` example also requires that feature.

The WebView is deliberately isolated from the software controls. On Windows it embeds WebView2 into the MataWin window. On Linux WebKitGTK requires a GTK widget host; therefore the Linux WebView backend owns the minimal GTK host needed by WebKitGTK. GTK is not used to implement any other widget.

On Android the software widget catalog is available, but the optional `webview` feature is not implemented yet. Enabling `webview` for an Android target produces an explicit compile error instead of failing later on an unresolved desktop backend.


## Styling

All `WidgetExt` catalog widgets and the live `Button`/`TextField` controls share the style engine documented in `STYLING.md`. Live controls automatically apply viewport units, box-model properties, z-index, interactive pseudo-classes, media queries, opacity, and keyframe animation during `WidgetWindow::render()`.
