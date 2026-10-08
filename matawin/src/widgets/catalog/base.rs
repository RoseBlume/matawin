//! Base widget identity, common state, styling, and container plumbing.

use std::{ collections::HashMap, sync::{ Arc, Mutex } };

use super::types::Align;
use crate::widgets::{
    Rect,
    WidgetId,
    next_id,
    style::{ InteractionState, StyleHandle, StyleProperties, Stylable },
};

#[derive(Debug, Clone)]
/// Stores the state and data for widget state.
///
/// Visibility: **private**.
struct WidgetState {
    /// Stores bounds.
    ///
    /// Visibility: **private**.
    bounds: Rect,
    /// Stores visible.
    ///
    /// Visibility: **private**.
    visible: bool,
    /// Stores sensitive.
    ///
    /// Visibility: **private**.
    sensitive: bool,
    /// Stores tooltip.
    ///
    /// Visibility: **private**.
    tooltip: Option<String>,
    /// Stores css classes.
    ///
    /// Visibility: **private**.
    css_classes: Vec<String>,
    /// Stores hexpand.
    ///
    /// Visibility: **private**.
    hexpand: bool,
    /// Stores vexpand.
    ///
    /// Visibility: **private**.
    vexpand: bool,
    /// Stores halign.
    ///
    /// Visibility: **private**.
    halign: Align,
    /// Stores valign.
    ///
    /// Visibility: **private**.
    valign: Align,
}

impl Default for WidgetState {
    /// Performs the default operation.
    ///
    /// Visibility: **private**.
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self {
            bounds: Rect::default(),
            visible: true,
            sensitive: true,
            tooltip: None,
            css_classes: Vec::new(),
            hexpand: false,
            vexpand: false,
            halign: Align::Fill,
            valign: Align::Fill,
        }
    }
}

/// Shared base object used by every MataWin software widget.
#[derive(Debug, Clone)]
pub struct Widget {
    /// Stores id.
    ///
    /// Visibility: **private**.
    id: WidgetId,
    /// Stores state.
    ///
    /// Visibility: **private**.
    state: Arc<Mutex<WidgetState>>,
    /// Stores style.
    ///
    /// Visibility: **private**.
    style: StyleHandle,
}

impl Default for Widget {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}

