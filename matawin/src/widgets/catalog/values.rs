//! Numeric, progress, selection, and date widgets.

use std::sync::{ Arc, Mutex };

use super::{ base::{ Widget, WidgetExt }, types::Orientation };

#[derive(Debug, Clone)]
/// Stores the state and data for range.
///
/// Visibility: **public**.
pub struct Range {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores min.
    ///
    /// Visibility: **private**.
    min: f64,
    /// Stores max.
    ///
    /// Visibility: **private**.
    max: f64,
    /// Stores value.
    ///
    /// Visibility: **private**.
    value: Arc<Mutex<f64>>,
}
impl Range {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        Self { widget: Widget::new(), min, max, value: Arc::new(Mutex::new(value.clamp(min, max))) }
    }
    /// Performs the value operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the value operation. Visibility: public."]
    pub fn value(&self) -> f64 {
        *self.value.lock().unwrap()
    }
    /// Sets value.
    ///
    /// Visibility: **public**.
    #[doc = "Sets value. Visibility: public."]
    pub fn set_value(&self, v: f64) {
        *self.value.lock().unwrap() = v.clamp(self.min, self.max);
    }
    /// Performs the min operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the min operation. Visibility: public."]
    pub fn min(&self) -> f64 {
        self.min
    }
    #[doc = "Implements the max operation. Visibility: public."]
    pub fn max(&self) -> f64 {
        self.max
    }
}
impl WidgetExt for Range {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for scale.
///
/// Visibility: **public**.
pub struct Scale {
    /// Stores range.
    ///
    /// Visibility: **private**.
    range: Range,
    /// Stores orientation.
    ///
    /// Visibility: **private**.
    orientation: Orientation,
}
impl Scale {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(orientation: Orientation, min: f64, max: f64, value: f64) -> Self {
        Self { range: Range::new(min, max, value), orientation }
    }
    #[doc = "Implements the value operation. Visibility: public."]
    pub fn value(&self) -> f64 {
        self.range.value()
    }
    #[doc = "Sets value. Visibility: public."]
    pub fn set_value(&self, v: f64) {
        self.range.set_value(v)
    }
    #[doc = "Implements the orientation operation. Visibility: public."]
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }
}
impl WidgetExt for Scale {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.range.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for scrollbar.
///
/// Visibility: **public**.
pub struct Scrollbar {
    /// Stores scale.
    ///
    /// Visibility: **private**.
    scale: Scale,
}
impl Scrollbar {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(orientation: Orientation, min: f64, max: f64) -> Self {
        Self { scale: Scale::new(orientation, min, max, min) }
    }
    #[doc = "Implements the value operation. Visibility: public."]
    pub fn value(&self) -> f64 {
        self.scale.value()
    }
    #[doc = "Sets value. Visibility: public."]
    pub fn set_value(&self, v: f64) {
        self.scale.set_value(v)
    }
}
impl WidgetExt for Scrollbar {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.scale.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for progress bar.
///
/// Visibility: **public**.
pub struct ProgressBar {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores fraction.
    ///
    /// Visibility: **private**.
    fraction: Arc<Mutex<f64>>,
    /// Stores text.
    ///
    /// Visibility: **private**.
    text: Arc<Mutex<Option<String>>>,
    /// Stores pulse step.
    ///
    /// Visibility: **private**.
    pulse_step: Arc<Mutex<f64>>,
}
impl Default for ProgressBar {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl ProgressBar {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            fraction: Arc::new(Mutex::new(0.0)),
            text: Arc::new(Mutex::new(None)),
            pulse_step: Arc::new(Mutex::new(0.1)),
        }
    }
    /// Performs the fraction operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the fraction operation. Visibility: public."]
    pub fn fraction(&self) -> f64 {
        *self.fraction.lock().unwrap()
    }
    #[doc = "Sets fraction. Visibility: public."]
    pub fn set_fraction(&self, v: f64) {
        *self.fraction.lock().unwrap() = v.clamp(0.0, 1.0);
    }
    /// Sets text.
    ///
    /// Visibility: **public**.
    #[doc = "Sets text. Visibility: public."]
    pub fn set_text<T: Into<String>>(&self, t: Option<T>) {
        *self.text.lock().unwrap() = t.map(Into::into);
    }
    #[doc = "Implements the pulse operation. Visibility: public."]
    pub fn pulse(&self) {
        let s = *self.pulse_step.lock().unwrap();
        self.set_fraction((self.fraction() + s) % 1.0)
    }
}
impl WidgetExt for ProgressBar {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for level bar.
///
/// Visibility: **public**.
pub struct LevelBar {
    /// Stores range.
    ///
    /// Visibility: **private**.
    range: Range,
}
impl LevelBar {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self { range: Range::new(min, max, value) }
    }
    #[doc = "Implements the value operation. Visibility: public."]
    pub fn value(&self) -> f64 {
        self.range.value()
    }
    #[doc = "Sets value. Visibility: public."]
    pub fn set_value(&self, v: f64) {
        self.range.set_value(v)
    }
}
impl WidgetExt for LevelBar {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.range.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for spin button.
///
/// Visibility: **public**.
pub struct SpinButton {
    /// Stores range.
    ///
    /// Visibility: **private**.
    range: Range,
    /// Stores step.
    ///
    /// Visibility: **private**.
    step: f64,
    /// Stores digits.
    ///
    /// Visibility: **private**.
    digits: u32,
}
impl SpinButton {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(min: f64, max: f64, step: f64) -> Self {
        Self { range: Range::new(min, max, min), step, digits: 0 }
    }
    #[doc = "Implements the value operation. Visibility: public."]
    pub fn value(&self) -> f64 {
        self.range.value()
    }
    #[doc = "Sets value. Visibility: public."]
    pub fn set_value(&self, v: f64) {
        self.range.set_value(v)
    }
    #[doc = "Implements the spin up operation. Visibility: public."]
    pub fn spin_up(&self) {
        self.set_value(self.value() + self.step)
    }
    #[doc = "Implements the spin down operation. Visibility: public."]
    pub fn spin_down(&self) {
        self.set_value(self.value() - self.step)
    }
    #[doc = "Sets digits. Visibility: public."]
    pub fn set_digits(&mut self, d: u32) {
        self.digits = d;
    }
}
impl WidgetExt for SpinButton {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.range.widget()
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for spinner.
///
/// Visibility: **public**.
pub struct Spinner {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores spinning.
    ///
    /// Visibility: **private**.
    spinning: Arc<Mutex<bool>>,
}
impl Default for Spinner {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Spinner {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self { widget: Widget::new(), spinning: Arc::new(Mutex::new(false)) }
    }
    #[doc = "Implements the start operation. Visibility: public."]
    pub fn start(&self) {
        *self.spinning.lock().unwrap() = true;
    }
    #[doc = "Implements the stop operation. Visibility: public."]
    pub fn stop(&self) {
        *self.spinning.lock().unwrap() = false;
    }
    #[doc = "Implements the spinning operation. Visibility: public."]
    pub fn spinning(&self) -> bool {
        *self.spinning.lock().unwrap()
    }
}
impl WidgetExt for Spinner {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for drop down.
///
/// Visibility: **public**.
pub struct DropDown {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores items.
    ///
    /// Visibility: **private**.
    items: Arc<Mutex<Vec<String>>>,
    /// Stores selected.
    ///
    /// Visibility: **private**.
    selected: Arc<Mutex<Option<usize>>>,
}
impl DropDown {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new<I, S>(items: I) -> Self where I: IntoIterator<Item = S>, S: Into<String> {
        let items = items.into_iter().map(Into::into).collect::<Vec<_>>();
        let selected = if items.is_empty() { None } else { Some(0) };
        Self {
            widget: Widget::new(),
            items: Arc::new(Mutex::new(items)),
            selected: Arc::new(Mutex::new(selected)),
        }
    }
    #[doc = "Implements the selected operation. Visibility: public."]
    pub fn selected(&self) -> Option<usize> {
        *self.selected.lock().unwrap()
    }
    #[doc = "Sets selected. Visibility: public."]
    pub fn set_selected(&self, i: Option<usize>) {
        let len = self.items.lock().unwrap().len();
        *self.selected.lock().unwrap() = i.filter(|v| *v < len);
    }
    #[doc = "Implements the selected text operation. Visibility: public."]
    pub fn selected_text(&self) -> Option<String> {
        let i = self.selected()?;
        self.items.lock().unwrap().get(i).cloned()
    }
    #[doc = "Implements the items operation. Visibility: public."]
    pub fn items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }
}
impl WidgetExt for DropDown {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone)]
/// Stores the state and data for combo box.
///
/// Visibility: **public**.
pub struct ComboBox {
    /// Stores dropdown.
    ///
    /// Visibility: **private**.
    dropdown: DropDown,
}
impl ComboBox {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new<I, S>(items: I) -> Self where I: IntoIterator<Item = S>, S: Into<String> {
        Self { dropdown: DropDown::new(items) }
    }
    #[doc = "Implements the selected operation. Visibility: public."]
    pub fn selected(&self) -> Option<usize> {
        self.dropdown.selected()
    }
    #[doc = "Sets selected. Visibility: public."]
    pub fn set_selected(&self, i: Option<usize>) {
        self.dropdown.set_selected(i)
    }
}
impl WidgetExt for ComboBox {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.dropdown.widget()
    }
}
/// Type alias for combo box text.
///
/// Visibility: **public**.
pub type ComboBoxText = ComboBox;

#[derive(Debug, Clone)]
/// Stores the state and data for scale button.
///
/// Visibility: **public**.
pub struct ScaleButton {
    /// Stores scale.
    ///
    /// Visibility: **private**.
    scale: Scale,
}
impl ScaleButton {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self { scale: Scale::new(Orientation::Horizontal, min, max, value) }
    }
    #[doc = "Implements the value operation. Visibility: public."]
    pub fn value(&self) -> f64 {
        self.scale.value()
    }
    #[doc = "Sets value. Visibility: public."]
    pub fn set_value(&self, v: f64) {
        self.scale.set_value(v)
    }
}
impl WidgetExt for ScaleButton {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.scale.widget()
    }
}
/// Type alias for volume button.
///
/// Visibility: **public**.
pub type VolumeButton = ScaleButton;

