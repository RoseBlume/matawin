//! Native software controls that participate directly in [`WidgetWindow`].

use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    sync::{ Arc, Mutex, atomic::{ AtomicBool, Ordering } },
};

use super::{
    geometry::{ Anchors, Rect, ResizePolicy, ResizeState },
    events::{ push_event, WidgetError, WidgetEvent, WidgetId, next_id },
    style::{ ComputedStyle, StyleHandle, StyleProperties, Stylable, TreeState },
    window::WidgetWindow,
};

#[derive(Debug)]
/// Stores the state and data for button state.
///
/// Visibility: **crate-visible**.
pub(crate) struct ButtonState {
    /// Stores bounds.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) bounds: Rect,
    /// Stores layout base.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) layout_base: Rect,
    /// Stores resize.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) resize: ResizeState,
    /// Stores label.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) label: String,
    /// Stores visible.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) visible: bool,
    /// Stores enabled.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) enabled: bool,
    /// Stores hovered.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) hovered: bool,
    /// Stores pressed.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) pressed: bool,
    /// Stores alive.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) alive: bool,
    /// Stores style.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) style: StyleHandle,
    /// Stores computed.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) computed: ComputedStyle,
}
#[derive(Debug)]
/// Stores the state and data for text field state.
///
/// Visibility: **crate-visible**.
pub(crate) struct TextFieldState {
    /// Stores bounds.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) bounds: Rect,
    /// Stores layout base.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) layout_base: Rect,
    /// Stores resize.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) resize: ResizeState,
    /// Stores text.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) text: String,
    /// Stores cursor.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) cursor: usize,
    /// Stores visible.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) visible: bool,
    /// Stores enabled.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) enabled: bool,
    /// Stores hovered.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) hovered: bool,
    /// Stores pressed.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) pressed: bool,
    /// Stores alive.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) alive: bool,
    /// Stores style.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) style: StyleHandle,
    /// Stores computed.
    ///
    /// Visibility: **crate-visible**.
    pub(crate) computed: ComputedStyle,
}

#[derive(Clone)]
/// Defines the available control entry choices.
///
/// Visibility: **crate-visible**.
pub(crate) enum ControlEntry {
    /// Represents the button variant.
    ///
    /// Visibility: inherited from the **crate-visible** enum.
    Button {
        id: WidgetId,
        state: Arc<Mutex<ButtonState>>,
    },
    /// Represents the text field variant.
    ///
    /// Visibility: inherited from the **crate-visible** enum.
    TextField {
        id: WidgetId,
        state: Arc<Mutex<TextFieldState>>,
    },
}
impl ControlEntry {
    /// Performs the id operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the id operation. Visibility: crate-visible."]
    pub(crate) fn id(&self) -> WidgetId {
        match self {
            Self::Button { id, .. } | Self::TextField { id, .. } => *id,
        }
    }
    /// Performs the bounds operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the bounds operation. Visibility: crate-visible."]
    pub(crate) fn bounds(&self) -> Rect {
        match self {
            Self::Button { state, .. } => state.lock().unwrap().bounds,
            Self::TextField { state, .. } => state.lock().unwrap().bounds,
        }
    }
    /// Performs the layout base operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the layout base operation. Visibility: crate-visible."]
    pub(crate) fn layout_base(&self) -> Rect {
        match self {
            Self::Button { state, .. } => state.lock().unwrap().layout_base,
            Self::TextField { state, .. } => state.lock().unwrap().layout_base,
        }
    }
    /// Performs the style handle operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the style handle operation. Visibility: crate-visible."]
    pub(crate) fn style_handle(&self) -> StyleHandle {
        match self {
            Self::Button { state, .. } => state.lock().unwrap().style.clone(),
            Self::TextField { state, .. } => state.lock().unwrap().style.clone(),
        }
    }
    /// Performs the z index operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the z index operation. Visibility: crate-visible."]
    pub(crate) fn z_index(&self) -> i32 {
        match self {
            Self::Button { state, .. } => state.lock().unwrap().computed.z_index,
            Self::TextField { state, .. } => state.lock().unwrap().computed.z_index,
        }
    }
    /// Performs the animation running operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the animation running operation. Visibility: crate-visible."]
    pub(crate) fn animation_running(&self) -> bool {
        match self {
            Self::Button { state, .. } => state.lock().unwrap().style.animation_running(),
            Self::TextField { state, .. } => state.lock().unwrap().style.animation_running(),
        }
    }
    /// Performs the alive visible operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the alive visible operation. Visibility: crate-visible."]
    pub(crate) fn alive_visible(&self) -> bool {
        match self {
            Self::Button { state, .. } => {
                let s = state.lock().unwrap();
                s.alive && s.visible
            }
            Self::TextField { state, .. } => {
                let s = state.lock().unwrap();
                s.alive && s.visible
            }
        }
    }
    /// Returns whether text.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Returns whether text. Visibility: crate-visible."]
    pub(crate) fn is_text(&self) -> bool {
        matches!(self, Self::TextField { .. })
    }
    /// Performs the apply resize operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the apply resize operation. Visibility: crate-visible."]
    pub(crate) fn apply_resize(&self, size: (u32, u32)) {
        match self {
            Self::Button { state, .. } => {
                let mut s = state.lock().unwrap();
                s.layout_base = s.resize.resolve(size);
                s.bounds = s.layout_base;
            }
            Self::TextField { state, .. } => {
                let mut s = state.lock().unwrap();
                s.layout_base = s.resize.resolve(size);
                s.bounds = s.layout_base;
            }
        }
    }
    /// Resolves style from the current state.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Resolves style. Visibility: crate-visible."]
    pub(crate) fn resolve_style(
        &self,
        size: (u32, u32),
        references: &HashMap<WidgetId, Rect>,
        focused: Option<WidgetId>
    ) {
        match self {
            Self::Button { id, state } => {
                let mut s = state.lock().unwrap();
                let mut i = s.style.interaction();
                i.hover = s.hovered;
                i.active = s.pressed;
                i.focus = focused == Some(*id);
                s.style.set_interaction(i);
                let reference = s.style.relative_to().and_then(|id| references.get(&id).copied());
                s.computed = s.style.computed(s.layout_base, size, reference);
                s.bounds = s.computed.bounds;
            }
            Self::TextField { id, state } => {
                let mut s = state.lock().unwrap();
                let mut i = s.style.interaction();
                i.hover = s.hovered;
                i.active = s.pressed;
                i.focus = focused == Some(*id);
                s.style.set_interaction(i);
                let reference = s.style.relative_to().and_then(|id| references.get(&id).copied());
                s.computed = s.style.computed(s.layout_base, size, reference);
                s.bounds = s.computed.bounds;
            }
        }
    }
}

