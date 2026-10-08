# Linux backend selection

`linux/mod.rs` performs runtime backend selection while keeping a single
`PlatformWindow` type for the rest of the crate.

When both backends are compiled in, construction follows this order:

1. Try to connect to and initialize Wayland.
2. If Wayland setup fails, try X11.
3. If both fail, panic with both initialization errors.

The selection happens only when the window is created. Rendering and event
handling then dispatch directly to the already-selected backend. Neither backend
requires `sudo`; both use the current user's graphical session.

## Cargo features

For automatic Wayland -> X11 fallback, enable **both** features. A typical setup
is:

```toml
[features]
default = ["wayland", "x11"]
wayland = ["dep:wayland-client", "dep:wayland-protocols"]
x11 = ["dep:x11rb"]

[target.'cfg(target_os = "linux")'.dependencies]
wayland-client = { version = "0.31.15", optional = true }
wayland-protocols = { version = "0.32.13", features = ["client"], optional = true }
x11rb = { version = "0.14.0", optional = true }
```

If only `wayland` is enabled, there is no X11 fallback. If only `x11` is enabled,
X11 is used directly.

## Runtime behavior

No environment-variable guessing is used to choose the backend. The code really
attempts a Wayland connection first. This handles cases such as an inherited or
stale `WAYLAND_DISPLAY` more reliably: if the compositor connection, required
Wayland globals, or initial XDG-shell setup fails, the attempt returns an error
and X11 is tried next.

`PlatformWindow::backend_name()` returns either `"wayland"` or `"x11"`, which is
useful for logging or diagnostics.
