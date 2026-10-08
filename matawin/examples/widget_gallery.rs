//! MataWin widget construction example hosted by an explicit `matawin::Window`.
//!
//! The interactive controls are rendered by the widget host. The catalog
//! containers below demonstrate how larger widget trees are composed.

use matawin::{
    Window,
    widgets::{
        Anchors, Box as WidgetBox, Button, CheckButton, DropDown, Label, ListView,
        Notebook, Orientation, ProgressBar, Rect, SelectionMode, Stack, Switch,
        TextField, WidgetEvent, WidgetExt, WidgetWindow,
    },
};

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native_window = Window::new("MataWin widget gallery", (760, 420));
    let window = WidgetWindow::from_window(native_window)?;

    // Interactive MataWin controls live inside the Window.
    let field = TextField::new(
        &window,
        Rect::new(24, 24, 520, 40),
        "Widgets are hosted by matawin::Window",
    )?;
    let print = Button::new(&window, Rect::new(560, 24, 176, 40), "Print text")?;
    field.set_anchors(Anchors::TOP_HORIZONTAL);
    print.set_anchors(Anchors::TOP_RIGHT);
    field.focus();

    // The generic widget catalog can still be composed into a hierarchy.
    let root = WidgetBox::new(Orientation::Vertical, 8);
    let title = Label::new("MataWin widget gallery");
    let enabled = CheckButton::new("Enabled");
    let toggle = Switch::new();
    let choices = DropDown::new(["One", "Two", "Three"]);
    let progress = ProgressBar::new();

    let list = ListView::new(SelectionMode::Single);
    list.append("First row");
    list.append("Second row");

    root.append(&title);
    root.append(&enabled);
    root.append(&toggle);
    root.append(&choices);
    root.append(&progress);
    root.append(&list);

    let stack = Stack::new();
    stack.add_named(&root, "main");

    let notebook = Notebook::new();
    notebook.append_page(&stack, "Widgets");

    println!("root widget id: {}", root.id().get());
    println!("children: {:?}", root.children());

    let mut events = Vec::with_capacity(16);
    loop {
        window.poll_events_into(&mut events);

        for event in &events {
            match event {
                WidgetEvent::Close => return Ok(()),
                WidgetEvent::ButtonClicked(id) if *id == print.id() => {
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
