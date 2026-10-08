# Wayland backend

This folder is a Wayland equivalent of the supplied Windows backend. It uses the
current user's Wayland session socket for window management and input, and
`wl_shm` shared-memory buffers for software-framebuffer presentation.

No `sudo` or root access is required. The backend does not read `/dev/input` and
does not open DRM/KMS device nodes. Keyboard and mouse events arrive from
`wl_seat`, and pixels are sent to the compositor with reusable `wl_buffer`s.

The source framebuffer keeps the size passed to `PlatformWindow::new`. When the
compositor configures the top-level surface to another size, `renderer.rs` scales
the CPU framebuffer into a three-buffer shared-memory ring using nearest-neighbor
sampling. If all buffers are still busy, the renderer drops that frame rather
than blocking the application thread.

## Wayland differences from Win32

* Top-level clients cannot choose absolute desktop coordinates, so `reposition`
  is intentionally a no-op.
* XDG shell has no standard equivalent of toggling maximize/minimize buttons, so
  those two compatibility methods are no-ops.
* Per-window icons are normally associated with the application/desktop entry,
  not loaded from an arbitrary path, so `set_icon` is a no-op here.
* `set_resizable(false)` is implemented by making the XDG min/max size equal.
* `hide` unmaps the window by committing a null buffer; `show` remaps on the next
  `present`.

The keyboard mapping in `mod.rs` maps Wayland's Linux evdev key codes directly
to the existing `Key` enum. This avoids a dependency on libxkbcommon and thus
avoids a native build dependency, but character keys follow the standard physical
US/QWERTY positions rather than interpreting arbitrary user keyboard layouts.


## Dependencies

The Wayland implementation uses only `wayland-client`, `wayland-protocols`, and the Rust standard library. The `wl_shm` backing files are created directly in `XDG_RUNTIME_DIR` and unlinked immediately after opening; event draining uses `wayland-client`'s own nonblocking read guard. No `memmap2`, `tempfile`, `rustix`, `libc`, or `nix` direct dependency is required.
