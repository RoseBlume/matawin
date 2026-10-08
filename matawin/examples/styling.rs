//! Demonstrates the styling MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use std::{thread, time::Duration};

use matawin::{
    Window,
    widgets::{
        AnimationIterationCount, AnimationSpec, BorderRadius, Button, Edges, FontPatch,
        Keyframes, Length, MediaQuery, PositionMode, Rect, StyleProperties, Stylable,
        TextField, WidgetEvent, WidgetWindow,
    },
};

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native = Window::new("MataWin styling", (900, 560));
    let window = WidgetWindow::from_window(native)?;

    let field = TextField::new(&window, Rect::new(24, 28, 420, 46), "Responsive units")?;
    field.set_style_properties(StyleProperties {
        width: Some(Length::Vw(48.0)),
        height: Some(Length::Rem(3.0)),
        padding: Some(Edges::vertical_horizontal(Length::Rem(0.45), Length::Rem(0.7))),
        border_radius: Some(BorderRadius::all(Length::Px(12.0))),
        font: FontPatch { size: Some(Length::Rem(1.05)), ..FontPatch::default() },
        background_color: Some(0xff17_1c24),
        ..StyleProperties::default()
    });
    field.set_focus_style(StyleProperties {
        border_color: Some(0xff7f_b7ff),
        border_radius: Some(BorderRadius::all(Length::Px(18.0))),
        ..StyleProperties::default()
    });

    let button = Button::new(&window, Rect::new(470, 28, 160, 46), "Animate")?;
    button.set_style_properties(StyleProperties {
        position: Some(PositionMode::Absolute),
        relative_to: Some(Some(field.id())),
        left: Some(Length::Percent(100.0)),
        top: Some(Length::Px(0.0)),
        width: Some(Length::Vw(18.0)),
        height: Some(Length::Rem(3.0)),
        padding: Some(Edges::vertical_horizontal(Length::Rem(0.4), Length::Rem(1.0))),
        margin: Some(Edges::trbl(
            Length::Px(0.0), Length::Px(0.0), Length::Px(0.0), Length::Vw(2.55),
        )),
        border_radius: Some(BorderRadius::elliptical(
            [Length::Px(14.0), Length::Px(5.0), Length::Px(14.0), Length::Px(5.0)],
            [Length::Px(10.0), Length::Px(5.0), Length::Px(10.0), Length::Px(5.0)],
        )),
        z_index: Some(5),
        font: FontPatch { size: Some(Length::Rem(1.0)), ..FontPatch::default() },
        ..StyleProperties::default()
    });
    button.set_hover_style(StyleProperties {
        background_color: Some(0xff45_5a72), opacity: Some(0.92), ..StyleProperties::default()
    });
    button.set_active_style(StyleProperties {
        background_color: Some(0xff26_384f),
        border_radius: Some(BorderRadius::all(Length::Px(22.0))),
        ..StyleProperties::default()
    });
    button.add_media_style(
        MediaQuery { max_width: Some(Length::Px(700.0)), ..MediaQuery::default() },
        StyleProperties {
            width: Some(Length::Vw(30.0)),
            font: FontPatch { size: Some(Length::Vw(2.4)), ..FontPatch::default() },
            ..StyleProperties::default()
        },
    );

    let relative = Button::new(&window, Rect::new(0, 0, 120, 42), "Relative")?;
    relative.set_style_properties(StyleProperties {
        position: Some(PositionMode::Absolute),
        relative_to: Some(Some(button.id())),
        left: Some(Length::Px(0.0)),
        top: Some(Length::Percent(120.0)),
        width: Some(Length::Rem(8.0)),
        z_index: Some(6),
        border_radius: Some(BorderRadius::all(Length::Px(999.0))),
        ..StyleProperties::default()
    });

    let pulse = Keyframes::new("pulse")
        .with_frame(0.0, StyleProperties {
            opacity: Some(0.55),
            border_radius: Some(BorderRadius::all(Length::Px(6.0))),
            ..StyleProperties::default()
        })
        .with_frame(0.5, StyleProperties {
            opacity: Some(1.0),
            border_radius: Some(BorderRadius::all(Length::Px(22.0))),
            ..StyleProperties::default()
        })
        .with_frame(1.0, StyleProperties {
            opacity: Some(0.55),
            border_radius: Some(BorderRadius::all(Length::Px(6.0))),
            ..StyleProperties::default()
        });
    relative.add_keyframes(pulse);
    let pulse_animation = AnimationSpec {
        name: "pulse".into(),
        duration: Duration::from_millis(1200),
        delay: Duration::from_millis(100),
        iteration_count: AnimationIterationCount::Count(1.0),
    };
    let animate_id = button.id();

    let mut events = Vec::with_capacity(16);
    loop {
        window.poll_events_into(&mut events);
        if events.iter().any(|e| matches!(e, WidgetEvent::Close)) { return Ok(()); }
        if events.iter().any(|e| matches!(e, WidgetEvent::ButtonClicked(id) if *id == animate_id)) {
            relative.set_animation(pulse_animation.clone());
        }
        // `present()` is dirty-driven. This can be called every loop without
        // continuously uploading the framebuffer while the UI is unchanged.
        // The pulse only renders while its one-shot animation is active.
        window.present();
        thread::sleep(Duration::from_millis(8));
    }
}
