//! Implements the `drop` module.
//!
//! This module documents both public API items and internal implementation details.

use x11rb::{ connection::Connection, protocol::xproto::ConnectionExt as _ };

impl Drop for super::PlatformWindow {
    /// Performs the drop operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the drop operation. Visibility: private/internal."]
    fn drop(&mut self) {
        self.renderer.shutdown(&self.connection);
        let _ = self.connection.destroy_window(self.window);
        let _ = self.connection.flush();
    }
}
