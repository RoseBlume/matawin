# matawin

`matawin` is a small native software-framebuffer windowing crate for Windows, Linux, and Android.

It provides native window creation, `Framebuffer` presentation, keyboard/mouse/window events, and basic native window operations. Higher-level game features belong in `mata-engine`.

## Fast loop

For a render/game loop, reuse one event vector instead of calling the allocation-returning compatibility helper every frame:

```rust
use matawin::{Framebuffer, Window, events::WindowEvent};

fn main() {
    let mut window = Window::new("matawin", (800, 600));
    let framebuffer = Framebuffer::from_color(800, 600, 0xff10_1820);
    let mut events = Vec::with_capacity(16);

    loop {
        window.poll_events_into(&mut events);

        for event in events.iter().copied() {
            if matches!(event, WindowEvent::Close) {
                return;
            }
        }

        window.present(&framebuffer);
    }
}
```

`poll_events()` is still available for source compatibility, but it returns a new `Vec` each call. `is_key_down(key)` is available when direct held-state checks are cheaper/more convenient than consuming `KeyHeld` events.

## Rendering paths

- **Windows:** persistent Direct2D render target + bitmap, one framebuffer upload and one nearest-neighbor draw per frame.
- **Wayland:** triple-buffered `wl_shm` buffers backed by persistent memory maps. Frames are written directly into compositor-visible shared memory instead of being copied to a temporary vector and then written with file I/O.
- **X11:** the normal 32-bit little-endian TrueColor layout uses the framebuffer bytes directly when no scaling is required. Resizing uses cached nearest-neighbor lookup maps; unusual visuals use cached component conversion tables.
- **Android:** the Activity-owned `ANativeWindow` is locked as RGBA8888 and filled directly from the software framebuffer. Touch is exposed natively and the primary contact is mirrored to the existing pointer/button events for widget compatibility.

## Linux

Both Linux backends are enabled by default. Wayland is attempted first and X11 is used as a fallback.

```toml
matawin = { path = "../matawin" }
```

Build only one backend when desired:

```toml
matawin = { path = "../matawin", default-features = false, features = ["wayland"] }
```

or:

```toml
matawin = { path = "../matawin", default-features = false, features = ["x11"] }
```


## Android

Android uses the Activity-owned native surface rather than creating a second top-level window. Construct the MataWin window from `android_main`:

```rust
use matawin::{AndroidApp, Window};

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let mut window = Window::from_android_app(app, "matawin", (1280, 720));
    // Poll and render here; return after WindowEvent::Close.
}
```

Android-only dependencies are target-gated in `Cargo.toml`. Touch, lifecycle events, NDK software presentation, and Android system-font discovery are wired into the existing APIs. See `ANDROID.md` for packaging, manifest, input, and platform-behavior details. The optional `webview` feature remains desktop-only.

## Widget styling and optional WebView

The widget layer includes a CSS-like style engine with `px`, `%`, `vw`, `vh`, `vmin`, `vmax`, `em`, and `rem` units; margin/padding; elliptical border radii; relative/absolute positioning; z-index; typography; opacity; pseudo-classes; media queries; and keyframe animations. See `STYLING.md` and `examples/styling.rs`.

WebView support is **off by default**. Enable it explicitly when needed:

```toml
matawin = { path = "../matawin", features = ["webview"] }
```
## Source documentation

All Rust modules, structs, enums, enum variants, traits, fields, functions, and methods are documented inline, including private/internal implementation items. `API_REFERENCE.md` provides an index that explicitly records each declaration's source visibility.

