use std::collections::HashMap;

pub enum TextAlign {
    Left,
    Center,
    Right,
    AutoFit, // scale text to fit in window width
}

// A drawable text element rendered using the built-in bitmap font.
///
/// Text is stored and managed by the window and rendered each frame
/// during the draw pass. Scaling is achieved by pixel replication,
/// and alignment affects how the position is interpreted.
pub struct TextItem {
    /// The text content to render.
    pub content: String,

    /// The reference position of the text in window coordinates.
    ///
    /// The meaning of this position depends on the selected alignment.
    pub position: (usize, usize),

    /// Scaling factor applied to the 5x5 bitmap font.
    ///
    /// A value of `1` renders the text at its base resolution.
    pub size: usize,

    /// Text color in 0xRRGGBB format.
    pub color: u32,

    /// Horizontal alignment mode used when rendering the text.
    pub align: TextAlign, // new
}

impl super::AppWindow {
    /// Displays a text element in the window.
    ///
    /// If a text item with the same `id` already exists, it will be
    /// replaced. The text will persist across frames until explicitly
    /// removed.
    ///
    /// # Parameters
    /// - `id`: Unique identifier for this text item.
    /// - `content`: Text string to display.
    /// - `position`: Reference position for rendering.
    /// - `size`: Scaling factor for the bitmap font.
    /// - `color`: Text color in 0xRRGGBB format.
    /// - `align`: Alignment mode used to interpret the position.
    pub fn show_text(
        &mut self,
        id: &str,
        content: &str,
        position: (usize, usize),
        size: usize,
        color: u32,
        align: TextAlign, // new
    ) {
        self.texts.insert(id.to_string(), TextItem {
            content: content.to_string(),
            position,
            size,
            color,
            align,
        });
    }

    /// Updates the content of an existing text item.
    ///
    /// If no text item with the given `id` exists, this method does nothing.
    ///
    /// # Parameters
    /// - `id`: Identifier of the text item to update.
    /// - `content`: New text content.
    pub fn update_text(&mut self, id: &str, content: &str) {
        if let Some(text_item) = self.texts.get_mut(id) {
            text_item.content = content.to_string();
        }
    }

    /// Removes a text item from the window.
    ///
    /// After removal, the text will no longer be rendered.
    /// If the `id` does not exist, this method does nothing.
    pub fn remove_text(&mut self, id: &str) {
        self.texts.remove(id);
    }
}


