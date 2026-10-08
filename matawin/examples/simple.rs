//! Demonstrates the simple MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use matawin::{
    Framebuffer,
    Window,
    events::WindowEvent,
};

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
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
