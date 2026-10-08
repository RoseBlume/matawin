//! Implements the `show` module.
//!
//! This module documents both public API items and internal implementation details.

use x11rb::{
    connection::Connection,
    properties::WmSizeHints,
    protocol::xproto::{
        ClientMessageData,
        ClientMessageEvent,
        ConfigureWindowAux,
        ConnectionExt as _,
        EventMask,
        PropMode,
    },
    wrapper::ConnectionExt as _,
};

impl super::PlatformWindow {
    /// Performs the hide operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the hide operation. Visibility: public."]
    pub fn hide(&self) {
        self.hidden.set(true);
        let _ = self.connection.unmap_window(self.window);
        let _ = self.connection.flush();
    }

    /// Retained for API parity. X11 window icons are pixel properties rather than
    /// file paths; loading/decoding an arbitrary image is intentionally left to
    /// the higher-level crate instead of adding another image dependency here.
    #[doc = "Sets icon. Visibility: public."]
    pub fn set_icon(&self, _path: &str) {}

    /// Performs the maximize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the maximize operation. Visibility: public."]
    pub fn maximize(&self) {
        self.send_net_wm_state(
            1,
            self.atoms.net_wm_state_maximized_horz,
            self.atoms.net_wm_state_maximized_vert
        );
        self.maximized.set(true);
    }

    /// Performs the minimize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the minimize operation. Visibility: public."]
    pub fn minimize(&self) {
        // ICCCM WM_CHANGE_STATE, IconicState = 3.
        let event = ClientMessageEvent::new(
            32,
            self.window,
            self.atoms.wm_change_state,
            ClientMessageData::from([3u32, 0, 0, 0, 0])
        );
        let _ = self.connection.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event
        );
        let _ = self.connection.flush();
    }

    /// Performs the restore operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the restore operation. Visibility: public."]
    pub fn restore(&self) {
        self.hidden.set(false);
        let _ = self.connection.map_window(self.window);
        self.send_net_wm_state(
            0,
            self.atoms.net_wm_state_maximized_horz,
            self.atoms.net_wm_state_maximized_vert
        );
        self.maximized.set(false);
        let _ = self.connection.flush();
    }

    /// Performs the reposition operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the reposition operation. Visibility: public."]
    pub fn reposition(&self, x: i32, y: i32) {
        let _ = self.connection.configure_window(self.window, &ConfigureWindowAux::new().x(x).y(y));
        let _ = self.connection.flush();
    }

    /// Performs the resize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resize operation. Visibility: public."]
    pub fn resize(&self, width: u32, height: u32) {
        if width == 0 || height == 0 || width > (u16::MAX as u32) || height > (u16::MAX as u32) {
            return;
        }
        let _ = self.connection.configure_window(
            self.window,
            &ConfigureWindowAux::new().width(width).height(height)
        );
        self.client_size.set((width, height));
        if !self.resizable.get() {
            self.apply_size_hints();
        }
        let _ = self.connection.flush();
    }

    /// Performs the show operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the show operation. Visibility: public."]
    pub fn show(&self) {
        self.hidden.set(false);
        let _ = self.connection.map_window(self.window);
        let _ = self.connection.flush();
    }

    /// Sets decorations.
    ///
    /// Visibility: **public**.
    #[doc = "Sets decorations. Visibility: public."]
    pub fn set_decorations(&self, enabled: bool) {
        self.decorations.set(enabled);
        self.apply_motif_hints();
    }

    /// Sets maximizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets maximizable. Visibility: public."]
    pub fn set_maximizable(&self, enabled: bool) {
        self.maximizable.set(enabled);
        self.apply_motif_hints();
    }

    /// Sets minimizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets minimizable. Visibility: public."]
    pub fn set_minimizable(&self, enabled: bool) {
        self.minimizable.set(enabled);
        self.apply_motif_hints();
    }

    /// Sets resizable.
    ///
    /// Visibility: **public**.
    #[doc = "Sets resizable. Visibility: public."]
    pub fn set_resizable(&self, enabled: bool) {
        self.resizable.set(enabled);
        self.apply_motif_hints();
        self.apply_size_hints();
    }

    /// Performs the send net wm state operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the send net wm state operation. Visibility: private/internal."]
    fn send_net_wm_state(&self, action: u32, first: u32, second: u32) {
        let event = ClientMessageEvent::new(
            32,
            self.window,
            self.atoms.net_wm_state,
            ClientMessageData::from([action, first, second, 1, 0])
        );
        let _ = self.connection.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event
        );
        let _ = self.connection.flush();
    }

    /// Performs the apply motif hints operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the apply motif hints operation. Visibility: private/internal."]
    fn apply_motif_hints(&self) {
        /// Constant used for mwm hints functions.
        ///
        /// Visibility: **private**.
        const MWM_HINTS_FUNCTIONS: u32 = 1 << 0;
        /// Constant used for mwm hints decorations.
        ///
        /// Visibility: **private**.
        const MWM_HINTS_DECORATIONS: u32 = 1 << 1;
        /// Constant used for mwm func resize.
        ///
        /// Visibility: **private**.
        const MWM_FUNC_RESIZE: u32 = 1 << 1;
        /// Constant used for mwm func move.
        ///
        /// Visibility: **private**.
        const MWM_FUNC_MOVE: u32 = 1 << 2;
        /// Constant used for mwm func minimize.
        ///
        /// Visibility: **private**.
        const MWM_FUNC_MINIMIZE: u32 = 1 << 3;
        /// Constant used for mwm func maximize.
        ///
        /// Visibility: **private**.
        const MWM_FUNC_MAXIMIZE: u32 = 1 << 4;
        /// Constant used for mwm func close.
        ///
        /// Visibility: **private**.
        const MWM_FUNC_CLOSE: u32 = 1 << 5;
        /// Constant used for mwm decor all.
        ///
        /// Visibility: **private**.
        const MWM_DECOR_ALL: u32 = 1 << 0;

        let mut functions = MWM_FUNC_MOVE | MWM_FUNC_CLOSE;
        if self.resizable.get() {
            functions |= MWM_FUNC_RESIZE;
        }
        if self.minimizable.get() {
            functions |= MWM_FUNC_MINIMIZE;
        }
        if self.maximizable.get() {
            functions |= MWM_FUNC_MAXIMIZE;
        }

        let decorations = if self.decorations.get() { MWM_DECOR_ALL } else { 0 };
        let data = [MWM_HINTS_FUNCTIONS | MWM_HINTS_DECORATIONS, functions, decorations, 0, 0];
        let _ = self.connection.change_property32(
            PropMode::REPLACE,
            self.window,
            self.atoms.motif_wm_hints,
            self.atoms.motif_wm_hints,
            &data
        );
        let _ = self.connection.flush();
    }

    /// Performs the apply size hints operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the apply size hints operation. Visibility: private/internal."]
    fn apply_size_hints(&self) {
        let mut hints = WmSizeHints::new();
        if !self.resizable.get() {
            let (w, h) = self.client_size.get();
            hints.min_size = Some((w as i32, h as i32));
            hints.max_size = Some((w as i32, h as i32));
        }
        let _ = hints.set_normal_hints(&self.connection, self.window);
        let _ = self.connection.flush();
    }
}
