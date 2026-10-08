//! Text-oriented catalog widgets.

use std::sync::{ Arc, Mutex };

use crate::Framebuffer;
use crate::widgets::{ FontRenderer, WidgetError };
use super::base::{ Widget, WidgetExt };

#[derive(Debug, Clone)]
/// Stores the state and data for text state.
///
/// Visibility: **private**.
struct TextState {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores text.
    ///
    /// Visibility: **private**.
    text: Arc<Mutex<String>>,
}
impl TextState {
    /// Creates a new instance.
    ///
    /// Visibility: **private**.
    #[doc = "Creates a new value. Visibility: private/internal."]
    fn new(text: impl Into<String>) -> Self {
        Self { widget: Widget::new(), text: Arc::new(Mutex::new(text.into())) }
    }
    /// Performs the text operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the text operation. Visibility: private/internal."]
    fn text(&self) -> String {
        self.text.lock().unwrap().clone()
    }
    /// Sets text.
    ///
    /// Visibility: **private**.
    #[doc = "Sets text. Visibility: private/internal."]
    fn set_text(&self, text: impl Into<String>) {
        *self.text.lock().unwrap() = text.into();
    }
}

macro_rules! text_widget {
    ($($name:ident),* $(,)?) => {
        $(
        #[derive(Debug, Clone)]
        #[doc = "Generated text-oriented catalog widget. Visibility: public."]
        pub struct $name {
            #[doc = "Stores the shared text-widget state. Visibility: private."]
            inner: TextState,
        }
        impl Default for $name { #[doc = "Creates the default value. Visibility: private/internal."] fn default() -> Self { Self::new("") } }
        impl $name {
            /// Creates a new instance.
            ///
            /// Visibility: **public**.
            #[doc = "Creates a new value. Visibility: public."] pub fn new(text: impl Into<String>) -> Self { Self { inner: TextState::new(text) } }
            /// Performs the text operation.
            ///
            /// Visibility: **public**.
            #[doc = "Implements the text operation. Visibility: public."] pub fn text(&self) -> String { self.inner.text() }
            /// Sets text.
            ///
            /// Visibility: **public**.
            #[doc = "Sets text. Visibility: public."] pub fn set_text(&self, text: impl Into<String>) { self.inner.set_text(text); }
            /// Draws this text with `matafonts` into a MataWin framebuffer.
            #[doc = "Implements the blit operation. Visibility: public."] pub fn blit(&self, target: &mut Framebuffer, font: &mut FontRenderer, pt: u16, color: u32) -> Result<(), WidgetError> {
                let viewport=(target.width,target.height);
                let computed=self.inner.widget.computed_style(viewport,None);
                let style=self.inner.widget.style_handle().properties();
                let pt=if style.font.size.is_some(){computed.font_size_px.round().clamp(1.0,u16::MAX as f32) as u16}else{pt};
                let color=computed.foreground_color.unwrap_or(color);
                let alpha=((color>>24)&0xff) as f32*computed.opacity;
                let color=((alpha.round().clamp(0.0,255.0) as u32)<<24)|(color&0x00ff_ffff);
                let text=self.text();
                let bounds=computed.content_bounds;
                font.draw_text_styled(target,&text,bounds.x,bounds.y,pt,color,Some(bounds),computed.font.style,computed.font.variant,computed.font.weight,computed.font.stretch,Some(computed.line_height_px.round().max(1.0) as u32))
            }
        }
        impl WidgetExt for $name { #[doc = "Implements the widget operation. Visibility: private/internal."] fn widget(&self) -> &Widget { &self.inner.widget } }
        )*
    };
}

text_widget!(
    Label,
    Inscription,
    EditableLabel,
    Entry,
    PasswordEntry,
    SearchEntry,
    Text,
    TextView,
    ShortcutLabel,
    ShortcutsShortcut,
    Statusbar
);
