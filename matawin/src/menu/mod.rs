use crate::{Window, TextAlign};

/// A simple vertical menu for selectable text-based options.
///
/// The menu maintains a list of static string options, tracks the
/// currently selected index, and renders itself centered in a window.
/// Navigation is clamped to valid bounds and does not wrap.
///
/// Rendering is stateless per frame; previously drawn menu text is
/// removed before re-drawing.
pub struct Menu {
    /// Menu option labels, displayed in order.
    pub options: Vec<&'static str>,

    /// Index of the currently selected option.
    selected: usize,

    /// Color used for the selected option.
    selected_col: u32,

    /// Color used for unselected options.
    unselected: u32,
}

impl Menu {
    /// Creates a new menu with the given options and colors.
    ///
    /// The first option is selected by default.
    ///
    /// # Parameters
    /// - `options`: A list of static string labels to display.
    /// - `selected_col`: Color used to render the selected option.
    /// - `unselected`: Color used to render unselected options.
    pub fn new(options: Vec<&'static str>, selected_col: u32, unselected: u32) -> Self {
        Self {
            options,
            selected: 0,
            selected_col,
            unselected,
        }
    }

    /// Moves the selection up by one entry.
    ///
    /// If the selection is already at the top, this method does nothing.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Moves the selection down by one entry.
    ///
    /// If the selection is already at the bottom, this method does nothing.
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.options.len() {
            self.selected += 1;
        }
    }

    /// Returns the currently selected option.
    pub fn current(&self) -> &str {
        self.options[self.selected]
    }

    /// Draws the menu to the given window.
    ///
    /// All menu options are rendered vertically centered with a fixed
    /// vertical gap. The selected option is rendered using
    /// `selected_col`, while all others use `unselected`.
    ///
    /// Any previously rendered text using the same `id_prefix`
    /// is removed before drawing.
    ///
    /// # Parameters
    /// - `window`: The window to render into.
    /// - `id_prefix`: A unique identifier prefix used for text elements.
    ///
    /// # Notes
    /// - Text width is estimated for centering using a fixed font scale.
    /// - Text is horizontally centered in the window.
    pub fn draw(&self, window: &mut Window, id_prefix: &str) {
        // Remove previous text for this menu
        for i in 0..self.options.len() {
            let id = format!("{}_{}", id_prefix, i);
            window.remove_text(&id);
        }

        let (width, height) = window.get_size();

        // Calculate starting Y so the menu is vertically centered
        let gap = 50;
        let total_height = gap * (self.options.len() - 1);
        let start_y = (height / 2).saturating_sub(total_height / 2);

        for (i, option) in self.options.iter().enumerate() {
            let color = if i == self.selected { self.selected_col } else { self.unselected };
            let text_width = option.len() * 5 * 5; // rough width approximation for AutoFit 5x5 font scaled by size
            let x = width / 2 - text_width / 2;    // center text horizontally
            let y = start_y + i * gap;

            window.show_text(
                &format!("{}_{}", id_prefix, i),
                option,
                (x, y),
                5,
                color,
                TextAlign::AutoFit,
            );
        }
    }
}