impl Widget {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            id: WidgetId(next_id()),
            state: Arc::new(Mutex::new(WidgetState::default())),
            style: StyleHandle::new(),
        }
    }
    /// Returns the value configured with bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Provides access configured with bounds. Visibility: public."]
    pub fn with_bounds(bounds: Rect) -> Self {
        let widget = Self::new();
        widget.set_bounds(bounds);
        widget
    }
    #[inline]
    #[doc = "Implements the id operation. Visibility: public."]
    pub const fn id(&self) -> WidgetId {
        self.id
    }
    #[inline]
    #[doc = "Implements the bounds operation. Visibility: public."]
    pub fn bounds(&self) -> Rect {
        self.state.lock().unwrap().bounds
    }
    #[inline]
    #[doc = "Sets bounds. Visibility: public."]
    pub fn set_bounds(&self, bounds: Rect) {
        self.state.lock().unwrap().bounds = bounds;
    }
    /// Resolve CSS-like units and conditional style against a viewport.
    #[doc = "Implements the computed style operation. Visibility: public."]
    pub fn computed_style(
        &self,
        viewport: (u32, u32),
        relative_to: Option<Rect>
    ) -> crate::widgets::style::ComputedStyle {
        self.style.computed(self.bounds(), viewport, relative_to)
    }
    #[inline]
    #[doc = "Implements the visible operation. Visibility: public."]
    pub fn visible(&self) -> bool {
        self.state.lock().unwrap().visible
    }
    #[inline]
    #[doc = "Sets visible. Visibility: public."]
    pub fn set_visible(&self, value: bool) {
        self.state.lock().unwrap().visible = value;
    }
    #[inline]
    #[doc = "Implements the sensitive operation. Visibility: public."]
    pub fn sensitive(&self) -> bool {
        self.state.lock().unwrap().sensitive
    }
    #[inline]
    #[doc = "Sets sensitive. Visibility: public."]
    pub fn set_sensitive(&self, value: bool) {
        self.state.lock().unwrap().sensitive = value;
    }
    #[inline]
    #[doc = "Implements the opacity operation. Visibility: public."]
    pub fn opacity(&self) -> f32 {
        self.style.properties().opacity.unwrap_or(1.0)
    }
    #[inline]
    #[doc = "Sets opacity. Visibility: public."]
    pub fn set_opacity(&self, value: f32) {
        self.style.update(|s| {
            s.base.opacity = Some(value.clamp(0.0, 1.0));
        });
    }
    /// Performs the tooltip operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the tooltip operation. Visibility: public."]
    pub fn tooltip(&self) -> Option<String> {
        self.state.lock().unwrap().tooltip.clone()
    }
    /// Sets tooltip.
    ///
    /// Visibility: **public**.
    #[doc = "Sets tooltip. Visibility: public."]
    pub fn set_tooltip<T: Into<String>>(&self, text: Option<T>) {
        self.state.lock().unwrap().tooltip = text.map(Into::into);
    }
    /// Adds css class.
    ///
    /// Visibility: **public**.
    #[doc = "Adds css class. Visibility: public."]
    pub fn add_css_class(&self, class: impl Into<String>) {
        let class = class.into();
        let mut state = self.state.lock().unwrap();
        if !state.css_classes.contains(&class) {
            state.css_classes.push(class);
        }
    }
    /// Removes css class.
    ///
    /// Visibility: **public**.
    #[doc = "Removes css class. Visibility: public."]
    pub fn remove_css_class(&self, class: &str) {
        self.state
            .lock()
            .unwrap()
            .css_classes.retain(|c| c != class);
    }
    /// Performs the css classes operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the css classes operation. Visibility: public."]
    pub fn css_classes(&self) -> Vec<String> {
        self.state.lock().unwrap().css_classes.clone()
    }
    /// Sets hexpand.
    ///
    /// Visibility: **public**.
    #[doc = "Sets hexpand. Visibility: public."]
    pub fn set_hexpand(&self, value: bool) {
        self.state.lock().unwrap().hexpand = value;
    }
    /// Sets vexpand.
    ///
    /// Visibility: **public**.
    #[doc = "Sets vexpand. Visibility: public."]
    pub fn set_vexpand(&self, value: bool) {
        self.state.lock().unwrap().vexpand = value;
    }
    /// Sets halign.
    ///
    /// Visibility: **public**.
    #[doc = "Sets halign. Visibility: public."]
    pub fn set_halign(&self, value: Align) {
        self.state.lock().unwrap().halign = value;
    }
    /// Sets valign.
    ///
    /// Visibility: **public**.
    #[doc = "Sets valign. Visibility: public."]
    pub fn set_valign(&self, value: Align) {
        self.state.lock().unwrap().valign = value;
    }
    #[inline]
    #[doc = "Implements the style handle operation. Visibility: public."]
    pub fn style_handle(&self) -> StyleHandle {
        self.style.clone()
    }
    #[inline]
    #[doc = "Implements the style properties operation. Visibility: public."]
    pub fn style_properties(&self) -> StyleProperties {
        self.style.properties()
    }
    #[inline]
    #[doc = "Sets style properties. Visibility: public."]
    pub fn set_style_properties(&self, properties: StyleProperties) {
        self.style.set_properties(properties);
    }
    #[inline]
    #[doc = "Updates style. Visibility: public."]
    pub fn update_style(&self, f: impl FnOnce(&mut StyleProperties)) {
        self.style.update_properties(f);
    }
    /// Sets interaction state.
    ///
    /// Visibility: **public**.
    #[doc = "Sets interaction state. Visibility: public."]
    pub fn set_interaction_state(&self, state: InteractionState) {
        self.style.set_interaction(state);
    }
}

/// Defines the behavior required for widget ext.
///
/// Visibility: **public**.
pub trait WidgetExt {
    /// Performs the widget operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget;
    #[inline]
    #[doc = "Implements the id operation. Visibility: private/internal."]
    fn id(&self) -> WidgetId {
        self.widget().id()
    }
    #[inline]
    #[doc = "Implements the bounds operation. Visibility: private/internal."]
    fn bounds(&self) -> Rect {
        self.widget().bounds()
    }
    #[inline]
    #[doc = "Sets bounds. Visibility: private/internal."]
    fn set_bounds(&self, bounds: Rect) {
        self.widget().set_bounds(bounds);
    }
    #[inline]
    #[doc = "Sets visible. Visibility: private/internal."]
    fn set_visible(&self, value: bool) {
        self.widget().set_visible(value);
    }
    #[inline]
    #[doc = "Sets sensitive. Visibility: private/internal."]
    fn set_sensitive(&self, value: bool) {
        self.widget().set_sensitive(value);
    }
    #[inline]
    #[doc = "Implements the computed style operation. Visibility: private/internal."]
    fn computed_style(
        &self,
        viewport: (u32, u32),
        relative_to: Option<Rect>
    ) -> crate::widgets::style::ComputedStyle {
        self.widget().computed_style(viewport, relative_to)
    }
}
impl WidgetExt for Widget {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self
    }
}
impl<T: WidgetExt> Stylable for T {
    #[doc = "Implements the style handle operation. Visibility: private/internal."]
    fn style_handle(&self) -> StyleHandle {
        self.widget().style_handle()
    }
}

