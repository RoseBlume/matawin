use crate::image::load_image_2d;
use super::Background;


impl super::AppWindow {
    /// Sets the background color of the window.
    ///
    /// # Arguments
    ///
    /// * `color` - A 32-bit unsigned integer representing the background color in RGBA format.
    ///
    /// # Example
    ///
    /// ```
    /// window.set_background_color(0xFF0000FF); // Sets background to red
    /// ```
    pub fn set_background_color(&mut self, color: u32) {
            self.background = Some(Background::Color(color));
        }

    /// Sets the background image of the window from a file path.
    ///
    /// # Arguments
    ///
    /// * `path` - A string slice containing the file path to the image.
    ///
    /// # Panics
    ///
    /// Panics if the image fails to load from the specified path.
    ///
    /// # Example
    ///
    /// ```
    /// window.set_background_image("assets/background.png");
    /// ```
    pub fn set_background_image(&mut self, path: &str) {
        let image: Vec<Vec<u32>> = load_image_2d(path).expect("Failed to load image");
        self.background = Some(Background::Image(image));
    }
}