// #[cfg(target_os = "windows")]
// #[path = "windows/mod.rs"]
// mod platform;

// #[cfg(target_os = "linux")]
// #[path = "linux/mod.rs"]
// mod platform;

// #[cfg(target_os = "macos")]
// #[path = "macos/mod.rs"]
// mod platform;

// pub struct Window {
//     platform: platform::PlatformWindow,
// }

// impl Window {
//     pub fn new(width: u32, height: u32, title: &str) -> Self {
//         Self {
//             platform: platform::PlatformWindow::new(width, height, title),
//         }
//     }

//     pub fn present(&mut self, fb: &Framebuffer) {
//         self.platform.present(fb);
//     }

//     pub fn handle(&self) -> *mut core::ffi::c_void {
//         self.platform.handle()
//     }
// }

#![allow(non_snake_case)]
pub mod text;
mod background;
use crate::sprites::{SpriteRender, Sprite};
use text::{
    get_font_map,
    TextItem
};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::ffi::c_void;
use crate::events::{WindowEvent, Key, EVENT_QUEUE, KEYS_DOWN};
use crate::buffer::Framebuffer;

// #[cfg_attr(windows, path = "windows/mod.rs")]
// #[cfg_attr(linux, path = "linux/mod.rs")]
// mod platform;

// pub use platform::Window;
use crate::events::Handlers;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
use windows::{PlatformWindow as Window};

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
use linux::{PlatformWindow as Window};

pub type HWND = *mut c_void;


pub enum Background {
    Color(u32),
    Image(Vec<Vec<u32>>), // size must be width * height
}
pub struct AppWindow {
    inner: Window,
    size: (u32, u32),
    pub framebuffer: Framebuffer,
    pub handlers: Handlers,
    pub sprites: Vec<Sprite>,
    pub background: Option<Background>,
    pub texts: HashMap<String, TextItem>,
    target_fps: u32,
    target_frame_time: Duration,
    last_frame: Instant,
}




impl AppWindow {
    pub fn new(title: &str, size: (u32, u32)) -> Self {
        let inner = Window::new(title, size);
        let color = crate::system::system_theme_color();
        let framebuffer = Framebuffer::from_color(size.0, size.1, color);

        let target_fps = 60;
        let target_frame_time = Duration::from_secs_f64(1.0 / target_fps as f64);

        Self {
            inner,
            size,
            framebuffer,
            handlers: Handlers::default(),
            sprites: Vec::new(),
            background: None,
            texts: HashMap::new(),
            target_fps,
            target_frame_time,
            last_frame: Instant::now(),
        }
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.target_fps = fps.max(1); // prevent divide-by-zero
        self.target_frame_time = Duration::from_secs_f64(1.0 / self.target_fps as f64);
    }

    pub fn set_title(&self, title: &str) {
        self.inner.set_title(title);
    }

    #[cfg(target_os = "windows")]
    pub fn set_icon(&self, path: &str) {
        self.inner.set_icon(path);
    }
    pub fn poll_events(&self) -> Vec<WindowEvent> {
        self.inner.pump_events();

        let mut events = Vec::new();

        // --- drain queued events ---
        if let Some(queue) = EVENT_QUEUE.get() {
            let mut q = queue.lock().unwrap();
            events.extend(q.drain(..));
        }

        // --- inject held keys ---
        if let Some(keys) = KEYS_DOWN.get() {
            for key in keys.lock().unwrap().iter() {
                events.push(WindowEvent::KeyHeld(*key));
            }
        }


        events
    }

    pub fn get_size(&self) -> (usize, usize) {
        (self.size.0 as usize, self.size.1 as usize)
    }

    pub fn handle_events(&mut self) {
        let events = self.poll_events();

        for event in events {
            match event {
                WindowEvent::Close => {
                    if let Some(cb) = self.handlers.on_close.as_mut() {
                        cb();
                        std::process::exit(0);
                    }
                }

                WindowEvent::Resized(w, h) => {
                    self.size = (w, h);
                    if let Some(cb) = self.handlers.on_resized.as_mut() {
                        cb(w, h);
                    }
                }

                WindowEvent::Maximized(w, h) => {
                    if let Some(cb) = self.handlers.on_maximized.as_mut() {
                        cb(w, h);
                    }
                }

                WindowEvent::Restored(w, h) => {
                    if let Some(cb) = self.handlers.on_restored.as_mut() {
                        cb(w, h);
                    }
                }

                WindowEvent::Minimized => {
                    if let Some(cb) = self.handlers.on_minimized.as_mut() {
                        cb();
                    }
                }

                WindowEvent::Repositioned(x, y) => {
                    if let Some(cb) = self.handlers.on_repositioned.as_mut() {
                        cb(x, y);
                    }
                }

                WindowEvent::KeyPressed(key) => {
                    if let Some(cb) = self.handlers.on_key_pressed.as_mut() {
                        cb(key);
                    }
                }
                WindowEvent::KeyHeld(key) => {
                    if let Some(cb) = self.handlers.on_key_held.as_mut() {
                        cb(key);
                    }
                }
                WindowEvent::KeyReleased(key) => {
                    if let Some(cb) = self.handlers.on_key_released.as_mut() {
                        cb(key);
                    }
                }

            }
        }
    }

