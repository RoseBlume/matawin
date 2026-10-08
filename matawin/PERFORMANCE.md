# matawin performance pass

This revision focuses on removing work from the per-frame hot paths while keeping the existing high-level API compatible.

## Presentation

### Windows

The Direct2D renderer keeps its factory, HWND render target, and bitmap alive across frames. The destination rectangle is cached and updated only when the native client size changes. When a frame is actually dirty, presentation remains a single `CopyFromMemory`, `DrawBitmap`, and `EndDraw` sequence. The HWND target is compositor-paced rather than forcing immediate presentation. `WidgetWindow` does not submit unchanged frames.

### Wayland

The old Wayland renderer performed two CPU-side copies per frame: first into a temporary `Vec<u32>`, then into the shared-memory file through repeated `pwrite` calls. It now persistently memory-maps each of the three compositor buffers and renders directly into those mappings. Same-size presentation therefore reduces to one contiguous memory copy into shared memory, with no per-frame file-write loop.

Nearest-neighbor resize maps are generated only when the target size changes. Per-pixel integer division was removed from the frame loop. Triple buffering remains non-blocking: if all buffers are owned by the compositor, the frame is dropped rather than stalling the game loop.

### X11

For the common 32-bit little-endian TrueColor layout, an unscaled framebuffer is submitted directly to X11 without color repacking or a staging copy. When scaling is needed, X/Y source indices are cached on resize rather than recomputed with division for every pixel on every frame.

For nonstandard visual layouts, RGB component conversion uses three 256-entry lookup tables instead of mask scaling arithmetic for every component of every pixel. `PutImage` chunk sizing is derived from the server's core maximum request size instead of a fixed conservative chunk size.

## Events

`Window::poll_events_into(&mut Vec<WindowEvent>)` was added for allocation-free steady-state polling. The caller owns and reuses capacity. `Window::handle_events()` internally reuses a scratch event vector as well.

X11 event pumping no longer gathers the complete native event queue into a temporary vector just to detect traditional key-repeat release/press pairs. It uses a one-event lookahead instead.

X11 keycode lookup now uses a fixed 256-entry array rather than hashing keycodes on every key event. Persistent event/key containers are preallocated.

Mouse held/release state on Windows is now tracked consistently, and focus loss clears held keys.

## Framebuffer and widget memory

`Framebuffer` remains a contiguous `Vec<u32>`. New direct slice/pointer accessors avoid wrapper work for hot callers. BMP loading now uses buffered I/O and clips the decode loop before per-pixel conversion instead of checking bounds for every source pixel.

The widget framebuffer reuses capacity during normal resize dragging, but if a restored/downscaled window needs less than half of the retained allocation it releases the oversized buffer instead of permanently retaining the largest maximized size. `WidgetWindow::with_framebuffer` provides borrowed access so callers do not have to clone the whole pixel array.

The software font renderer bounds its responsive-size glyph cache and no longer clones a glyph's pixel vector each time a character is drawn. The per-window resolved font-family cache is also bounded to four renderers so cycling through many font families cannot retain every decoded font and glyph cache indefinitely. Widget rendering reuses z-order and style-reference scratch storage instead of cloning the control list and constructing a new `HashMap` each frame.

## Release build

The crate includes an aggressive standalone release profile (`opt-level=3`, fat LTO, one codegen unit, stripped symbols, abort-on-panic). When `matawin` is consumed as a dependency, the final application's/workspace's release profile controls code generation, so apply equivalent settings at the workspace root for the same result.

## Recommended hot-loop pattern

Keep one framebuffer and one event vector alive for the lifetime of the loop. Mutate `framebuffer.pixels`/`as_mut_slice()` in place, call `poll_events_into()` once, then call `present()` once. Avoid resizing the native window or framebuffer unless the size genuinely changes.


## Widget invalidation and GPU usage

`WidgetWindow::present()` is dirty-driven. Setters, input-state changes, text edits, and resizes mark the widget surface dirty. If nothing changed, repeated calls to `present()` return without clearing the CPU framebuffer, uploading it to Direct2D, or asking the compositor for another frame.

Animations still redraw while active, but animation-only frames are capped to approximately 60 Hz. A finite animation receives one final frame after completion. Applications may still poll events as frequently as needed without continuously consuming presentation GPU bandwidth.
