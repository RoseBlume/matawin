//! Implements the `events` module.
//!
//! This module documents both public API items and internal implementation details.

use x11rb::{ connection::Connection, protocol::{ Event, xproto::ConnectionExt as _ } };

use crate::events::WindowEvent;

impl super::PlatformWindow {
    /// Pumps all currently queued X11 events without blocking.
    ///
    /// Auto-repeat suppression uses a single-event lookahead instead of first
    /// collecting the entire X11 queue into a temporary `Vec`, keeping the
    /// common per-frame event pump allocation-free.
    #[doc = "Implements the pump events operation. Visibility: public."]
    pub fn pump_events(&self) {
        let mut pending: Option<Event> = None;

        loop {
            let event = if let Some(event) = pending.take() {
                event
            } else {
                match self.connection.poll_for_event() {
                    Ok(Some(event)) => event,
                    Ok(None) | Err(_) => {
                        break;
                    }
                }
            };

            if let Event::KeyRelease(release) = &event {
                match self.connection.poll_for_event() {
                    Ok(Some(Event::KeyPress(press))) if
                        release.detail == press.detail &&
                        release.time == press.time
                    => {
                        // X11 traditional auto-repeat: keep the key held and
                        // suppress the synthetic release/press pair.
                        continue;
                    }
                    Ok(Some(next)) => {
                        self.handle_x11_event(&event);
                        pending = Some(next);
                        continue;
                    }
                    Ok(None) | Err(_) => {
                        self.handle_x11_event(&event);
                        break;
                    }
                }
            }

            self.handle_x11_event(&event);
        }
    }

    #[inline]
    /// Performs the handle x11 event operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the handle x11 event operation. Visibility: private/internal."]
    fn handle_x11_event(&self, event: &Event) {
        match event {
            Event::ClientMessage(ev) => {
                if
                    ev.type_ == self.atoms.wm_protocols &&
                    ev.format == 32 &&
                    ev.data.as_data32()[0] == self.atoms.wm_delete_window
                {
                    super::push_event(WindowEvent::Close);
                }
            }
            Event::ConfigureNotify(ev) if ev.window == self.window => {
                let new_size = (ev.width as u32, ev.height as u32);
                let new_pos = (ev.x as i32, ev.y as i32);

                if new_size != self.client_size.replace(new_size) {
                    if self.maximized.get() {
                        super::push_event(WindowEvent::Maximized(new_size.0, new_size.1));
                    } else {
                        super::push_event(WindowEvent::Resized(new_size.0, new_size.1));
                    }
                }
                if new_pos != self.position.replace(new_pos) {
                    super::push_event(WindowEvent::Repositioned(new_pos.0, new_pos.1));
                }
            }
            Event::KeyPress(ev) if ev.event == self.window => {
                if let Some(key) = self.keymap.borrow()[ev.detail as usize] {
                    if super::set_key_down(key, true) {
                        super::push_event(WindowEvent::KeyPressed(key));
                    }
                }
            }
            Event::KeyRelease(ev) if ev.event == self.window => {
                if let Some(key) = self.keymap.borrow()[ev.detail as usize] {
                    if super::set_key_down(key, false) {
                        super::push_event(WindowEvent::KeyReleased(key));
                    }
                }
            }
            Event::MotionNotify(ev) if ev.event == self.window => {
                super::push_event(WindowEvent::CursorMoved(ev.event_x as i32, ev.event_y as i32));
            }
            Event::ButtonPress(ev) if ev.event == self.window => {
                match ev.detail {
                    4 => super::push_event(WindowEvent::Scroll(0, 120)),
                    5 => super::push_event(WindowEvent::Scroll(0, -120)),
                    6 => super::push_event(WindowEvent::Scroll(-120, 0)),
                    7 => super::push_event(WindowEvent::Scroll(120, 0)),
                    _ => {
                        let key = match ev.detail {
                            1 => Some(crate::events::Key::MouseLeft),
                            2 => Some(crate::events::Key::MouseMiddle),
                            3 => Some(crate::events::Key::MouseRight),
                            _ => None,
                        };
                        if let Some(key) = key {
                            if super::set_key_down(key, true) {
                                super::push_event(WindowEvent::KeyPressed(key));
                            }
                        }
                    }
                }
            }
            Event::ButtonRelease(ev) if ev.event == self.window => {
                let key = match ev.detail {
                    1 => Some(crate::events::Key::MouseLeft),
                    2 => Some(crate::events::Key::MouseMiddle),
                    3 => Some(crate::events::Key::MouseRight),
                    _ => None,
                };
                if let Some(key) = key {
                    if super::set_key_down(key, false) {
                        super::push_event(WindowEvent::KeyReleased(key));
                    }
                }
            }
            Event::FocusOut(_) => {
                if let Some(keys) = crate::events::KEYS_DOWN.get() {
                    keys.lock().unwrap().clear();
                }
            }
            Event::MappingNotify(_) => self.reload_keymap(),
            Event::UnmapNotify(ev) if ev.window == self.window && !self.hidden.get() => {
                super::push_event(WindowEvent::Minimized);
            }
            Event::Expose(_) => {
                // Full-frame software presentation redraws on the next present.
            }
            _ => {}
        }
    }
}
