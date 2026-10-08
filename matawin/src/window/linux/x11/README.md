# X11 backend

This folder is an X11 counterpart to the supplied Windows and Wayland backends.
It uses **x11rb's pure-Rust `RustConnection`** rather than Xlib/XCB FFI, so the
Rust crate itself does not need X11 development headers or a native Xlib link.

It does **not require sudo/root privileges**. The backend connects to the user's
existing X server through the normal `DISPLAY` session connection. Keyboard and
mouse events are X11 window events, so it does not read `/dev/input` directly.

## Rendering

`renderer.rs` presents the software framebuffer with core X11 `PutImage` calls.
It discovers the root visual's RGB masks, pixel depth, bits-per-pixel, byte order,
and scanline padding from the X11 setup data. The source layout matches the
Windows backend: B, G, R, A/X bytes. When the native window differs from the
logical framebuffer size, nearest-neighbor scaling is performed before upload.
Large images are split into row chunks so ordinary X11 request-size limits are
not exceeded.

## Window management

* close uses `WM_PROTOCOLS` / `WM_DELETE_WINDOW`;
* maximize/restore use EWMH `_NET_WM_STATE` requests;
* minimize uses ICCCM `WM_CHANGE_STATE`;
* resize/reposition use `ConfigureWindow`;
* resizable size constraints use `WM_NORMAL_HINTS`;
* decoration/minimize/maximize capability hints use `_MOTIF_WM_HINTS` where the
  window manager supports them.

`set_icon(path)` remains a compatibility no-op because X11's `_NET_WM_ICON`
expects raw ARGB pixels rather than an image filename. If desired, it can be
connected to your existing image-decoder crate without adding an X11-specific
image dependency.

## Dependency

```toml
[target.'cfg(target_os = "linux")'.dependencies]
x11rb = { version = "0.14.0", optional = true }
```

No x11rb extension features are required for this implementation.