#[derive(Clone)]
/// Stores the state and data for button.
///
/// Visibility: **public**.
pub struct Button {
    /// Stores id.
    ///
    /// Visibility: **private**.
    id: WidgetId,
    /// Stores state.
    ///
    /// Visibility: **private**.
    state: Arc<Mutex<ButtonState>>,
    /// Stores host size.
    ///
    /// Visibility: **private**.
    host_size: Rc<Cell<(u32, u32)>>,
    /// Stores dirty.
    ///
    /// Visibility: **private**.
    dirty: Arc<AtomicBool>,
}
impl Button {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(parent: &WidgetWindow, bounds: Rect, label: &str) -> Result<Self, WidgetError> {
        let id = WidgetId(next_id());
        let resize = ResizeState::new(bounds, parent.size());
        let style = StyleHandle::with_invalidator(parent.dirty.clone());
        let mut computed = ComputedStyle::default();
        computed.bounds = bounds;
        computed.content_bounds = bounds;
        let state = Arc::new(
            Mutex::new(ButtonState {
                bounds,
                layout_base: bounds,
                resize,
                label: label.into(),
                visible: true,
                enabled: true,
                hovered: false,
                pressed: false,
                alive: true,
                style,
                computed,
            })
        );
        parent.register(ControlEntry::Button { id, state: state.clone() });
        Ok(Self { id, state, host_size: parent.host_size.clone(), dirty: parent.dirty.clone() })
    }
    #[inline]
    #[doc = "Implements the id operation. Visibility: public."]
    pub const fn id(&self) -> WidgetId {
        self.id
    }
    #[inline]
    #[doc = "Implements the invalidate operation. Visibility: private/internal."]
    fn invalidate(&self) {
        self.dirty.store(true, Ordering::Release)
    }
    /// Performs the style properties operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the style properties operation. Visibility: public."]
    pub fn style_properties(&self) -> StyleProperties {
        self.style_handle().properties()
    }
    /// Sets style properties.
    ///
    /// Visibility: **public**.
    #[doc = "Sets style properties. Visibility: public."]
    pub fn set_style_properties(&self, properties: StyleProperties) {
        self.style_handle().set_properties(properties)
    }
    /// Updates style.
    ///
    /// Visibility: **public**.
    #[doc = "Updates style. Visibility: public."]
    pub fn update_style(&self, f: impl FnOnce(&mut StyleProperties)) {
        self.style_handle().update_properties(f)
    }
    /// Performs the label operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the label operation. Visibility: public."]
    pub fn label(&self) -> String {
        self.state.lock().unwrap().label.clone()
    }
    /// Sets label.
    ///
    /// Visibility: **public**.
    #[doc = "Sets label. Visibility: public."]
    pub fn set_label(&self, label: &str) {
        self.state.lock().unwrap().label = label.into();
        self.invalidate()
    }
    /// Sets bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bounds. Visibility: public."]
    pub fn set_bounds(&self, bounds: Rect) {
        let mut s = self.state.lock().unwrap();
        s.bounds = bounds;
        s.layout_base = bounds;
        s.resize.reset(bounds, self.host_size.get());
        drop(s);
        self.invalidate()
    }
    /// Performs the bounds operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the bounds operation. Visibility: public."]
    pub fn bounds(&self) -> Rect {
        self.state.lock().unwrap().bounds
    }
    /// Sets resize policy.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resize policy. Visibility: public."]
    pub fn set_resize_policy(&self, policy: ResizePolicy) {
        let mut s = self.state.lock().unwrap();
        let bounds = s.layout_base;
        s.resize.set_policy(policy, bounds, self.host_size.get());
        drop(s);
        self.invalidate()
    }
    /// Sets anchors.
    ///
    /// Visibility: **public**.
    #[doc = "Sets anchors. Visibility: public."]
    pub fn set_anchors(&self, anchors: Anchors) {
        self.set_resize_policy(ResizePolicy::Anchored(anchors))
    }
    /// Sets visible.
    ///
    /// Visibility: **public**.
    #[doc = "Sets visible. Visibility: public."]
    pub fn set_visible(&self, v: bool) {
        self.state.lock().unwrap().visible = v;
        self.invalidate()
    }
    /// Sets enabled.
    ///
    /// Visibility: **public**.
    #[doc = "Sets enabled. Visibility: public."]
    pub fn set_enabled(&self, v: bool) {
        self.state.lock().unwrap().enabled = v;
        self.invalidate()
    }
    /// Performs the style handle operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the style handle operation. Visibility: public."]
    pub fn style_handle(&self) -> StyleHandle {
        self.state.lock().unwrap().style.clone()
    }
}
impl Stylable for Button {
    #[doc = "Implements the style handle operation. Visibility: private/internal."]
    fn style_handle(&self) -> StyleHandle {
        Button::style_handle(self)
    }
}
impl Drop for Button {
    #[doc = "Implements the drop operation. Visibility: private/internal."]
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) <= 2 {
            self.state.lock().unwrap().alive = false;
            self.invalidate()
        }
    }
}

