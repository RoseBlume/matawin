use crate::events::{Key, WindowEvent};

pub struct Handlers {
    pub on_close: Option<Box<dyn FnMut()>>,
    pub on_resized: Option<Box<dyn FnMut(u32, u32)>>,
    pub on_key_pressed: Option<Box<dyn FnMut(Key)>>,
}

impl Default for Handlers {
    fn default() -> Self {
        Self {
            on_close: None,
            on_resized: None,
            on_key_pressed: None,
        }
    }
}

impl super::Window {
    pub fn with_close_handler<F: FnMut() + 'static>(mut self, f: F) -> Self {
        self.handlers.on_close = Some(Box::new(f));
        self
    }

    pub fn with_resize_handler<F: FnMut(u32, u32) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_resized = Some(Box::new(f));
        self
    }

    pub fn with_key_handler<F: FnMut(Key) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_key_pressed = Some(Box::new(f));
        self
    }

    pub fn poll_events(&self) -> Vec<WindowEvent> {
        self.pump_events();

        if let Some(queue) = super::EVENT_QUEUE.get() {
            let mut q = queue.lock().unwrap();
            let events = q.clone();
            q.clear();
            return events;
        }

        Vec::new()
    }

    pub fn handle_events(&mut self) {
        let events = self.poll_events();

        for event in events {
            match event {
                WindowEvent::Close => {
                    if let Some(cb) = self.handlers.on_close.as_mut() {
                        cb();
                    }
                }

                WindowEvent::Resized(w, h) => {
                    if let Some(cb) = self.handlers.on_resized.as_mut() {
                        cb(w, h);
                    }
                }

                WindowEvent::KeyPressed(key) => {
                    if let Some(cb) = self.handlers.on_key_pressed.as_mut() {
                        cb(key);
                    }
                }

                _ => {}
            }
        }
    }
}