    pub fn with_close_handler<F: FnMut() + 'static>(mut self, f: F) -> Self {
        self.handlers.on_close = Some(Box::new(f));
        self
    }

    pub fn with_resize_handler<F: FnMut(u32, u32) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_resized = Some(Box::new(f));
        self
    }

    pub fn with_maximized_handler<F: FnMut(u32, u32) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_maximized = Some(Box::new(f));
        self
    }

    pub fn with_restored_handler<F: FnMut(u32, u32) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_restored = Some(Box::new(f));
        self
    }

    pub fn with_minimized_handler<F: FnMut() + 'static>(mut self, f: F) -> Self {
        self.handlers.on_minimized = Some(Box::new(f));
        self
    }

    pub fn with_repositioned_handler<F: FnMut(i32, i32) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_repositioned = Some(Box::new(f));
        self
    }

    pub fn with_key_pressed_handler<F: FnMut(Key) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_key_pressed = Some(Box::new(f));
        self
    }

    pub fn with_key_held_handler<F: FnMut(Key) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_key_held = Some(Box::new(f));
        self
    }

    pub fn with_key_released_handler<F: FnMut(Key) + 'static>(mut self, f: F) -> Self {
        self.handlers.on_key_released = Some(Box::new(f));
        self
    }
    pub fn hide(&self) {
        self.inner.hide();
    }

    pub fn maximize(&self) {
        self.inner.maximize();
    }

    pub fn minimize(&self) {
        self.inner.minimize();
    }
    pub fn restore(&self) {
       self.inner.restore();
    }

    pub fn reposition(&self, x: i32, y: i32) {
        self.inner.reposition(x, y);
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.inner.resize(width, height);
    }
    pub fn show(&self) {
        self.inner.show()
    }
 

    pub fn set_decorations(&self, enabled: bool) {
        self.inner.set_decorations(enabled);
    }
    pub fn set_maximizable(&self, enabled: bool) {
        self.inner.set_maximizable(enabled);
    }
    pub fn set_minimizable(&self, enabled: bool) {
        self.inner.set_minimizable(enabled);
    }
    pub fn set_resizable(&self, enabled: bool) {
        self.inner.set_resizable(enabled);
    }

    pub fn handle(&self) -> HWND {
        self.inner.handle()
    }

    pub fn draw(&mut self) {
        let width = self.framebuffer.width as usize;
        let height = self.framebuffer.height as usize;

        // --- Create buffer ---
        let mut buffer: Vec<Vec<u32>> = match &self.background {
            Some(Background::Color(color)) => {
                vec![vec![*color; width]; height]
            }
            Some(Background::Image(image)) => image.clone(),
            None => vec![vec![0x000000; width]; height],
        };

        // --- Draw sprites ---
        for sprite in &mut self.sprites {
            let (sx, sy) = sprite.position;

            match &mut sprite.render {
                SpriteRender::Color(color) => {
                    let (w, h) = sprite.size;

                    for y in 0..h {
                        let py = sy.saturating_add(y);
                        if py >= height { continue; }

                        for x in 0..w {
                            let px = sx.saturating_add(x);
                            if px >= width { continue; }

                            buffer[py][px] = *color;
                        }
                    }
                }

                SpriteRender::Bitmap { pixels } => {
                    let h = pixels.len();
                    if h == 0 { continue; }
                    let w = pixels[0].len();

                    for y in 0..h {
                        let py = sy.saturating_add(y);
                        if py >= height { continue; }

                        for x in 0..w {
                            let pixel = pixels[y][x];
                            if pixel == 0 { continue; }

                            let px = sx.saturating_add(x);
                            if px >= width { continue; }

                            buffer[py][px] = pixel;
                        }
                    }
                }

                SpriteRender::AnimatedBitmap {
                    frames,
                    frame_index,
                    frame_delay,
                    frame_timer,
                } => {
                    if frames.is_empty() { continue; }

                    let frame = &frames[*frame_index];
                    let h = frame.len();
                    if h == 0 { continue; }
                    let w = frame[0].len();

                    for y in 0..h {
                        let py = sy.saturating_add(y);
                        if py >= height { continue; }

                        for x in 0..w {
                            let pixel = frame[y][x];
                            if pixel == 0 { continue; }

                            let px = sx.saturating_add(x);
                            if px >= width { continue; }

                            buffer[py][px] = pixel;
                        }
                    }

                    // animation step (safe)
                    *frame_timer = frame_timer.saturating_add(1);
                    if *frame_timer >= *frame_delay {
                        *frame_timer = 0;
                        *frame_index = (*frame_index + 1) % frames.len();
                    }
                }
            }
        }

        // --- Draw text ---
        let font_map = get_font_map();

        for text_item in self.texts.values() {
            let (tx, ty) = text_item.position;
            let color = text_item.color;
            let size = text_item.size;

            for (i, c) in text_item.content.chars().enumerate() {
                let char_pixels = font_map
                    .get(&c.to_ascii_uppercase())
                    .unwrap_or(&font_map[&' ']);

                // precompute safely
                let char_spacing = 5usize
                    .saturating_mul(size)
                    .saturating_add(7);

                let char_offset = i.saturating_mul(char_spacing);

                for y in 0..5 {
                    for x in 0..5 {
                        if char_pixels[y][x] == 0 { continue; }

                        for sy in 0..size {
                            let py = ty
                                .saturating_add(y.saturating_mul(size))
                                .saturating_add(sy);

                            if py >= height { continue; }

                            for sx in 0..size {
                                let px = tx
                                    .saturating_add(char_offset)
                                    .saturating_add(x.saturating_mul(size))
                                    .saturating_add(sx);

                                if px >= width { continue; }

                                buffer[py][px] = color;
                            }
                        }
                    }
                }
            }
        }

        // --- Flatten ---
        self.framebuffer.pixels = buffer.into_iter().flatten().collect();
    }

    

    

    pub fn present(&mut self) {
        let frame_start = Instant::now();
        self.draw();
        self.inner.present(&self.framebuffer);

        let elapsed = frame_start.elapsed();

        if elapsed < self.target_frame_time {
            std::thread::sleep(self.target_frame_time - elapsed);
        }

        self.last_frame = Instant::now();
    }
}