#[derive(Clone)]
/// Stores the state and data for text field.
///
/// Visibility: **public**.
pub struct TextField {
    /// Stores id.
    ///
    /// Visibility: **private**.
    id: WidgetId,
    /// Stores host id.
    ///
    /// Visibility: **private**.
    host_id: u64,
    /// Stores state.
    ///
    /// Visibility: **private**.
    state: Arc<Mutex<TextFieldState>>,
    /// Stores focus.
    ///
    /// Visibility: **private**.
    focus: Arc<Mutex<Option<WidgetId>>>,
    /// Stores host size.
    ///
    /// Visibility: **private**.
    host_size: Rc<Cell<(u32, u32)>>,
    /// Stores dirty.
    ///
    /// Visibility: **private**.
    dirty: Arc<AtomicBool>,
}
impl TextField {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(parent: &WidgetWindow, bounds: Rect, text: &str) -> Result<Self, WidgetError> {
        let id = WidgetId(next_id());
        let resize = ResizeState::new(bounds, parent.size());
        let style = StyleHandle::with_invalidator(parent.dirty.clone());
        let mut computed = ComputedStyle::default();
        computed.bounds = bounds;
        computed.content_bounds = bounds;
        let state = Arc::new(
            Mutex::new(TextFieldState {
                bounds,
                layout_base: bounds,
                resize,
                text: text.into(),
                cursor: text.len(),
                visible: true,
                enabled: true,
                hovered: false,
                pressed: false,
                alive: true,
                style,
                computed,
            })
        );
        parent.register(ControlEntry::TextField { id, state: state.clone() });
        Ok(Self {
            id,
            host_id: parent.id,
            state,
            focus: parent.focus.clone(),
            host_size: parent.host_size.clone(),
            dirty: parent.dirty.clone(),
        })
    }
    #[inline]
    #[doc = "Implements the id operation. Visibility: public."]
    pub const fn id(&self) -> WidgetId {
        self.id
    }
    #[inline]
    #[doc = "Implements the invalidate operation. Visibility: private/internal."]
    fn invalidate(&self) {
        self.dirty.store(true, Ordering::Release)
    }
    /// Performs the style properties operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the style properties operation. Visibility: public."]
    pub fn style_properties(&self) -> StyleProperties {
        self.style_handle().properties()
    }
    /// Sets style properties.
    ///
    /// Visibility: **public**.
    #[doc = "Sets style properties. Visibility: public."]
    pub fn set_style_properties(&self, properties: StyleProperties) {
        self.style_handle().set_properties(properties)
    }
    /// Updates style.
    ///
    /// Visibility: **public**.
    #[doc = "Updates style. Visibility: public."]
    pub fn update_style(&self, f: impl FnOnce(&mut StyleProperties)) {
        self.style_handle().update_properties(f)
    }
    /// Performs the text operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the text operation. Visibility: public."]
    pub fn text(&self) -> String {
        self.state.lock().unwrap().text.clone()
    }
    /// Sets text.
    ///
    /// Visibility: **public**.
    #[doc = "Sets text. Visibility: public."]
    pub fn set_text(&self, text: &str) {
        let mut s = self.state.lock().unwrap();
        s.text = text.into();
        s.cursor = s.text.len();
        drop(s);
        self.invalidate();
        push_event(self.host_id, WidgetEvent::TextChanged(self.id))
    }
    /// Sets bounds.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bounds. Visibility: public."]
    pub fn set_bounds(&self, bounds: Rect) {
        let mut s = self.state.lock().unwrap();
        s.bounds = bounds;
        s.layout_base = bounds;
        s.resize.reset(bounds, self.host_size.get());
        drop(s);
        self.invalidate()
    }
    /// Performs the bounds operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the bounds operation. Visibility: public."]
    pub fn bounds(&self) -> Rect {
        self.state.lock().unwrap().bounds
    }
    /// Sets resize policy.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resize policy. Visibility: public."]
    pub fn set_resize_policy(&self, policy: ResizePolicy) {
        let mut s = self.state.lock().unwrap();
        let bounds = s.layout_base;
        s.resize.set_policy(policy, bounds, self.host_size.get());
        drop(s);
        self.invalidate()
    }
    /// Sets anchors.
    ///
    /// Visibility: **public**.
    #[doc = "Sets anchors. Visibility: public."]
    pub fn set_anchors(&self, anchors: Anchors) {
        self.set_resize_policy(ResizePolicy::Anchored(anchors))
    }
    /// Sets visible.
    ///
    /// Visibility: **public**.
    #[doc = "Sets visible. Visibility: public."]
    pub fn set_visible(&self, v: bool) {
        self.state.lock().unwrap().visible = v;
        self.invalidate()
    }
    /// Sets enabled.
    ///
    /// Visibility: **public**.
    #[doc = "Sets enabled. Visibility: public."]
    pub fn set_enabled(&self, v: bool) {
        self.state.lock().unwrap().enabled = v;
        self.invalidate()
    }
    /// Performs the focus operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the focus operation. Visibility: public."]
    pub fn focus(&self) {
        *self.focus.lock().unwrap() = Some(self.id);
        self.invalidate()
    }
    /// Performs the blur operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the blur operation. Visibility: public."]
    pub fn blur(&self) {
        let mut f = self.focus.lock().unwrap();
        if *f == Some(self.id) {
            *f = None;
            drop(f);
            self.invalidate()
        }
    }
    /// Performs the style handle operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the style handle operation. Visibility: public."]
    pub fn style_handle(&self) -> StyleHandle {
        self.state.lock().unwrap().style.clone()
    }
}
impl Stylable for TextField {
    #[doc = "Implements the style handle operation. Visibility: private/internal."]
    fn style_handle(&self) -> StyleHandle {
        TextField::style_handle(self)
    }
}
impl Drop for TextField {
    #[doc = "Implements the drop operation. Visibility: private/internal."]
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) <= 2 {
            self.state.lock().unwrap().alive = false;
            self.blur();
            self.invalidate()
        }
    }
}

/// Implements sync tree states.
///
/// Visibility: **crate-visible**.
#[doc = "Synchronizes tree states. Visibility: crate-visible."]
pub(crate) fn sync_tree_states(controls: &[ControlEntry]) {
    let count = controls.len();
    for (index, c) in controls.iter().enumerate() {
        let h = c.style_handle();
        let mut tree: TreeState = h.tree_state();
        tree.child_index = Some(index);
        tree.sibling_count = Some(count);
        h.set_tree_state(tree);
    }
}
