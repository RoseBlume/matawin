//! MataWin software button/text-field example using `matafonts` for text.
//! Widgets are attached to an explicitly created `matawin::Window`.

use matawin::{
    Window,
    widgets::{Anchors, Button, Rect, TextField, WidgetEvent, WidgetWindow},
};

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native_window = Window::new("Mata widgets", (640, 180));
    let window = WidgetWindow::from_window(native_window)?;

    let field = TextField::new(&window, Rect::new(16, 24, 460, 38), "Type here")?;
    let button = Button::new(&window, Rect::new(488, 24, 132, 38), "Print")?;

    field.set_anchors(Anchors::TOP_HORIZONTAL);
    button.set_anchors(Anchors::TOP_RIGHT);
    field.focus();

    let mut events = Vec::with_capacity(16);
    loop {
        window.poll_events_into(&mut events);

        for event in &events {
            match event {
                WidgetEvent::Close => return Ok(()),
                WidgetEvent::ButtonClicked(id) if *id == button.id() => {
                    println!("{}", field.text());
                }
                WidgetEvent::TextFieldActivated(id) if *id == field.id() => {
                    println!("{}", field.text());
                }
                _ => {}
            }
        }

        window.present();
    }
}
