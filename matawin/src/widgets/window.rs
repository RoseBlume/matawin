//! Widget host, input dispatch, focus management, and presentation.

use std::{
    cell::{ Cell, RefCell },
    collections::HashMap,
    path::Path,
    rc::Rc,
    sync::{ Arc, Mutex, atomic::{ AtomicBool, Ordering } },
    time::{ Duration, Instant },
};

use crate::{ Framebuffer, Window as NativeWindow, events::{ Key, WindowEvent } };

use super::{
    controls::{ sync_tree_states, ControlEntry, TextFieldState },
    events::{ drain_events, next_id, WidgetError, WidgetEvent, WidgetId },
    font::FontRenderer,
    render::{ draw_button, draw_text_field, next_boundary, prev_boundary, BG },
};

#[cfg(feature = "webview")]
use std::{ rc::Weak as RcWeak, sync::Weak as ArcWeak };

#[cfg(feature = "webview")]
use super::geometry::ResizeState;

#[cfg(feature = "webview")]
use super::webview_platform;

#[cfg(feature = "webview")]
/// Stores the state and data for web view resize entry.
///
/// Visibility: **private**.
struct WebViewResizeEntry {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: RcWeak<webview_platform::WebView>,
    /// Stores resize.
    ///
    /// Visibility: **private**.
    resize: ArcWeak<Mutex<ResizeState>>,
    /// Stores style.
    ///
    /// Visibility: **private**.
    style: crate::widgets::style::StyleHandle,
}

/// A MataWin software-widget host. No GTK/Win32 child controls are created.
pub struct WidgetWindow {
    /// Stores id.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) id: u64,
    /// Stores window.
    ///
    /// Visibility: **private**.
    window: RefCell<NativeWindow>,
    /// Stores framebuffer.
    ///
    /// Visibility: **private**.
    framebuffer: RefCell<Framebuffer>,
    /// Stores controls.
    ///
    /// Visibility: **private**.
    controls: RefCell<Vec<ControlEntry>>,
    /// Stores render order.
    ///
    /// Visibility: **private**.
    render_order: RefCell<Vec<usize>>,
    /// Stores style references.
    ///
    /// Visibility: **private**.
    style_references: RefCell<HashMap<WidgetId, crate::widgets::Rect>>,
    /// Stores focus.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) focus: Arc<Mutex<Option<WidgetId>>>,
    /// Stores cursor.
    ///
    /// Visibility: **private**.
    cursor: Cell<(i32, i32)>,
    /// Stores left shift down.
    ///
    /// Visibility: **private**.
    left_shift_down: Cell<bool>,
    /// Stores right shift down.
    ///
    /// Visibility: **private**.
    right_shift_down: Cell<bool>,
    /// Stores font.
    ///
    /// Visibility: **private**.
    font: RefCell<Option<FontRenderer>>,
    /// Stores font cache.
    ///
    /// Visibility: **private**.
    font_cache: RefCell<HashMap<String, FontRenderer>>,
    /// Stores background.
    ///
    /// Visibility: **private**.
    background: Cell<u32>,
    /// Stores dirty.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) dirty: Arc<AtomicBool>,
    /// Stores last present.
    ///
    /// Visibility: **private**.
    last_present: Cell<Instant>,
    /// Stores was animating.
    ///
    /// Visibility: **private**.
    was_animating: Cell<bool>,
    /// Stores native events.
    ///
    /// Visibility: **private**.
    native_events: RefCell<Vec<WindowEvent>>,
    /// Stores host size.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) host_size: Rc<Cell<(u32, u32)>>,
    #[cfg(feature = "webview")]
    /// Stores webviews.
    ///
    /// Visibility: **private**.
    webviews: RefCell<Vec<WebViewResizeEntry>>,
}
impl WidgetWindow {
    /// Attaches the MataWin software-widget layer to an existing [`Window`].
    ///
    /// This is the preferred constructor for applications: create the native
    /// `matawin::Window` first, configure it as needed, and then move it into
    /// the widget host. The host continues to expose the same underlying
    /// window for event pumping and framebuffer presentation.
    #[doc = "Implements the from window operation. Visibility: public."]
    pub fn from_window(window: NativeWindow) -> Result<Self, WidgetError> {
        let (width, height) = window.client_size();
        let size = (width.max(1), height.max(1));
        Ok(Self {
            id: next_id(),
            window: RefCell::new(window),
            framebuffer: RefCell::new(Framebuffer::from_color(size.0, size.1, BG)),
            controls: RefCell::new(Vec::with_capacity(16)),
            render_order: RefCell::new(Vec::with_capacity(16)),
            style_references: RefCell::new(HashMap::with_capacity(16)),
            focus: Arc::new(Mutex::new(None)),
            cursor: Cell::new((0, 0)),
            left_shift_down: Cell::new(false),
            right_shift_down: Cell::new(false),
            font: RefCell::new(FontRenderer::system_default().ok()),
            font_cache: RefCell::new(HashMap::new()),
            background: Cell::new(BG),
            dirty: Arc::new(AtomicBool::new(true)),
            last_present: Cell::new(Instant::now() - Duration::from_millis(17)),
            was_animating: Cell::new(false),
            native_events: RefCell::new(Vec::with_capacity(16)),
            host_size: Rc::new(Cell::new(size)),
            #[cfg(feature = "webview")]
            webviews: RefCell::new(Vec::with_capacity(4)),
        })
    }

