//! Demonstrates the browser MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use std::{thread, time::Duration};

use matawin::{Window, widgets::{Anchors, Button, Rect, TextField, WebView, WidgetEvent, WidgetWindow}};

/// Demonstrates or supports `normalize_uri`.
///
/// Visibility: **private to the example binary**.
fn normalize_uri(value: &str) -> String {
    let value = value.trim();
    if value.contains("://") || value.starts_with("about:") || value.starts_with("file:") {
        value.to_owned()
    } else {
        format!("https://{value}")
    }
}

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native_window = Window::new("Mata Browser", (1200, 800));
    let window = WidgetWindow::from_window(native_window)?;

    let back = Button::new(&window, Rect::new(8, 9, 36, 34), "<")?;
    let forward = Button::new(&window, Rect::new(50, 9, 36, 34), ">")?;
    let reload = Button::new(&window, Rect::new(92, 9, 36, 34), "R")?;
    let address = TextField::new(
        &window,
        Rect::new(136, 9, 990, 34),
        "https://www.rust-lang.org",
    )?;
    let go = Button::new(&window, Rect::new(1138, 9, 54, 34), "Go")?;
    let webview = WebView::new(&window, Rect::new(0, 52, 1200, 748))?;

    // WidgetWindow applies these policies automatically whenever the native
    // Window is resized/maximized/restored.
    back.set_anchors(Anchors::TOP_LEFT);
    forward.set_anchors(Anchors::TOP_LEFT);
    reload.set_anchors(Anchors::TOP_LEFT);
    address.set_anchors(Anchors::TOP_HORIZONTAL);
    go.set_anchors(Anchors::TOP_RIGHT);
    webview.set_anchors(Anchors::ALL);

    // On Windows WebView2 initializes asynchronously, so navigate() queues this
    // URL until the controller is ready. On Linux WebKitGTK loads it immediately.
    webview.navigate(&address.text())?;
    address.focus();

    let mut events = Vec::with_capacity(16);

    loop {
        window.poll_events_into(&mut events);

        for event in events.iter() {
            match event {
                WidgetEvent::Close => return Ok(()),
                WidgetEvent::ButtonClicked(id) if *id == back.id() => webview.go_back(),
                WidgetEvent::ButtonClicked(id) if *id == forward.id() => webview.go_forward(),
                WidgetEvent::ButtonClicked(id) if *id == reload.id() => webview.reload(),
                WidgetEvent::ButtonClicked(id) if *id == go.id() => {
                    webview.navigate(&normalize_uri(&address.text()))?;
                }
                WidgetEvent::TextFieldActivated(id) if *id == address.id() => {
                    webview.navigate(&normalize_uri(&address.text()))?;
                }
                WidgetEvent::WebViewError(_, message) => eprintln!("WebView error: {message}"),
                _ => {}
            }
        }

        // The toolbar is rendered by MataWin; only the WebView is native.
        window.present();

        // Both event pumps are non-blocking.
        thread::sleep(Duration::from_millis(8));
    }
}
