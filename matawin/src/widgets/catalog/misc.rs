//! Thin catalog wrappers whose behavior is entirely inherited from the base widget.

use crate::widgets::Rect;
use super::base::{ ContainerState, Widget, WidgetExt, WidgetLike };

macro_rules! simple_widget {
    ($($name:ident),* $(,)?) => {
        $(
        #[derive(Debug, Clone, Default)]
        #[doc = "Generated catalog widget backed by the shared widget state. Visibility: public."]
        pub struct $name {
            #[doc = "Stores the shared widget state. Visibility: private."]
            widget: Widget,
        }
        impl $name {
            /// Creates a new instance.
            ///
            /// Visibility: **public**.
            #[doc = "Creates a new value. Visibility: public."] pub fn new() -> Self { Self { widget: Widget::new() } }
            /// Returns the value configured with bounds.
            ///
            /// Visibility: **public**.
            #[doc = "Provides access configured with bounds. Visibility: public."] pub fn with_bounds(bounds: Rect) -> Self { Self { widget: Widget::with_bounds(bounds) } }
        }
        impl WidgetExt for $name { #[doc = "Implements the widget operation. Visibility: private/internal."] fn widget(&self) -> &Widget { &self.widget } }
        )*
    };
}

macro_rules! container_widget {
    ($($name:ident),* $(,)?) => {
        $(
        #[derive(Debug, Clone, Default)]
        #[doc = "Generated container widget. Visibility: public."]
        pub struct $name {
            #[doc = "Stores the shared container state. Visibility: private."]
            inner: ContainerState,
        }
        impl $name {
            /// Creates a new instance.
            ///
            /// Visibility: **public**.
            #[doc = "Creates a new value. Visibility: public."] pub fn new() -> Self { Self { inner: ContainerState::new() } }
            /// Performs the append operation.
            ///
            /// Visibility: **public**.
            #[doc = "Implements the append operation. Visibility: public."] pub fn append<W: WidgetLike>(&self, child: &W) { self.inner.append(child); }
            /// Performs the prepend operation.
            ///
            /// Visibility: **public**.
            #[doc = "Implements the prepend operation. Visibility: public."] pub fn prepend<W: WidgetLike>(&self, child: &W) { self.inner.prepend(child); }
            /// Removes .
            ///
            /// Visibility: **public**.
            #[doc = "Implements the remove operation. Visibility: public."] pub fn remove<W: WidgetLike>(&self, child: &W) { self.inner.remove(child); }
            /// Performs the children operation.
            ///
            /// Visibility: **public**.
            #[doc = "Implements the children operation. Visibility: public."] pub fn children(&self) -> Vec<crate::widgets::WidgetId> { self.inner.children() }
        }
        impl WidgetExt for $name { #[doc = "Implements the widget operation. Visibility: private/internal."] fn widget(&self) -> &Widget { self.inner.widget() } }
        )*
    };
}

simple_widget!(
    AboutDialog,
    Dialog,
    AppChooserDialog,
    ColorChooserDialog,
    FileChooserDialog,
    FontChooserDialog,
    MessageDialog,
    PageSetupUnixDialog,
    PrintUnixDialog,
    ApplicationWindow,
    Assistant,
    ShortcutsWindow,
    ActionBar,
    AppChooserButton,
    AppChooserWidget,
    AspectFrame,
    ShortcutsGroup,
    ShortcutsSection,
    LockButton,
    CellView,
    CenterBox,
    ColorButton,
    ColorChooserWidget,
    ColorDialogButton,
    DragIcon,
    FileChooserWidget,
    FlowBoxChild,
    FontButton,
    FontChooserWidget,
    FontDialogButton,
    Frame,
    GraphicsOffload,
    HeaderBar,
    InfoBar,
    ListBase,
    MenuButton,
    PopoverBin,
    PopoverMenuBar,
    SearchBar,
    Separator,
    StackSidebar,
    StackSwitcher,
    TreeExpander,
    Viewport,
    WindowControls,
    WindowHandle,
    EmojiChooser,
    Popover,
    PopoverMenu,
    Expander
);

/// Link-like widget. `:link` is active by default; call [`LinkButton::set_visited`]
/// after navigation to activate `:visited` rules.
#[derive(Debug, Clone)]
/// Stores the state and data for link button.
///
/// Visibility: **public**.
pub struct LinkButton {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
}
impl Default for LinkButton {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl LinkButton {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        let widget = Widget::new();
        let mut state = widget.style_handle().interaction();
        state.link = true;
        widget.style_handle().set_interaction(state);
        Self { widget }
    }
    /// Returns the value configured with bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with bounds. Visibility: public."]
    pub fn with_bounds(bounds: Rect) -> Self {
        let this = Self::new();
        this.widget.set_bounds(bounds);
        this
    }
    /// Sets visited.
    ///
    /// Visibility: **public**.
    #[doc = "Sets visited. Visibility: public."]
    pub fn set_visited(&self, visited: bool) {
        let h = self.widget.style_handle();
        let mut state = h.interaction();
        state.visited = visited;
        h.set_interaction(state)
    }
}
impl WidgetExt for LinkButton {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

container_widget!(
    Fixed,
    FlowBox,
    Grid,
    Overlay,
    ShortcutsGroupContainer,
    ShortcutsSectionContainer
);

/// Alias matching GTK's top-level class name without replacing MataWin's
/// platform `crate::Window`.
pub type Window = ApplicationWindow;