    /// Convenience constructor retained for compatibility.
    ///
    /// New examples use [`WidgetWindow::from_window`] so that the application
    /// creates and configures its `matawin::Window` explicitly.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(title: &str, size: (u32, u32)) -> Result<Self, WidgetError> {
        let size = (size.0.max(1), size.1.max(1));
        Self::from_window(NativeWindow::new(title, size))
    }
    /// Sets font file.
    ///
    /// Visibility: **public**.
    #[doc = "Sets font file. Visibility: public."]
    pub fn set_font_file(&self, path: impl AsRef<Path>) -> Result<(), WidgetError> {
        *self.font.borrow_mut() = Some(FontRenderer::from_file(path)?);
        self.request_redraw();
        Ok(())
    }
    /// Sets font bytes.
    ///
    /// Visibility: **public**.
    #[doc = "Sets font bytes. Visibility: public."]
    pub fn set_font_bytes(&self, bytes: &[u8]) -> Result<(), WidgetError> {
        *self.font.borrow_mut() = Some(FontRenderer::from_bytes(bytes)?);
        self.request_redraw();
        Ok(())
    }
    #[inline]
    #[doc = "Sets background. Visibility: public."]
    pub fn set_background(&self, color: u32) {
        if self.background.replace(color) != color {
            self.request_redraw()
        }
    }
    #[inline]
    #[doc = "Implements the size operation. Visibility: public."]
    pub fn size(&self) -> (u32, u32) {
        self.host_size.get()
    }
    #[inline]
    #[doc = "Sets title. Visibility: public."]
    pub fn set_title(&self, title: &str) {
        self.window.borrow().set_title(title)
    }
    #[inline]
    #[doc = "Implements the id operation. Visibility: crate-visible."]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    #[inline]
    #[doc = "Implements the native handle operation. Visibility: crate-visible."]
    pub(crate) fn native_handle(&self) -> *mut std::ffi::c_void {
        self.window.borrow().handle()
    }
    /// Marks the software widget surface dirty. The next `present()` rerenders it.
    #[inline]
    #[doc = "Implements the request redraw operation. Visibility: public."]
    pub fn request_redraw(&self) {
        self.dirty.store(true, Ordering::Release)
    }
    #[doc = "Implements the register operation. Visibility: crate-visible."]
    pub(crate) fn register(&self, c: ControlEntry) {
        let mut controls = self.controls.borrow_mut();
        controls.push(c);
        sync_tree_states(&controls);
        drop(controls);
        self.request_redraw();
    }
    #[cfg(feature = "webview")]
    /// Registers webview.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Registers webview. Visibility: crate-visible."]
    pub(crate) fn register_webview(
        &self,
        inner: &Rc<webview_platform::WebView>,
        resize: &Arc<Mutex<ResizeState>>,
        style: &crate::widgets::style::StyleHandle
    ) {
        self.webviews
            .borrow_mut()
            .push(WebViewResizeEntry {
                inner: Rc::downgrade(inner),
                resize: Arc::downgrade(resize),
                style: style.clone(),
            });
    }
    /// Performs the resize widgets operation.
    ///
    /// Visibility: **private**.
    #[doc = "Resizes widgets. Visibility: private/internal."]
    fn resize_widgets(&self, size: (u32, u32)) {
        let size = (size.0.max(1), size.1.max(1));
        self.host_size.set(size);
        for control in self.controls.borrow().iter() {
            control.apply_resize(size);
        }
        self.resolve_control_styles();
        #[cfg(feature = "webview")]
        self.sync_webviews();
    }

    /// Invalidates the old raster surface and lays every software widget out
    /// against the new native client-pixel dimensions. No old frame is scaled
    /// into the replacement framebuffer.
    #[doc = "Resizes render surface. Visibility: private/internal."]
    fn resize_render_surface(&self, size: (u32, u32)) {
        let size = (size.0.max(1), size.1.max(1));
        self.framebuffer.borrow_mut().resize_and_clear(size.0, size.1, self.background.get());
        self.resize_widgets(size);
        // A resize can move responsive controls underneath a stationary cursor.
        // Re-evaluate hover immediately rather than waiting for another mouse
        // movement message.
        let (x, y) = self.cursor.get();
        self.update_hover(x, y);
        self.request_redraw();
    }

    /// Synchronizes the software render target to the native drawable client
    /// area. This runs both while polling and immediately before presentation,
    /// so a coalesced/missed resize event cannot leave an old low-resolution
    /// framebuffer stretched across a larger window.
    #[doc = "Synchronizes native size. Visibility: private/internal."]
    fn sync_native_size(&self) -> Option<(u32, u32)> {
        let native = self.window.borrow().client_size();
        if native.0 == 0 || native.1 == 0 {
            return None;
        }
        if native != self.host_size.get() {
            self.resize_render_surface(native);
        }
        Some(native)
    }

    #[inline]
    /// Performs the resize event size operation.
    ///
    /// Visibility: **private**.
    #[doc = "Resizes event size. Visibility: private/internal."]
    fn resize_event_size(&self, fallback: (u32, u32)) -> (u32, u32) {
        self.sync_native_size().unwrap_or((fallback.0.max(1), fallback.1.max(1)))
    }

    #[cfg(feature = "webview")]
    /// Performs the sync webviews operation.
    ///
    /// Visibility: **private**.
    #[doc = "Synchronizes webviews. Visibility: private/internal."]
    fn sync_webviews(&self) {
        let size = self.size();
        let references: HashMap<WidgetId, crate::widgets::Rect> = self.controls
            .borrow()
            .iter()
            .map(|c| (c.id(), c.bounds()))
            .collect();
        self.webviews.borrow_mut().retain(|entry| {
            let (Some(inner), Some(resize)) = (entry.inner.upgrade(), entry.resize.upgrade()) else {
                return false;
            };
            let base = resize.lock().unwrap().resolve(size);
            let reference = entry.style.relative_to().and_then(|id| references.get(&id).copied());
            let bounds = entry.style.computed(base, size, reference).bounds;
            inner.set_bounds(bounds);
            true
        });
    }

    /// Performs the poll events into operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the poll events into operation. Visibility: public."]
    pub fn poll_events_into(&self, out: &mut Vec<WidgetEvent>) {
        #[cfg(feature = "webview")]
        {
            webview_platform::pump_events();
            self.sync_webviews();
        }
        let mut native = self.native_events.borrow_mut();
        self.window.borrow().poll_events_into(&mut native);
        // Query the actual drawable size after the native message pump. The
        // resize event is still forwarded below, but rendering no longer relies
        // on that event having exact/current dimensions.
        self.sync_native_size();
        out.clear();
        for event in native.iter().copied() {
            self.handle_native(event, out);
        }
        native.clear();
        drain_events(self.id, out);
    }
    /// Performs the poll events operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the poll events operation. Visibility: public."]
    pub fn poll_events(&self) -> Vec<WidgetEvent> {
        let mut out = Vec::with_capacity(16);
        self.poll_events_into(&mut out);
        out
    }

    /// Rerasterizes the entire software widget tree at the current native client
    /// resolution and only then submits the frame to the platform renderer.
    ///
    /// A resize can race with rendering. If that happens, `present_exact` rejects
    /// the stale-size frame and this method rerenders once at the newest size
    /// instead of stretching the previous raster image.
    /// Presents only when the widget tree is dirty or an animation needs a frame.
    /// Static focused windows therefore consume essentially no presentation GPU
    /// bandwidth instead of uploading the full framebuffer in a tight loop.
    #[doc = "Implements the present operation. Visibility: public."]
    pub fn present(&self) {
        self.sync_native_size();
        let animating = self.controls.borrow().iter().any(ControlEntry::animation_running);
        let was_animating = self.was_animating.get();
        let dirty = self.dirty.load(Ordering::Acquire);
        if !dirty && !animating && !was_animating {
            return;
        }

        // Animation-only updates are capped at about 60 Hz. Input/style changes
        // bypass the cap so hover, typing, and focus remain immediate.
        if !dirty && animating && self.last_present.get().elapsed() < Duration::from_millis(16) {
            return;
        }

        // Consume the current invalidation before rendering. If another setter
        // runs during the render it sets the flag again and that update is not lost.
        self.dirty.swap(false, Ordering::AcqRel);
        for _ in 0..2 {
            self.sync_native_size();
            self.render();
            let presented = {
                let fb = self.framebuffer.borrow();
                debug_assert_eq!((fb.width, fb.height), self.host_size.get());
                self.window.borrow_mut().present_exact(&fb)
            };
            if presented {
                self.last_present.set(Instant::now());
                self.was_animating.set(animating);
                return;
            }
        }
        self.request_redraw();
    }

    /// Returns a copy of the current framebuffer. For large windows this is a
    /// full pixel-buffer copy; prefer `with_framebuffer` when a borrow is enough.
    #[doc = "Implements the framebuffer operation. Visibility: public."]
    pub fn framebuffer(&self) -> Framebuffer {
        self.framebuffer.borrow().clone()
    }
    /// Returns the value configured with framebuffer.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with framebuffer. Visibility: public."]
    pub fn with_framebuffer<R>(&self, f: impl FnOnce(&Framebuffer) -> R) -> R {
        let fb = self.framebuffer.borrow();
        f(&fb)
    }

    /// Performs the render operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the render operation. Visibility: public."]
    pub fn render(&self) {
        self.resolve_control_styles();
        let mut fb = self.framebuffer.borrow_mut();
        fb.clear(self.background.get());
        let controls = self.controls.borrow();
        let mut order = self.render_order.borrow_mut();
        order.clear();
        order.extend(0..controls.len());
        order.sort_unstable_by_key(|index| controls[*index].z_index());
        let focused = *self.focus.lock().unwrap();
        let mut default_font = self.font.borrow_mut();
        let mut font_cache = self.font_cache.borrow_mut();
        for &index in order.iter() {
            match &controls[index] {
                ControlEntry::Button { state, .. } => {
                    let s = state.lock().unwrap();
                    if s.alive && s.visible {
                        let font = font_for_style(
                            &mut default_font,
                            &mut font_cache,
                            &s.computed.font
                        );
                        draw_button(&mut fb, font, &s);
                    }
                }
                ControlEntry::TextField { id, state } => {
                    let s = state.lock().unwrap();
                    if s.alive && s.visible {
                        let font = font_for_style(
                            &mut default_font,
                            &mut font_cache,
                            &s.computed.font
                        );
                        draw_text_field(&mut fb, font, &s, focused == Some(*id));
                    }
                }
            }
        }
    }

    /// Resolves control styles from the current state.
    ///
    /// Visibility: **private**.
    #[doc = "Resolves control styles. Visibility: private/internal."]
    fn resolve_control_styles(&self) {
        let controls = self.controls.borrow();
        let size = self.size();
        let focused = *self.focus.lock().unwrap();
        let mut references = self.style_references.borrow_mut();
        references.clear();
        let additional = controls.len().saturating_sub(references.capacity());
        references.reserve(additional);
        for c in controls.iter() {
            references.insert(c.id(), c.bounds());
        }
        // Two passes allow a widget to position itself relative to a sibling
        // regardless of which sibling was registered first.
        for _ in 0..2 {
            for c in controls.iter() {
                c.resolve_style(size, &references, focused);
                references.insert(c.id(), c.bounds());
            }
        }
    }

    /// Performs the hit test operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the hit test operation. Visibility: private/internal."]
    fn hit_test(&self, x: i32, y: i32) -> Option<ControlEntry> {
        self.resolve_control_styles();
        self.controls
            .borrow()
            .iter()
            .filter(|c| c.alive_visible() && c.bounds().contains(x, y))
            .max_by_key(|c| c.z_index())
            .cloned()
    }

    /// Performs the handle native operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the handle native operation. Visibility: private/internal."]
    fn handle_native(&self, event: WindowEvent, out: &mut Vec<WidgetEvent>) {
        match event {
            WindowEvent::Close => out.push(WidgetEvent::Close),
            | WindowEvent::Resized(w, h)
            | WindowEvent::Maximized(w, h)
            | WindowEvent::Restored(w, h) => {
                let size = self.resize_event_size((w, h));
                if self.host_size.get() != size {
                    self.resize_render_surface(size);
                }
                out.push(WidgetEvent::Resized(size.0, size.1));
            }
            WindowEvent::CursorMoved(x, y) => {
                // WidgetWindow's framebuffer, layout and Windows client area are
                // intentionally 1:1. Keep the native client coordinates exactly
                // as delivered instead of applying another resize/DPI transform.
                self.sync_native_size();
                self.cursor.set((x, y));
                self.update_hover(x, y)
            }
            WindowEvent::KeyPressed(Key::MouseLeft) => {
                let p = self.cursor.get();
                self.pointer_down(p.0, p.1)
            }
            WindowEvent::KeyReleased(Key::MouseLeft) => {
                let p = self.cursor.get();
                self.pointer_up(p.0, p.1, out)
            }
            WindowEvent::KeyPressed(Key::LeftShift) => self.left_shift_down.set(true),
            WindowEvent::KeyPressed(Key::RightShift) => self.right_shift_down.set(true),
            WindowEvent::KeyReleased(Key::LeftShift) => self.left_shift_down.set(false),
            WindowEvent::KeyReleased(Key::RightShift) => self.right_shift_down.set(false),
            WindowEvent::KeyPressed(key @ (Key::Char(_) | Key::Num(_))) => {
                if let Some(ch) = self.text_input_char(key) {
                    self.insert_char(ch, out)
                }
            }
            WindowEvent::KeyPressed(Key::Space) => self.insert_char(' ', out),
            WindowEvent::KeyPressed(Key::Backspace) => self.backspace(out),
            WindowEvent::KeyPressed(Key::Left) => self.move_cursor(false),
            WindowEvent::KeyPressed(Key::Right) => self.move_cursor(true),
            WindowEvent::KeyPressed(Key::Enter) => {
                if let Some(id) = *self.focus.lock().unwrap() {
                    if self.is_text(id) {
                        out.push(WidgetEvent::TextFieldActivated(id))
                    }
                }
            }
            WindowEvent::KeyPressed(Key::Tab) => self.focus_next(),
            _ => {}
        }
    }
    /// Updates hover.
    ///
    /// Visibility: **private**.
    #[doc = "Updates hover. Visibility: private/internal."]
    fn update_hover(&self, x: i32, y: i32) {
        let top = self.hit_test(x, y).map(|c| c.id());
        let mut changed = false;
        for c in self.controls.borrow().iter() {
            let hovered = top == Some(c.id());
            match c {
                ControlEntry::Button { state, .. } => {
                    let mut s = state.lock().unwrap();
                    let next = hovered && s.enabled;
                    if s.hovered != next {
                        s.hovered = next;
                        changed = true;
                    }
                }
                ControlEntry::TextField { state, .. } => {
                    let mut s = state.lock().unwrap();
                    let next = hovered && s.enabled;
                    if s.hovered != next {
                        s.hovered = next;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.request_redraw()
        }
    }
    /// Performs the pointer down operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the pointer down operation. Visibility: private/internal."]
    fn pointer_down(&self, x: i32, y: i32) {
        let hit = self.hit_test(x, y);
        match hit {
            Some(ControlEntry::Button { id, state }) => {
                let mut s = state.lock().unwrap();
                if s.enabled {
                    s.pressed = true;
                    *self.focus.lock().unwrap() = Some(id);
                    self.request_redraw()
                }
            }
            Some(ControlEntry::TextField { id, state }) => {
                let enabled = {
                    let mut s = state.lock().unwrap();
                    let enabled = s.enabled;
                    if enabled {
                        s.pressed = true;
                    }
                    enabled
                };
                if enabled {
                    *self.focus.lock().unwrap() = Some(id);
                    self.place_cursor(id, x);
                    self.request_redraw()
                }
            }
            None => {
                let mut focus = self.focus.lock().unwrap();
                if focus.take().is_some() {
                    drop(focus);
                    self.request_redraw()
                }
            }
        }
    }
    /// Performs the pointer up operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the pointer up operation. Visibility: private/internal."]
    fn pointer_up(&self, x: i32, y: i32, out: &mut Vec<WidgetEvent>) {
        let mut changed = false;
        for c in self.controls.borrow().iter() {
            match c {
                ControlEntry::Button { id, state } => {
                    let mut s = state.lock().unwrap();
                    let click =
                        s.alive && s.visible && s.enabled && s.pressed && s.bounds.contains(x, y);
                    if s.pressed {
                        changed = true;
                    }
                    s.pressed = false;
                    if click {
                        out.push(WidgetEvent::ButtonClicked(*id));
                    }
                }
                ControlEntry::TextField { state, .. } => {
                    let mut s = state.lock().unwrap();
                    if s.pressed {
                        changed = true;
                    }
                    s.pressed = false;
                }
            }
        }
        if changed {
            self.request_redraw()
        }
    }
    /// Performs the text state operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the text state operation. Visibility: private/internal."]
    fn text_state(&self, id: WidgetId) -> Option<Arc<Mutex<TextFieldState>>> {
        self.controls
            .borrow()
            .iter()
            .find_map(|c| {
                match c {
                    ControlEntry::TextField { id: other, state } if *other == id =>
                        Some(state.clone()),
                    _ => None,
                }
            })
    }
    /// Returns whether text.
    ///
    /// Visibility: **private**.
    #[doc = "Returns whether text. Visibility: private/internal."]
    fn is_text(&self, id: WidgetId) -> bool {
        self.controls
            .borrow()
            .iter()
            .any(|c| c.id() == id && c.is_text() && c.alive_visible())
    }
    /// Converts a physical key into the character a text field should insert.
    ///
    /// The native backends expose letter, number-row, and punctuation keys in
    /// their unshifted form. Text input applies the current Shift modifier here
    /// so every backend gets the same behavior.
    #[doc = "Implements the text input char operation. Visibility: private/internal."]
    fn text_input_char(&self, key: Key) -> Option<char> {
        key_to_text_char(key, self.left_shift_down.get() || self.right_shift_down.get())
    }

    /// Inserts char.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the insert char operation. Visibility: private/internal."]
    fn insert_char(&self, ch: char, out: &mut Vec<WidgetEvent>) {
        if ch.is_control() {
            return;
        }
        let Some(id) = *self.focus.lock().unwrap() else {
            return;
        };
        let Some(state) = self.text_state(id) else {
            return;
        };
        let mut s = state.lock().unwrap();
        if !s.enabled {
            return;
        }
        let cursor = s.cursor.min(s.text.len());
        s.text.insert(cursor, ch);
        s.cursor = cursor + ch.len_utf8();
        out.push(WidgetEvent::TextChanged(id));
        self.request_redraw();
    }
    /// Performs the backspace operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the backspace operation. Visibility: private/internal."]
    fn backspace(&self, out: &mut Vec<WidgetEvent>) {
        let Some(id) = *self.focus.lock().unwrap() else {
            return;
        };
        let Some(state) = self.text_state(id) else {
            return;
        };
        let mut s = state.lock().unwrap();
        if !s.enabled || s.cursor == 0 {
            return;
        }
        let end = s.cursor.min(s.text.len());
        let start = prev_boundary(&s.text, end);
        s.text.replace_range(start..end, "");
        s.cursor = start;
        out.push(WidgetEvent::TextChanged(id));
        self.request_redraw();
    }
    /// Performs the move cursor operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the move cursor operation. Visibility: private/internal."]
    fn move_cursor(&self, right: bool) {
        let Some(id) = *self.focus.lock().unwrap() else {
            return;
        };
        let Some(state) = self.text_state(id) else {
            return;
        };
        let mut s = state.lock().unwrap();
        let old = s.cursor;
        s.cursor = if right {
            next_boundary(&s.text, s.cursor)
        } else {
            prev_boundary(&s.text, s.cursor)
        };
        if s.cursor != old {
            drop(s);
            self.request_redraw()
        }
    }
    /// Performs the focus next operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the focus next operation. Visibility: private/internal."]
    fn focus_next(&self) {
        let ids: Vec<_> = self.controls
            .borrow()
            .iter()
            .filter(|c| c.is_text() && c.alive_visible())
            .map(ControlEntry::id)
            .collect();
        if ids.is_empty() {
            let mut f = self.focus.lock().unwrap();
            if f.take().is_some() {
                drop(f);
                self.request_redraw();
            }
            return;
        }
        let cur = *self.focus.lock().unwrap();
        let next = cur
            .and_then(|id| ids.iter().position(|x| *x == id))
            .map(|i| ids[(i + 1) % ids.len()])
            .unwrap_or(ids[0]);
        if cur != Some(next) {
            *self.focus.lock().unwrap() = Some(next);
            self.request_redraw()
        }
    }
    /// Performs the place cursor operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the place cursor operation. Visibility: private/internal."]
    fn place_cursor(&self, id: WidgetId, x: i32) {
        let Some(state) = self.text_state(id) else {
            return;
        };
        let mut s = state.lock().unwrap();
        let area = if s.computed.content_bounds == s.bounds {
            crate::widgets::Rect::new(
                s.bounds.x + 6,
                s.bounds.y + 3,
                s.bounds.width.saturating_sub(12),
                s.bounds.height.saturating_sub(6)
            )
        } else {
            s.computed.content_bounds
        };
        let rel = x.saturating_sub(area.x).max(0) as u32;
        let pt = s.computed.font_size_px.round().clamp(1.0, u16::MAX as f32) as u16;
        let line = Some(s.computed.line_height_px.round().max(1.0) as u32);
        let mut best = 0;
        let mut default_font = self.font.borrow_mut();
        let mut font_cache = self.font_cache.borrow_mut();
        if let Some(font) = font_for_style(&mut default_font, &mut font_cache, &s.computed.font) {
            for (byte, _) in s.text.char_indices() {
                let w = font
                    .measure_styled(
                        &s.text[..byte],
                        pt,
                        s.computed.font.variant,
                        s.computed.font.weight,
                        s.computed.font.stretch,
                        line
                    )
                    .map(|v| v.0)
                    .unwrap_or(0);
                if w <= rel {
                    best = byte;
                } else {
                    break;
                }
            }
            let full = font
                .measure_styled(
                    &s.text,
                    pt,
                    s.computed.font.variant,
                    s.computed.font.weight,
                    s.computed.font.stretch,
                    line
                )
                .map(|v| v.0)
                .unwrap_or(0);
            if rel >= full {
                best = s.text.len();
            }
        } else {
            best = s.text.len();
        }
        let old = s.cursor;
        s.cursor = best;
        if old != best {
            drop(s);
            self.request_redraw()
        }
    }
}

/// Maps the crate's logical key values to printable text using a conventional
/// US keyboard Shift layer. Backends intentionally report the unshifted key so
/// modifier handling stays consistent in one place.
#[doc = "Implements the key to text char operation. Visibility: private/internal."]
fn key_to_text_char(key: Key, shift: bool) -> Option<char> {
    match key {
        Key::Num(n) if n <= 9 => {
            if shift {
                Some(match n {
                    0 => ')',
                    1 => '!',
                    2 => '@',
                    3 => '#',
                    4 => '$',
                    5 => '%',
                    6 => '^',
                    7 => '&',
                    8 => '*',
                    9 => '(',
                    _ => unreachable!(),
                })
            } else {
                Some((b'0' + n) as char)
            }
        }
        Key::Char(ch) => {
            if !shift {
                return Some(ch);
            }
            Some(match ch {
                'a'..='z' => ch.to_ascii_uppercase(),
                'A'..='Z' => ch,
                '`' => '~',
                '-' => '_',
                '=' => '+',
                '[' => '{',
                ']' => '}',
                '\\' => '|',
                ';' => ':',
                '\'' => '"',
                ',' => '<',
                '.' => '>',
                '/' => '?',
                _ => ch,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
/// Contains the text input tests implementation.
///
/// Visibility: **private**.
mod text_input_tests {
    use super::key_to_text_char;
    use crate::events::Key;

    #[test]
    /// Implements shift capitalizes letters.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the shift capitalizes letters operation. Visibility: private/internal."]
    fn shift_capitalizes_letters() {
        assert_eq!(key_to_text_char(Key::Char('a'), true), Some('A'));
        assert_eq!(key_to_text_char(Key::Char('z'), true), Some('Z'));
        assert_eq!(key_to_text_char(Key::Char('a'), false), Some('a'));
    }

    #[test]
    /// Implements number row uses shift symbols.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the number row uses shift symbols operation. Visibility: private/internal."]
    fn number_row_uses_shift_symbols() {
        let shifted = [')', '!', '@', '#', '$', '%', '^', '&', '*', '('];
        for n in 0..=9 {
            assert_eq!(key_to_text_char(Key::Num(n), false), Some((b'0' + n) as char));
            assert_eq!(key_to_text_char(Key::Num(n), true), Some(shifted[n as usize]));
        }
    }

    #[test]
    /// Implements punctuation uses shift layer.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the punctuation uses shift layer operation. Visibility: private/internal."]
    fn punctuation_uses_shift_layer() {
        let pairs = [
            ('`', '~'),
            ('-', '_'),
            ('=', '+'),
            ('[', '{'),
            (']', '}'),
            ('\\', '|'),
            (';', ':'),
            ('\'', '"'),
            (',', '<'),
            ('.', '>'),
            ('/', '?'),
        ];
        for (plain, shifted) in pairs {
            assert_eq!(key_to_text_char(Key::Char(plain), true), Some(shifted));
        }
    }
}

/// Constant used for font renderer cache limit.
///
/// Visibility: **private**.
const FONT_RENDERER_CACHE_LIMIT: usize = 4;

/// Implements font for style.
///
/// Visibility: **private**.
#[doc = "Implements the font for style operation. Visibility: private/internal."]
fn font_for_style<'a>(
    default: &'a mut Option<FontRenderer>,
    cache: &'a mut HashMap<String, FontRenderer>,
    font: &crate::widgets::style::FontProperties
) -> Option<&'a mut FontRenderer> {
    for family in &font.family {
        let key = format!("{}|{:?}|{:?}", family.to_ascii_lowercase(), font.style, font.weight);
        if !cache.contains_key(&key) {
            // Each FontRenderer owns decoded font data plus a glyph cache. Keep
            // the family/style cache deliberately small so applications that
            // cycle through many fonts do not retain them for the window's
            // entire lifetime. The active family is reloaded on demand.
            if cache.len() >= FONT_RENDERER_CACHE_LIMIT {
                cache.clear();
            }
            if let Ok(renderer) = FontRenderer::system_family(family, font.style, font.weight) {
                cache.insert(key.clone(), renderer);
            }
        }
        if cache.contains_key(&key) {
            return cache.get_mut(&key);
        }
    }
    default.as_mut()
}
