//! Streamed audio widget example inside an explicitly created `matawin::Window`.
//!
//! Run with an audio file path as the first argument. Playback uses
//! `mata-audio`; the visible controls are normal MataWin software widgets.

use matawin::{
    Window,
    widgets::{Anchors, Audio, Button, Rect, TextField, WidgetEvent, WidgetExt, WidgetWindow},
};

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/music.mp3".to_string());

    let native_window = Window::new("MataWin streamed audio", (760, 190));
    let window = WidgetWindow::from_window(native_window)?;

    // The final argument enables the optional MediaControls model.
    let audio = Audio::from_file(path, true, true)?;
    audio.set_bounds(Rect::new(16, 16, 728, 158));
    audio.set_volume(0.65);
    audio.set_looping(false);

    let play_pause = Button::new(&window, Rect::new(16, 24, 112, 38), "Play/Pause")?;
    let stop = Button::new(&window, Rect::new(136, 24, 72, 38), "Stop")?;
    let back = Button::new(&window, Rect::new(216, 24, 82, 38), "-5 sec")?;
    let forward = Button::new(&window, Rect::new(306, 24, 82, 38), "+5 sec")?;
    let quieter = Button::new(&window, Rect::new(396, 24, 82, 38), "Vol -")?;
    let louder = Button::new(&window, Rect::new(486, 24, 82, 38), "Vol +")?;
    let position = TextField::new(&window, Rect::new(16, 78, 552, 38), "Playing")?;
    // Keep the transport row attached to the top-left while the status field
    // stretches with the window. Individual controls can use different anchors.
    play_pause.set_anchors(Anchors::TOP_LEFT);
    stop.set_anchors(Anchors::TOP_LEFT);
    back.set_anchors(Anchors::TOP_LEFT);
    forward.set_anchors(Anchors::TOP_LEFT);
    quieter.set_anchors(Anchors::TOP_LEFT);
    louder.set_anchors(Anchors::TOP_LEFT);
    position.set_anchors(Anchors::TOP_HORIZONTAL);
    position.set_enabled(false);

    let controls = audio.controls();
    let mut events = Vec::with_capacity(16);

    loop {
        window.poll_events_into(&mut events);

        for event in &events {
            match event {
                WidgetEvent::Close => {
                    audio.stop();
                    return Ok(());
                }
                WidgetEvent::ButtonClicked(id) if *id == play_pause.id() => {
                    if let Some(controls) = &controls {
                        controls.toggle();
                    }
                }
                WidgetEvent::ButtonClicked(id) if *id == stop.id() => {
                    if let Some(controls) = &controls {
                        controls.stop();
                    }
                }
                WidgetEvent::ButtonClicked(id) if *id == back.id() => {
                    if let Some(controls) = &controls {
                        controls.seek((controls.position() - 5.0).max(0.0));
                    }
                }
                WidgetEvent::ButtonClicked(id) if *id == forward.id() => {
                    if let Some(controls) = &controls {
                        let target = controls.position() + 5.0;
                        controls.seek(controls.duration().map_or(target, |d| target.min(d)));
                    }
                }
                WidgetEvent::ButtonClicked(id) if *id == quieter.id() => {
                    if let Some(controls) = &controls {
                        controls.set_volume((controls.volume() - 0.05).max(0.0));
                    }
                }
                WidgetEvent::ButtonClicked(id) if *id == louder.id() => {
                    if let Some(controls) = &controls {
                        controls.set_volume((controls.volume() + 0.05).min(1.0));
                    }
                }
                _ => {}
            }
        }

        if let Some(controls) = &controls {
            controls.sync();
            let status = format!(
                "Position: {:.1}s / {}   Volume: {:.0}%",
                controls.position(),
                controls
                    .duration()
                    .map(|d| format!("{d:.1}s"))
                    .unwrap_or_else(|| "unknown".to_string()),
                controls.volume() * 100.0,
            );
            if position.text() != status {
                position.set_text(&status);
            }
        }

        window.present();
    }
}