pub fn get_font_map() -> HashMap<char, [[u8; 5]; 5]> {
    let mut map = HashMap::new();

    // Letters A-Z
    map.insert('A', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [1,1,1,1,1],
        [1,0,0,0,1],
        [1,0,0,0,1],
    ]);

    map.insert('B', [
        [1,1,1,1,0],
        [1,0,0,0,1],
        [1,1,1,1,0],
        [1,0,0,0,1],
        [1,1,1,1,0],
    ]);

    map.insert('C', [
        [0,1,1,1,1],
        [1,0,0,0,0],
        [1,0,0,0,0],
        [1,0,0,0,0],
        [0,1,1,1,1],
    ]);

    map.insert('D', [
        [1,1,1,0,0],
        [1,0,0,1,0],
        [1,0,0,0,1],
        [1,0,0,1,0],
        [1,1,1,0,0],
    ]);

    map.insert('E', [
        [1,1,1,1,1],
        [1,0,0,0,0],
        [1,1,1,1,0],
        [1,0,0,0,0],
        [1,1,1,1,1],
    ]);

    map.insert('F', [
        [1,1,1,1,1],
        [1,0,0,0,0],
        [1,1,1,1,0],
        [1,0,0,0,0],
        [1,0,0,0,0],
    ]);

    map.insert('G', [
        [0,1,1,1,1],
        [1,0,0,0,0],
        [1,0,1,1,1],
        [1,0,0,0,1],
        [0,1,1,1,1],
    ]);

    map.insert('H', [
        [1,0,0,0,1],
        [1,0,0,0,1],
        [1,1,1,1,1],
        [1,0,0,0,1],
        [1,0,0,0,1],
    ]);

    map.insert('I', [
        [1,1,1,1,1],
        [0,0,1,0,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
        [1,1,1,1,1],
    ]);

    map.insert('J', [
        [0,0,0,1,1],
        [0,0,0,0,1],
        [0,0,0,0,1],
        [1,0,0,0,1],
        [0,1,1,1,0],
    ]);

    map.insert('K', [
        [1,0,0,0,1],
        [1,0,0,1,0],
        [1,1,1,0,0],
        [1,0,0,1,0],
        [1,0,0,0,1],
    ]);

    map.insert('L', [
        [1,0,0,0,0],
        [1,0,0,0,0],
        [1,0,0,0,0],
        [1,0,0,0,0],
        [1,1,1,1,1],
    ]);

    map.insert('M', [
        [1,0,0,0,1],
        [1,1,0,1,1],
        [1,0,1,0,1],
        [1,0,0,0,1],
        [1,0,0,0,1],
    ]);

    map.insert('N', [
        [1,0,0,0,1],
        [1,1,0,0,1],
        [1,0,1,0,1],
        [1,0,0,1,1],
        [1,0,0,0,1],
    ]);

    map.insert('O', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [1,0,0,0,1],
        [1,0,0,0,1],
        [0,1,1,1,0],
    ]);

    map.insert('P', [
        [1,1,1,1,0],
        [1,0,0,0,1],
        [1,1,1,1,0],
        [1,0,0,0,0],
        [1,0,0,0,0],
    ]);

    map.insert('Q', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [1,0,0,0,1],
        [1,0,0,1,0],
        [0,1,1,0,1],
    ]);

    map.insert('R', [
        [1,1,1,1,0],
        [1,0,0,0,1],
        [1,1,1,1,0],
        [1,0,0,1,0],
        [1,0,0,0,1],
    ]);

    map.insert('S', [
        [0,1,1,1,1],
        [1,0,0,0,0],
        [0,1,1,1,0],
        [0,0,0,0,1],
        [1,1,1,1,0],
    ]);

    map.insert('T', [
        [1,1,1,1,1],
        [0,0,1,0,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
    ]);

    map.insert('U', [
        [1,0,0,0,1],
        [1,0,0,0,1],
        [1,0,0,0,1],
        [1,0,0,0,1],
        [0,1,1,1,0],
    ]);

    map.insert('V', [
        [1,0,0,0,1],
        [1,0,0,0,1],
        [0,1,0,1,0],
        [0,1,0,1,0],
        [0,0,1,0,0],
    ]);

    map.insert('W', [
        [1,0,0,0,1],
        [1,0,0,0,1],
        [1,0,1,0,1],
        [1,1,0,1,1],
        [1,0,0,0,1],
    ]);

    map.insert('X', [
        [1,0,0,0,1],
        [0,1,0,1,0],
        [0,0,1,0,0],
        [0,1,0,1,0],
        [1,0,0,0,1],
    ]);

    map.insert('Y', [
        [1,0,0,0,1],
        [0,1,0,1,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
    ]);

    map.insert('Z', [
        [1,1,1,1,1],
        [0,0,0,1,0],
        [0,0,1,0,0],
        [0,1,0,0,0],
        [1,1,1,1,1],
    ]);

    // Numbers 0-9
    map.insert('0', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [1,0,0,1,1],
        [1,0,1,0,1],
        [0,1,1,1,0],
    ]);
    map.insert('1', [
        [0,0,1,0,0],
        [0,1,1,0,0],
        [1,0,1,0,0],
        [0,0,1,0,0],
        [1,1,1,1,0],
    ]);
    map.insert('2', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [0,0,0,1,0],
        [0,0,1,0,0],
        [1,1,1,1,1],
    ]);
    map.insert('3', [
        [1,1,1,1,0],
        [0,0,0,0,1],
        [0,0,1,1,0],
        [0,0,0,0,1],
        [1,1,1,1,0],
    ]);
    map.insert('4', [
        [0,0,1,1,0],
        [0,1,0,1,0],
        [1,0,0,1,0],
        [1,1,1,1,1],
        [0,0,0,1,0],
    ]);
    map.insert('5', [
        [1,1,1,1,1],
        [1,0,0,0,0],
        [1,1,1,1,0],
        [0,0,0,0,1],
        [1,1,1,1,0],
    ]);
    map.insert('6', [
        [0,1,1,1,0],
        [1,0,0,0,0],
        [1,1,1,1,0],
        [1,0,0,0,1],
        [0,1,1,1,0],
    ]);
    map.insert('7', [
        [1,1,1,1,1],
        [0,0,0,0,1],
        [0,0,0,1,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
    ]);
    map.insert('8', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [0,1,1,1,0],
        [1,0,0,0,1],
        [0,1,1,1,0],
    ]);
    map.insert('9', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [0,1,1,1,1],
        [0,0,0,0,1],
        [0,1,1,1,0],
    ]);

    // Space and punctuation
    map.insert(' ', [
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,0,0,0],
    ]);

    map.insert('.', [
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
    ]);

    map.insert(',', [
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,0,0,0],
        [0,0,1,0,0],
        [0,1,0,0,0],
    ]);

    map.insert('!', [
        [0,0,1,0,0],
        [0,0,1,0,0],
        [0,0,1,0,0],
        [0,0,0,0,0],
        [0,0,1,0,0],
    ]);

    map.insert('?', [
        [0,1,1,1,0],
        [1,0,0,0,1],
        [0,0,0,1,0],
        [0,0,0,0,0],
        [0,0,1,0,0],
    ]);

    map
}