//! Geometry and resize policies shared by native widgets.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Stores the state and data for rect.
///
/// Visibility: **public**.
pub struct Rect {
    /// Stores x.
    ///
    /// Visibility: **public**.
    pub x: i32,
    /// Stores y.
    ///
    /// Visibility: **public**.
    pub y: i32,
    /// Stores width.
    ///
    /// Visibility: **public**.
    pub width: u32,
    /// Stores height.
    ///
    /// Visibility: **public**.
    pub height: u32,
}
impl Rect {
    #[inline]
    #[doc = "Creates a new value. Visibility: public."]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
    #[inline]
    #[doc = "Implements the contains operation. Visibility: public."]
    pub fn contains(self, x: i32, y: i32) -> bool {
        self.width != 0 &&
            self.height != 0 &&
            x >= self.x &&
            y >= self.y &&
            x < self.x.saturating_add(self.width as i32) &&
            y < self.y.saturating_add(self.height as i32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Stores the state and data for anchors.
///
/// Visibility: **public**.
pub struct Anchors {
    /// Stores left.
    ///
    /// Visibility: **public**.
    pub left: bool,
    /// Stores top.
    ///
    /// Visibility: **public**.
    pub top: bool,
    /// Stores right.
    ///
    /// Visibility: **public**.
    pub right: bool,
    /// Stores bottom.
    ///
    /// Visibility: **public**.
    pub bottom: bool,
}
impl Anchors {
    /// Constant used for none.
    ///
    /// Visibility: **public**.
    pub const NONE: Self = Self { left: false, top: false, right: false, bottom: false };
    /// Constant used for top left.
    ///
    /// Visibility: **public**.
    pub const TOP_LEFT: Self = Self { left: true, top: true, right: false, bottom: false };
    /// Constant used for top right.
    ///
    /// Visibility: **public**.
    pub const TOP_RIGHT: Self = Self { left: false, top: true, right: true, bottom: false };
    /// Constant used for bottom left.
    ///
    /// Visibility: **public**.
    pub const BOTTOM_LEFT: Self = Self { left: true, top: false, right: false, bottom: true };
    /// Constant used for bottom right.
    ///
    /// Visibility: **public**.
    pub const BOTTOM_RIGHT: Self = Self { left: false, top: false, right: true, bottom: true };
    /// Constant used for horizontal.
    ///
    /// Visibility: **public**.
    pub const HORIZONTAL: Self = Self { left: true, top: false, right: true, bottom: false };
    /// Constant used for vertical.
    ///
    /// Visibility: **public**.
    pub const VERTICAL: Self = Self { left: false, top: true, right: false, bottom: true };
    /// Constant used for top horizontal.
    ///
    /// Visibility: **public**.
    pub const TOP_HORIZONTAL: Self = Self { left: true, top: true, right: true, bottom: false };
    /// Constant used for bottom horizontal.
    ///
    /// Visibility: **public**.
    pub const BOTTOM_HORIZONTAL: Self = Self { left: true, top: false, right: true, bottom: true };
    /// Constant used for left vertical.
    ///
    /// Visibility: **public**.
    pub const LEFT_VERTICAL: Self = Self { left: true, top: true, right: false, bottom: true };
    /// Constant used for right vertical.
    ///
    /// Visibility: **public**.
    pub const RIGHT_VERTICAL: Self = Self { left: false, top: true, right: true, bottom: true };
    /// Constant used for all.
    ///
    /// Visibility: **public**.
    pub const ALL: Self = Self { left: true, top: true, right: true, bottom: true };
}
impl Default for Anchors {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::TOP_LEFT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Defines the available resize policy choices.
///
/// Visibility: **public**.
pub enum ResizePolicy {
    #[default]
    /// Represents the fixed variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Fixed,
    /// Represents the anchored variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Anchored(Anchors),
    /// Scale position and size proportionally to the client area.
    Proportional,
}

#[derive(Debug, Clone, Copy)]
/// Stores the state and data for resize state.
///
/// Visibility: **crate-visible**.
pub(crate) struct ResizeState {
    /// Stores base bounds.
    ///
    /// Visibility: **private**.
    base_bounds: Rect,
    /// Stores base window.
    ///
    /// Visibility: **private**.
    base_window: (u32, u32),
    /// Stores policy.
    ///
    /// Visibility: **private**.
    policy: ResizePolicy,
}
impl ResizeState {
    /// Creates a new instance.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Creates a new value. Visibility: crate-visible."]
    pub(crate) fn new(bounds: Rect, window: (u32, u32)) -> Self {
        Self {
            base_bounds: bounds,
            base_window: (window.0.max(1), window.1.max(1)),
            policy: ResizePolicy::Fixed,
        }
    }
    /// Performs the reset operation.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the reset operation. Visibility: crate-visible."]
    pub(crate) fn reset(&mut self, bounds: Rect, window: (u32, u32)) {
        self.base_bounds = bounds;
        self.base_window = (window.0.max(1), window.1.max(1));
    }
    /// Sets policy.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Sets policy. Visibility: crate-visible."]
    pub(crate) fn set_policy(
        &mut self,
        policy: ResizePolicy,
        current_bounds: Rect,
        current_window: (u32, u32)
    ) {
        self.reset(current_bounds, current_window);
        self.policy = policy;
    }
    /// Resolves resolve from the current state.
    ///
    /// Visibility: **crate-visible**.
    #[doc = "Implements the resolve operation. Visibility: crate-visible."]
    pub(crate) fn resolve(&self, window: (u32, u32)) -> Rect {
        let window = (window.0.max(1), window.1.max(1));
        match self.policy {
            ResizePolicy::Fixed => self.base_bounds,
            ResizePolicy::Proportional => {
                let sx = (window.0 as f64) / (self.base_window.0 as f64);
                let sy = (window.1 as f64) / (self.base_window.1 as f64);
                Rect::new(
                    ((self.base_bounds.x as f64) * sx)
                        .round()
                        .clamp(i32::MIN as f64, i32::MAX as f64) as i32,
                    ((self.base_bounds.y as f64) * sy)
                        .round()
                        .clamp(i32::MIN as f64, i32::MAX as f64) as i32,
                    ((self.base_bounds.width as f64) * sx)
                        .round()
                        .max(1.0)
                        .min(u32::MAX as f64) as u32,
                    ((self.base_bounds.height as f64) * sy)
                        .round()
                        .max(1.0)
                        .min(u32::MAX as f64) as u32
                )
            }
            ResizePolicy::Anchored(a) => {
                let dx = (window.0 as i64) - (self.base_window.0 as i64);
                let dy = (window.1 as i64) - (self.base_window.1 as i64);
                let mut x = self.base_bounds.x as i64;
                let mut y = self.base_bounds.y as i64;
                let mut w = self.base_bounds.width as i64;
                let mut h = self.base_bounds.height as i64;
                if a.left && a.right {
                    w += dx;
                } else if !a.left && a.right {
                    x += dx;
                }
                if a.top && a.bottom {
                    h += dy;
                } else if !a.top && a.bottom {
                    y += dy;
                }
                Rect::new(
                    x.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    w.max(1).min(u32::MAX as i64) as u32,
                    h.max(1).min(u32::MAX as i64) as u32
                )
            }
        }
    }
}
