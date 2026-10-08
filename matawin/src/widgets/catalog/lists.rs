//! List, grid, and column view widgets.

use std::sync::{ Arc, Mutex };

use super::{ base::{ Widget, WidgetExt }, types::{ Orientation, SelectionMode } };

#[derive(Debug, Clone)]
/// Stores the state and data for list box.
///
/// Visibility: **public**.
pub struct ListBox {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores rows.
    ///
    /// Visibility: **private**.
    rows: Arc<Mutex<Vec<String>>>,
    /// Stores selected.
    ///
    /// Visibility: **private**.
    selected: Arc<Mutex<Vec<usize>>>,
    /// Stores mode.
    ///
    /// Visibility: **private**.
    mode: SelectionMode,
}
impl ListBox {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(mode: SelectionMode) -> Self {
        Self {
            widget: Widget::new(),
            rows: Arc::new(Mutex::new(Vec::new())),
            selected: Arc::new(Mutex::new(Vec::new())),
            mode,
        }
    }
    #[doc = "Implements the append operation. Visibility: public."]
    pub fn append(&self, text: impl Into<String>) {
        self.rows.lock().unwrap().push(text.into())
    }
    #[doc = "Implements the rows operation. Visibility: public."]
    pub fn rows(&self) -> Vec<String> {
        self.rows.lock().unwrap().clone()
    }
    #[doc = "Implements the select operation. Visibility: public."]
    pub fn select(&self, index: usize) {
        if index >= self.rows.lock().unwrap().len() {
            return;
        }
        let mut s = self.selected.lock().unwrap();
        match self.mode {
            SelectionMode::None => {}
            SelectionMode::Multiple => if !s.contains(&index) {
                s.push(index)
            }
            _ => {
                s.clear();
                s.push(index)
            }
        }
    }
    #[doc = "Implements the selected operation. Visibility: public."]
    pub fn selected(&self) -> Vec<usize> {
        self.selected.lock().unwrap().clone()
    }
}
impl WidgetExt for ListBox {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for list box row.
///
/// Visibility: **public**.
pub struct ListBoxRow {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores text.
    ///
    /// Visibility: **private**.
    text: Arc<Mutex<String>>,
}
impl ListBoxRow {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(text: impl Into<String>) -> Self {
        Self { widget: Widget::new(), text: Arc::new(Mutex::new(text.into())) }
    }
    #[doc = "Implements the text operation. Visibility: public."]
    pub fn text(&self) -> String {
        self.text.lock().unwrap().clone()
    }
}
impl WidgetExt for ListBoxRow {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for list view.
///
/// Visibility: **public**.
pub struct ListView {
    /// Stores list.
    ///
    /// Visibility: **private**.
    list: ListBox,
    /// Stores orientation.
    ///
    /// Visibility: **private**.
    orientation: Orientation,
    /// Stores show separators.
    ///
    /// Visibility: **private**.
    show_separators: Arc<Mutex<bool>>,
}
impl ListView {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(mode: SelectionMode) -> Self {
        Self {
            list: ListBox::new(mode),
            orientation: Orientation::Vertical,
            show_separators: Arc::new(Mutex::new(false)),
        }
    }
    #[doc = "Implements the append operation. Visibility: public."]
    pub fn append(&self, text: impl Into<String>) {
        self.list.append(text)
    }
    #[doc = "Implements the rows operation. Visibility: public."]
    pub fn rows(&self) -> Vec<String> {
        self.list.rows()
    }
    #[doc = "Sets show separators. Visibility: public."]
    pub fn set_show_separators(&self, v: bool) {
        *self.show_separators.lock().unwrap() = v;
    }
    #[doc = "Sets orientation. Visibility: public."]
    pub fn set_orientation(&mut self, o: Orientation) {
        self.orientation = o;
    }
}
impl WidgetExt for ListView {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.list.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for grid view.
///
/// Visibility: **public**.
pub struct GridView {
    /// Stores list.
    ///
    /// Visibility: **private**.
    list: ListBox,
    /// Stores min columns.
    ///
    /// Visibility: **private**.
    min_columns: Arc<Mutex<u32>>,
    /// Stores max columns.
    ///
    /// Visibility: **private**.
    max_columns: Arc<Mutex<u32>>,
}
impl GridView {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(mode: SelectionMode) -> Self {
        Self {
            list: ListBox::new(mode),
            min_columns: Arc::new(Mutex::new(1)),
            max_columns: Arc::new(Mutex::new(u32::MAX)),
        }
    }
    #[doc = "Implements the append operation. Visibility: public."]
    pub fn append(&self, text: impl Into<String>) {
        self.list.append(text)
    }
    #[doc = "Sets min columns. Visibility: public."]
    pub fn set_min_columns(&self, v: u32) {
        *self.min_columns.lock().unwrap() = v.max(1);
    }
    #[doc = "Sets max columns. Visibility: public."]
    pub fn set_max_columns(&self, v: u32) {
        *self.max_columns.lock().unwrap() = v.max(1);
    }
}
impl WidgetExt for GridView {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.list.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for column view.
///
/// Visibility: **public**.
pub struct ColumnView {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores columns.
    ///
    /// Visibility: **private**.
    columns: Arc<Mutex<Vec<String>>>,
    /// Stores rows.
    ///
    /// Visibility: **private**.
    rows: Arc<Mutex<Vec<Vec<String>>>>,
}
impl Default for ColumnView {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl ColumnView {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            columns: Arc::new(Mutex::new(Vec::new())),
            rows: Arc::new(Mutex::new(Vec::new())),
        }
    }
    #[doc = "Implements the append column operation. Visibility: public."]
    pub fn append_column(&self, name: impl Into<String>) {
        self.columns.lock().unwrap().push(name.into())
    }
    #[doc = "Implements the append row operation. Visibility: public."]
    pub fn append_row<I, S>(&self, row: I) where I: IntoIterator<Item = S>, S: Into<String> {
        self.rows.lock().unwrap().push(row.into_iter().map(Into::into).collect())
    }
    #[doc = "Implements the columns operation. Visibility: public."]
    pub fn columns(&self) -> Vec<String> {
        self.columns.lock().unwrap().clone()
    }
    #[doc = "Implements the rows operation. Visibility: public."]
    pub fn rows(&self) -> Vec<Vec<String>> {
        self.rows.lock().unwrap().clone()
    }
}
impl WidgetExt for ColumnView {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

/// Type alias for tree view.
///
/// Visibility: **public**.
pub type TreeView = ColumnView;
/// Type alias for icon view.
///
/// Visibility: **public**.
pub type IconView = GridView;