/// Common identity trait accepted by containers.
pub trait WidgetLike {
    /// Performs the widget id operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the widget id operation. Visibility: private/internal."]
    fn widget_id(&self) -> WidgetId;
    /// Performs the widget style operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the widget style operation. Visibility: private/internal."]
    fn widget_style(&self) -> Option<StyleHandle> {
        None
    }
}
impl<T: WidgetExt> WidgetLike for T {
    /// Performs the widget id operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the widget id operation. Visibility: private/internal."]
    fn widget_id(&self) -> WidgetId {
        self.id()
    }
    /// Performs the widget style operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the widget style operation. Visibility: private/internal."]
    fn widget_style(&self) -> Option<StyleHandle> {
        Some(self.widget().style_handle())
    }
}
impl WidgetLike for crate::widgets::Button {
    #[doc = "Implements the widget id operation. Visibility: private/internal."]
    fn widget_id(&self) -> WidgetId {
        self.id()
    }
    #[doc = "Implements the widget style operation. Visibility: private/internal."]
    fn widget_style(&self) -> Option<StyleHandle> {
        Some(self.style_handle())
    }
}
impl WidgetLike for crate::widgets::TextField {
    #[doc = "Implements the widget id operation. Visibility: private/internal."]
    fn widget_id(&self) -> WidgetId {
        self.id()
    }
    #[doc = "Implements the widget style operation. Visibility: private/internal."]
    fn widget_style(&self) -> Option<StyleHandle> {
        Some(self.style_handle())
    }
}
#[cfg(feature = "webview")]
impl WidgetLike for crate::widgets::WebView {
    #[doc = "Implements the widget id operation. Visibility: private/internal."]
    fn widget_id(&self) -> WidgetId {
        self.id()
    }
    #[doc = "Implements the widget style operation. Visibility: private/internal."]
    fn widget_style(&self) -> Option<StyleHandle> {
        Some(self.style_handle())
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for container state.
///
/// Visibility: **public**.
pub struct ContainerState {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores children.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) children: Arc<Mutex<Vec<WidgetId>>>,
    /// Stores child styles.
    ///
    /// Visibility: **private**.
    child_styles: Arc<Mutex<HashMap<WidgetId, StyleHandle>>>,
}
impl Default for ContainerState {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl ContainerState {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            children: Arc::new(Mutex::new(Vec::new())),
            child_styles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    /// Performs the append operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the append operation. Visibility: public."]
    pub fn append<W: WidgetLike>(&self, child: &W) {
        self.children.lock().unwrap().push(child.widget_id());
        self.remember(child);
        self.sync_structure();
    }
    /// Performs the prepend operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the prepend operation. Visibility: public."]
    pub fn prepend<W: WidgetLike>(&self, child: &W) {
        self.children.lock().unwrap().insert(0, child.widget_id());
        self.remember(child);
        self.sync_structure();
    }
    /// Removes .
    ///
    /// Visibility: **public**.
    #[doc = "Implements the remove operation. Visibility: public."]
    pub fn remove<W: WidgetLike>(&self, child: &W) {
        self.children
            .lock()
            .unwrap()
            .retain(|id| *id != child.widget_id());
        self.child_styles.lock().unwrap().remove(&child.widget_id());
        self.sync_structure();
    }
    /// Performs the children operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the children operation. Visibility: public."]
    pub fn children(&self) -> Vec<WidgetId> {
        self.children.lock().unwrap().clone()
    }
    /// Sets child at.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Sets child at. Visibility: crate-visible."]
    pub(crate) fn set_child_at<W: WidgetLike>(&self, index: usize, child: &W) {
        let id = child.widget_id();
        let mut c = self.children.lock().unwrap();
        while c.len() <= index {
            c.push(id);
        }
        c[index] = id;
        drop(c);
        self.remember(child);
        self.sync_structure()
    }
    /// Performs the remember operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the remember operation. Visibility: private/internal."]
    fn remember<W: WidgetLike>(&self, child: &W) {
        if let Some(style) = child.widget_style() {
            self.child_styles.lock().unwrap().insert(child.widget_id(), style);
        }
    }
    /// Performs the sync structure operation.
    ///
    /// Visibility: **private**.
    #[doc = "Synchronizes structure. Visibility: private/internal."]
    fn sync_structure(&self) {
        let ids = self.children();
        let count = ids.len();
        let styles = self.child_styles.lock().unwrap();
        for (index, id) in ids.into_iter().enumerate() {
            if let Some(style) = styles.get(&id) {
                let mut tree = style.tree_state();
                tree.child_index = Some(index);
                tree.sibling_count = Some(count);
                style.set_tree_state(tree);
            }
        }
    }
}
impl WidgetExt for ContainerState {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}
