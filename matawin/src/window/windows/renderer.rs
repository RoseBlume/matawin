//! Implements the `renderer` module.
//!
//! This module documents both public API items and internal implementation details.

use std::ffi::c_void;

use crate::buffer::Framebuffer;

use windows::{
    core::Result,
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{
                Common::{ D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U },
                D2D1CreateFactory,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                D2D1_BITMAP_PROPERTIES,
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_NONE,
                D2D1_RENDER_TARGET_PROPERTIES,
                ID2D1Bitmap,
                ID2D1Factory,
                ID2D1HwndRenderTarget,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        },
    },
};

/// Direct2D presentation backend for the CPU framebuffer.
///
/// The bitmap is reused while its size is unchanged and recreated only when
/// the application submits a differently-sized framebuffer. WidgetWindow keeps
/// that bitmap exactly equal to the native client size after every resize, so
/// this backend performs a 1:1 pixel presentation instead of DPI upscaling. A normal frame
/// therefore performs only:
///
/// 1. one CPU -> Direct2D bitmap upload (`CopyFromMemory`),
/// 2. one `DrawBitmap`,
/// 3. one `EndDraw`/present.
///
/// The pixel format intentionally matches the old 32-bit `BI_RGB` GDI path:
/// bytes in memory are interpreted as B, G, R, X/A. This keeps existing
/// framebuffer colour packing compatible with the previous backend.
pub(super) struct Direct2DRenderer {
    /// Stores factory.
    ///
    /// Visibility: **private**.
    factory: ID2D1Factory,
    /// Stores render target.
    ///
    /// Visibility: **private**.
    render_target: Option<ID2D1HwndRenderTarget>,
    /// Stores bitmap.
    ///
    /// Visibility: **private**.
    bitmap: Option<ID2D1Bitmap>,
    /// Stores framebuffer size.
    ///
    /// Visibility: **private**.
    framebuffer_size: D2D_SIZE_U,
    /// Stores target size.
    ///
    /// Visibility: **private**.
    target_size: D2D_SIZE_U,
    /// Stores destination.
    ///
    /// Visibility: **private**.
    destination: D2D_RECT_F,
}

impl Direct2DRenderer {
    /// Creates a new instance.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Creates a new value. Visibility: parent-module-visible."]
    pub(super) fn new(hwnd: HWND, framebuffer_size: (u32, u32), target_size: (u32, u32)) -> Self {
        let factory = unsafe {
            D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).expect(
                "D2D1CreateFactory failed"
            )
        };

