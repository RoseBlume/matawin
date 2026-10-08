//! Demonstrates the basic MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use matahtml::{Browser, matawin::{Window, events::{Key, WindowEvent}}};

const PAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/index.html");

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() {
    let mut browser = Browser::load(PAGE).expect("failed to load examples/assets/index.html");
    let mut window = Window::new(browser.title(), (900, 650));
    let mut events = Vec::with_capacity(16);
    let mut dirty = true;
    let mut framebuffer = browser.render(900, 650);

    loop {
        window.poll_events_into(&mut events);
        for event in events.iter().copied() {
            match event {
                WindowEvent::Close | WindowEvent::KeyPressed(Key::Escape) => return,
                WindowEvent::Resized(width, height)
                | WindowEvent::Maximized(width, height)
                | WindowEvent::Restored(width, height) => {
                    framebuffer = browser.render(width.max(1), height.max(1));
                    dirty = false;
                }
                WindowEvent::KeyPressed(Key::Down) | WindowEvent::KeyHeld(Key::Down) => {
                    let (_, height) = window.framebuffer_size();
                    browser.scroll_by(28, height);
                    dirty = true;
                }
                WindowEvent::KeyPressed(Key::Up) | WindowEvent::KeyHeld(Key::Up) => {
                    let (_, height) = window.framebuffer_size();
                    browser.scroll_by(-28, height);
                    dirty = true;
                }
                _ => {}
            }
        }

        if dirty {
            let (width, height) = window.framebuffer_size();
            framebuffer = browser.render(width.max(1), height.max(1));
            dirty = false;
        }
        window.present(&framebuffer);
    }
}
