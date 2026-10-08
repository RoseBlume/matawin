//! Layout and child-container widgets.

use std::{ collections::BTreeMap, sync::{ Arc, Mutex } };

use crate::widgets::WidgetId;
use super::{
    base::{ ContainerState, Widget, WidgetExt, WidgetLike },
    types::{ Orientation, PolicyType, RevealerTransition },
};

#[derive(Debug, Clone)]
/// Stores the state and data for box.
///
/// Visibility: **public**.
pub struct Box {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: ContainerState,
    /// Stores orientation.
    ///
    /// Visibility: **private**.
    orientation: Orientation,
    /// Stores spacing.
    ///
    /// Visibility: **private**.
    spacing: Arc<Mutex<i32>>,
}
impl Box {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(orientation: Orientation, spacing: i32) -> Self {
        Self { inner: ContainerState::new(), orientation, spacing: Arc::new(Mutex::new(spacing)) }
    }
    #[doc = "Implements the append operation. Visibility: public."]
    pub fn append<W: WidgetLike>(&self, w: &W) {
        self.inner.append(w)
    }
    #[doc = "Implements the children operation. Visibility: public."]
    pub fn children(&self) -> Vec<WidgetId> {
        self.inner.children()
    }
    #[doc = "Implements the orientation operation. Visibility: public."]
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }
    #[doc = "Implements the spacing operation. Visibility: public."]
    pub fn spacing(&self) -> i32 {
        *self.spacing.lock().unwrap()
    }
}
impl WidgetExt for Box {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.inner.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for paned.
///
/// Visibility: **public**.
pub struct Paned {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: ContainerState,
    /// Stores orientation.
    ///
    /// Visibility: **private**.
    orientation: Orientation,
    /// Stores position.
    ///
    /// Visibility: **private**.
    position: Arc<Mutex<i32>>,
}
impl Paned {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(orientation: Orientation) -> Self {
        Self { inner: ContainerState::new(), orientation, position: Arc::new(Mutex::new(0)) }
    }
    #[doc = "Sets start child. Visibility: public."]
    pub fn set_start_child<W: WidgetLike>(&self, w: &W) {
        self.inner.set_child_at(0, w)
    }
    #[doc = "Sets end child. Visibility: public."]
    pub fn set_end_child<W: WidgetLike>(&self, w: &W) {
        self.inner.set_child_at(1, w)
    }
    #[doc = "Sets position. Visibility: public."]
    pub fn set_position(&self, p: i32) {
        *self.position.lock().unwrap() = p;
    }
    #[doc = "Implements the orientation operation. Visibility: public."]
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }
}
impl WidgetExt for Paned {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.inner.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for stack.
///
/// Visibility: **public**.
pub struct Stack {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores pages.
    ///
    /// Visibility: **private**.
    pages: Arc<Mutex<BTreeMap<String, WidgetId>>>,
    /// Stores visible.
    ///
    /// Visibility: **private**.
    visible: Arc<Mutex<Option<String>>>,
}
impl Default for Stack {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Stack {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            pages: Arc::new(Mutex::new(BTreeMap::new())),
            visible: Arc::new(Mutex::new(None)),
        }
    }
    #[doc = "Adds named. Visibility: public."]
    pub fn add_named<W: WidgetLike>(&self, child: &W, name: impl Into<String>) {
        let name = name.into();
        self.pages.lock().unwrap().insert(name.clone(), child.widget_id());
        if self.visible.lock().unwrap().is_none() {
            *self.visible.lock().unwrap() = Some(name);
        }
    }
    #[doc = "Sets visible child name. Visibility: public."]
    pub fn set_visible_child_name(&self, name: &str) {
        if self.pages.lock().unwrap().contains_key(name) {
            *self.visible.lock().unwrap() = Some(name.to_string());
        }
    }
    #[doc = "Implements the visible child name operation. Visibility: public."]
    pub fn visible_child_name(&self) -> Option<String> {
        self.visible.lock().unwrap().clone()
    }
}
impl WidgetExt for Stack {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for notebook.
///
/// Visibility: **public**.
pub struct Notebook {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores pages.
    ///
    /// Visibility: **private**.
    pages: Arc<Mutex<Vec<(String, WidgetId)>>>,
    /// Stores current.
    ///
    /// Visibility: **private**.
    current: Arc<Mutex<usize>>,
}
impl Default for Notebook {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Notebook {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            pages: Arc::new(Mutex::new(Vec::new())),
            current: Arc::new(Mutex::new(0)),
        }
    }
    #[doc = "Implements the append page operation. Visibility: public."]
    pub fn append_page<W: WidgetLike>(&self, child: &W, label: impl Into<String>) {
        self.pages.lock().unwrap().push((label.into(), child.widget_id()))
    }
    #[doc = "Sets current page. Visibility: public."]
    pub fn set_current_page(&self, index: usize) {
        if index < self.pages.lock().unwrap().len() {
            *self.current.lock().unwrap() = index;
        }
    }
    #[doc = "Implements the current page operation. Visibility: public."]
    pub fn current_page(&self) -> usize {
        *self.current.lock().unwrap()
    }
}
impl WidgetExt for Notebook {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for revealer.
///
/// Visibility: **public**.
pub struct Revealer {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: ContainerState,
    /// Stores revealed.
    ///
    /// Visibility: **private**.
    revealed: Arc<Mutex<bool>>,
    /// Stores transition.
    ///
    /// Visibility: **private**.
    transition: Arc<Mutex<RevealerTransition>>,
}
impl Default for Revealer {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Revealer {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            inner: ContainerState::new(),
            revealed: Arc::new(Mutex::new(false)),
            transition: Arc::new(Mutex::new(RevealerTransition::default())),
        }
    }
    #[doc = "Sets child. Visibility: public."]
    pub fn set_child<W: WidgetLike>(&self, w: &W) {
        let mut c = self.inner.children.lock().unwrap();
        c.clear();
        c.push(w.widget_id())
    }
    #[doc = "Sets reveal child. Visibility: public."]
    pub fn set_reveal_child(&self, v: bool) {
        *self.revealed.lock().unwrap() = v;
    }
    #[doc = "Implements the reveal child operation. Visibility: public."]
    pub fn reveal_child(&self) -> bool {
        *self.revealed.lock().unwrap()
    }
    #[doc = "Sets transition type. Visibility: public."]
    pub fn set_transition_type(&self, t: RevealerTransition) {
        *self.transition.lock().unwrap() = t;
    }
}
impl WidgetExt for Revealer {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.inner.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for scrolled window.
///
/// Visibility: **public**.
pub struct ScrolledWindow {
    /// Stores inner.
    ///
    /// Visibility: **private**.
    inner: ContainerState,
    /// Stores hpolicy.
    ///
    /// Visibility: **private**.
    hpolicy: Arc<Mutex<PolicyType>>,
    /// Stores vpolicy.
    ///
    /// Visibility: **private**.
    vpolicy: Arc<Mutex<PolicyType>>,
}
impl Default for ScrolledWindow {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl ScrolledWindow {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            inner: ContainerState::new(),
            hpolicy: Arc::new(Mutex::new(PolicyType::Automatic)),
            vpolicy: Arc::new(Mutex::new(PolicyType::Automatic)),
        }
    }
    #[doc = "Sets child. Visibility: public."]
    pub fn set_child<W: WidgetLike>(&self, w: &W) {
        let mut c = self.inner.children.lock().unwrap();
        c.clear();
        c.push(w.widget_id())
    }
    #[doc = "Sets policy. Visibility: public."]
    pub fn set_policy(&self, h: PolicyType, v: PolicyType) {
        *self.hpolicy.lock().unwrap() = h;
        *self.vpolicy.lock().unwrap() = v;
    }
}
impl WidgetExt for ScrolledWindow {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.inner.widget()
    }
}