        let mut renderer = Self {
            factory,
            render_target: None,
            bitmap: None,
            framebuffer_size: D2D_SIZE_U {
                width: framebuffer_size.0,
                height: framebuffer_size.1,
            },
            target_size: D2D_SIZE_U {
                width: target_size.0,
                height: target_size.1,
            },
            destination: D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: target_size.0 as f32,
                bottom: target_size.1 as f32,
            },
        };

        // A minimized window has a 0x0 client area. In that case resources are
        // created lazily when the window becomes visible again.
        if target_size.0 != 0 && target_size.1 != 0 {
            renderer
                .create_device_resources(hwnd)
                .expect("Failed to create Direct2D device resources");
        }

        renderer
    }

    /// Performs the create device resources operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the create device resources operation. Visibility: private/internal."]
    fn create_device_resources(&mut self, hwnd: HWND) -> Result<()> {
        if
            self.target_size.width == 0 ||
            self.target_size.height == 0 ||
            self.framebuffer_size.width == 0 ||
            self.framebuffer_size.height == 0
        {
            return Ok(());
        }

        if self.render_target.is_none() {
            // WidgetWindow rasterizes directly in native client pixels and the
            // Win32 pointer messages are also expressed in client coordinates.
            // Force the Direct2D target to 96 DPI so one D2D unit is exactly one
            // client/framebuffer pixel. Leaving dpiX/dpiY at 0 asks Direct2D to
            // inherit the monitor DPI; under per-monitor DPI awareness that
            // scales the already-native-size framebuffer a second time and makes
            // the visible controls disagree with hit-testing coordinates.
            let render_target_properties = D2D1_RENDER_TARGET_PROPERTIES {
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let hwnd_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: self.target_size,
                // Let Direct2D/DWM pace presentation instead of forcing immediate full-surface uploads.
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };

            let render_target = unsafe {
                self.factory.CreateHwndRenderTarget(&render_target_properties, &hwnd_properties)?
            };
            self.render_target = Some(render_target);
        }

        if self.bitmap.is_none() {
            let bitmap_properties = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };

            let render_target = self.render_target
                .as_ref()
                .expect("Direct2D render target missing after creation");
            let bitmap = unsafe {
                render_target.CreateBitmap(
                    self.framebuffer_size,
                    None,
                    self.framebuffer_size.width.saturating_mul(4),
                    &bitmap_properties
                )?
            };
            self.bitmap = Some(bitmap);
        }

        Ok(())
    }

    /// Performs the discard device resources operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the discard device resources operation. Visibility: private/internal."]
    fn discard_device_resources(&mut self) {
        // Bitmap is tied to the render target, so release it first.
        self.bitmap = None;
        self.render_target = None;
    }

    /// Performs the shutdown operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the shutdown operation. Visibility: parent-module-visible."]
    pub(super) fn shutdown(&mut self) {
        self.discard_device_resources();
    }

    /// Changes the CPU source bitmap size without scaling old pixels into the
    /// new window size. The next present creates a matching Direct2D bitmap.
    #[doc = "Resizes framebuffer. Visibility: parent-module-visible."]
    pub(super) fn resize_framebuffer(&mut self, size: (u32, u32)) {
        let new_size = D2D_SIZE_U {
            width: size.0,
            height: size.1,
        };
        if self.framebuffer_size == new_size {
            return;
        }

        self.framebuffer_size = new_size;
        // The Direct2D bitmap has immutable dimensions, but the HWND render
        // target does not need to be recreated just because the source changes.
        self.bitmap = None;
    }

    /// Performs the resize target operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Resizes target. Visibility: parent-module-visible."]
    pub(super) fn resize_target(&mut self, hwnd: HWND, size: (u32, u32)) {
        let new_size = D2D_SIZE_U {
            width: size.0,
            height: size.1,
        };

        if self.target_size == new_size {
            return;
        }

        self.target_size = new_size;
        self.destination.right = size.0 as f32;
        self.destination.bottom = size.1 as f32;

        if size.0 == 0 || size.1 == 0 {
            return;
        }

        if let Some(target) = &self.render_target {
            if (unsafe { target.Resize(&new_size) }).is_err() {
                self.discard_device_resources();
            }
        }

        if self.render_target.is_none() {
            let _ = self.create_device_resources(hwnd);
        }
    }

    /// Performs the present once operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the present once operation. Visibility: private/internal."]
    fn present_once(&mut self, hwnd: HWND, pixels: *const c_void) -> Result<()> {
        self.create_device_resources(hwnd)?;

        let Some(bitmap) = self.bitmap.as_ref() else {
            return Ok(());
        };
        let Some(target) = self.render_target.as_ref() else {
            return Ok(());
        };

        unsafe {
            bitmap.CopyFromMemory(None, pixels, self.framebuffer_size.width.saturating_mul(4))?;

            target.BeginDraw();

            target.DrawBitmap(
                bitmap,
                Some(&self.destination as *const D2D_RECT_F),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None
            );

            target.EndDraw(None, None)?;
        }

        Ok(())
    }

    /// Performs the present operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the present operation. Visibility: parent-module-visible."]
    pub(super) fn present(&mut self, hwnd: HWND, framebuffer: &Framebuffer) {
        if self.target_size.width == 0 || self.target_size.height == 0 {
            return;
        }

        self.resize_framebuffer((framebuffer.width, framebuffer.height));
        // WidgetWindow normally makes these equal. Keep the generic Window API's
        // historical scaling behavior for callers that intentionally submit a
        // fixed-resolution framebuffer.
        let pixels = framebuffer.as_ptr() as *const c_void;

        // A display/device change can invalidate the HWND render target. Rebuild
        // the device-dependent resources once and retry without changing the
        // public API of PlatformWindow::present().
        if self.present_once(hwnd, pixels).is_err() {
            self.discard_device_resources();

            self.present_once(hwnd, pixels).expect(
                "Direct2D presentation failed after recreating the render target"
            );
        }
    }
}
