//! `mataimage` backed `Picture` rendered inside a normal `matawin::Window`.

use matawin::{
    Framebuffer, Window,
    events::WindowEvent,
    widgets::{Picture, Rect, WidgetExt},
};

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/image.png".to_string());

    let mut window = Window::new("MataWin Picture", (640, 480));
    let picture = Picture::from_file(path)?;
    picture.set_bounds(Rect::new(0, 0, 640, 480));

    let mut framebuffer = Framebuffer::from_color(640, 480, 0xff20_2020);
    let mut events = Vec::with_capacity(16);
    let mut dirty = true;

    loop {
        window.poll_events_into(&mut events);

        for event in events.iter().copied() {
            match event {
                WindowEvent::Close => return Ok(()),
                WindowEvent::Resized(width, height)
                | WindowEvent::Maximized(width, height)
                | WindowEvent::Restored(width, height) => {
                    let width = width.max(1);
                    let height = height.max(1);
                    framebuffer.resize(width, height);
                    picture.set_bounds(Rect::new(0, 0, width, height));
                    dirty = true;
                }
                _ => {}
            }
        }

        if dirty {
            framebuffer.clear(0xff20_2020);
            picture.blit(&mut framebuffer);
            dirty = false;
        }

        window.present(&framebuffer);
    }
}