#[derive(Debug, Clone)]
/// Stores the state and data for calendar.
///
/// Visibility: **public**.
pub struct Calendar {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores year.
    ///
    /// Visibility: **private**.
    year: Arc<Mutex<i32>>,
    /// Stores month.
    ///
    /// Visibility: **private**.
    month: Arc<Mutex<u32>>,
    /// Stores day.
    ///
    /// Visibility: **private**.
    day: Arc<Mutex<u32>>,
}
impl Calendar {
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self {
            widget: Widget::new(),
            year: Arc::new(Mutex::new(year)),
            month: Arc::new(Mutex::new(month.clamp(1, 12))),
            day: Arc::new(Mutex::new(day.clamp(1, 31))),
        }
    }
    #[doc = "Implements the date operation. Visibility: public."]
    pub fn date(&self) -> (i32, u32, u32) {
        (*self.year.lock().unwrap(), *self.month.lock().unwrap(), *self.day.lock().unwrap())
    }
    #[doc = "Sets date. Visibility: public."]
    pub fn set_date(&self, y: i32, m: u32, d: u32) {
        *self.year.lock().unwrap() = y;
        *self.month.lock().unwrap() = m.clamp(1, 12);
        *self.day.lock().unwrap() = d.clamp(1, 31);
    }
}
impl WidgetExt for Calendar {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}
