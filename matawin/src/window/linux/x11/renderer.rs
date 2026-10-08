//! Implements the `renderer` module.
//!
//! This module documents both public API items and internal implementation details.

use x11rb::{
    connection::Connection,
    protocol::xproto::{ ConnectionExt as _, Gcontext, ImageFormat, ImageOrder, Window },
    rust_connection::RustConnection,
};

/// CPU framebuffer presenter using core X11 `PutImage` requests.
///
/// The common 32-bit little-endian TrueColor layout matches matawin's native
/// `0xAARRGGBB` framebuffer in memory, so an unscaled frame is submitted directly
/// without repacking or copying. Scaling uses cached nearest-neighbour lookup
/// tables, avoiding integer division in the per-pixel hot loop.
pub(super) struct X11Renderer {
    /// Stores gc.
    ///
    /// Visibility: **private**.
    gc: Gcontext,
    /// Stores framebuffer size.
    ///
    /// Visibility: **private**.
    framebuffer_size: (u32, u32),
    /// Stores target size.
    ///
    /// Visibility: **private**.
    target_size: (u32, u32),
    /// Stores depth.
    ///
    /// Visibility: **private**.
    depth: u8,
    /// Stores bits per pixel.
    ///
    /// Visibility: **private**.
    bits_per_pixel: u8,
    /// Stores scanline pad.
    ///
    /// Visibility: **private**.
    scanline_pad: u8,
    /// Stores byte order.
    ///
    /// Visibility: **private**.
    byte_order: ImageOrder,
    /// Stores red mask.
    ///
    /// Visibility: **private**.
    red_mask: u32,
    /// Stores green mask.
    ///
    /// Visibility: **private**.
    green_mask: u32,
    /// Stores blue mask.
    ///
    /// Visibility: **private**.
    blue_mask: u32,
    /// Stores native bgra32.
    ///
    /// Visibility: **private**.
    native_bgra32: bool,
    /// Stores x map.
    ///
    /// Visibility: **private**.
    x_map: Vec<usize>,
    /// Stores y map.
    ///
    /// Visibility: **private**.
    y_map: Vec<usize>,
    /// Stores staging32.
    ///
    /// Visibility: **private**.
    staging32: Vec<u32>,
    /// Stores staging.
    ///
    /// Visibility: **private**.
    staging: Vec<u8>,
    /// Stores red lut.
    ///
    /// Visibility: **private**.
    red_lut: [u32; 256],
    /// Stores green lut.
    ///
    /// Visibility: **private**.
    green_lut: [u32; 256],
    /// Stores blue lut.
    ///
    /// Visibility: **private**.
    blue_lut: [u32; 256],
    /// Stores max request bytes.
    ///
    /// Visibility: **private**.
    max_request_bytes: usize,
}

impl X11Renderer {
    /// Creates a new instance.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Creates a new value. Visibility: parent-module-visible."]
    pub(super) fn new(
        conn: &RustConnection,
        window: Window,
        screen_num: usize,
        framebuffer_size: (u32, u32),
        target_size: (u32, u32)
    ) -> Self {
        let setup = conn.setup();
        let screen = &setup.roots[screen_num];
        let format = setup.pixmap_formats
            .iter()
            .find(|f| f.depth == screen.root_depth)
            .expect("X11 server has no pixmap format for the root depth");
        let visual = screen.allowed_depths
            .iter()
            .find(|d| d.depth == screen.root_depth)
            .and_then(|d| d.visuals.iter().find(|v| v.visual_id == screen.root_visual))
            .expect("X11 root visual was not found");

        let gc = conn.generate_id().expect("Failed to allocate X11 GC ID");
        conn.create_gc(gc, window, &Default::default()).expect("X11 create_gc failed");

        let native_bgra32 =
            format.bits_per_pixel == 32 &&
            setup.image_byte_order == ImageOrder::LSB_FIRST &&
            visual.red_mask == 0x00ff_0000 &&
            visual.green_mask == 0x0000_ff00 &&
            visual.blue_mask == 0x0000_00ff;

        let red_lut = component_lut(visual.red_mask);
        let green_lut = component_lut(visual.green_mask);
        let blue_lut = component_lut(visual.blue_mask);

        // Core X11's setup value is expressed in 4-byte units. Leave enough
        // headroom for the PutImage request header itself.
        let max_request_bytes = (setup.maximum_request_length as usize)
            .saturating_mul(4)
            .saturating_sub(64)
            .max(4096);

        let mut renderer = Self {
            gc,
            framebuffer_size,
            target_size,
            depth: screen.root_depth,
            bits_per_pixel: format.bits_per_pixel,
            scanline_pad: format.scanline_pad,
            byte_order: setup.image_byte_order,
            red_mask: visual.red_mask,
            green_mask: visual.green_mask,
            blue_mask: visual.blue_mask,
            native_bgra32,
            x_map: Vec::new(),
            y_map: Vec::new(),
            staging32: Vec::new(),
            staging: Vec::new(),
            red_lut,
            green_lut,
            blue_lut,
            max_request_bytes,
        };
        renderer.rebuild_scale_maps();
        renderer
    }

    #[inline]
    /// Performs the resize framebuffer operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Resizes framebuffer. Visibility: parent-module-visible."]
    pub(super) fn resize_framebuffer(&mut self, size: (u32, u32)) {
        if self.framebuffer_size != size {
            self.framebuffer_size = size;
            self.rebuild_scale_maps();
        }
    }

    #[inline]
    /// Performs the resize target operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Resizes target. Visibility: parent-module-visible."]
    pub(super) fn resize_target(&mut self, size: (u32, u32)) {
        if self.target_size != size {
            self.target_size = size;
            self.rebuild_scale_maps();
        }
    }

    /// Performs the rebuild scale maps operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the rebuild scale maps operation. Visibility: private/internal."]
    fn rebuild_scale_maps(&mut self) {
        let (sw, sh) = self.framebuffer_size;
        let (dw, dh) = self.target_size;
        self.x_map.clear();
        self.y_map.clear();

        if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
            return;
        }

        self.x_map.reserve(dw as usize);
        self.y_map.reserve(dh as usize);
        for x in 0..dw as usize {
            self.x_map.push((x * (sw as usize)) / (dw as usize));
        }
        for y in 0..dh as usize {
            self.y_map.push((y * (sh as usize)) / (dh as usize));
        }
    }

    /// Performs the present operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the present operation. Visibility: parent-module-visible."]
    pub(super) fn present(&mut self, conn: &RustConnection, window: Window, pixels: *const u32) {
        let (dw, dh) = self.target_size;
        let (sw, sh) = self.framebuffer_size;
        if dw == 0 || dh == 0 || dw > (u16::MAX as u32) || dh > (u16::MAX as u32) {
            return;
        }

        let source_len = (sw as usize) * (sh as usize);
        let source = unsafe { std::slice::from_raw_parts(pixels, source_len) };

        if self.native_bgra32 {
            if sw == dw && sh == dh {
                let bytes = as_bytes(source);
                self.submit(conn, window, bytes, dw, dh, (dw as usize) * 4);
                return;
            }

            let target_len = (dw as usize) * (dh as usize);
            self.staging32.resize(target_len, 0);
            scale_u32(
                source,
                sw as usize,
                &mut self.staging32,
                dw as usize,
                &self.x_map,
                &self.y_map
            );
            let bytes = as_bytes(&self.staging32);
            self.submit(conn, window, bytes, dw, dh, (dw as usize) * 4);
            return;
        }

        self.present_converted(conn, window, source);
    }

    /// Performs the present converted operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the present converted operation. Visibility: private/internal."]
    fn present_converted(&mut self, conn: &RustConnection, window: Window, source: &[u32]) {
        let (dw, dh) = self.target_size;
        let (sw, _) = self.framebuffer_size;
        let bpp = self.bits_per_pixel as usize;
        assert!(matches!(bpp, 16 | 24 | 32), "Unsupported X11 bits-per-pixel: {bpp}");

        let bytes_pp = bpp / 8;
        let pad_bits = self.scanline_pad.max(8) as usize;
        let stride_bits = (((dw as usize) * bpp + pad_bits - 1) / pad_bits) * pad_bits;
        let stride = stride_bits / 8;
        let total = stride.checked_mul(dh as usize).expect("X11 staging buffer overflow");
        self.staging.resize(total, 0);

        let same_width = sw == dw;
        for (y, &sy) in self.y_map.iter().enumerate() {
            let src_row = &source[sy * (sw as usize)..sy * (sw as usize) + (sw as usize)];
            let row_start = y * stride;
            let dst_row = &mut self.staging[row_start..row_start + (dw as usize) * bytes_pp];

            if same_width {
                for (pixel, dst) in src_row.iter().zip(dst_row.chunks_exact_mut(bytes_pp)) {
                    write_converted(
                        *pixel,
                        dst,
                        self.byte_order,
                        &self.red_lut,
                        &self.green_lut,
                        &self.blue_lut
                    );
                }
            } else {
                for (&sx, dst) in self.x_map.iter().zip(dst_row.chunks_exact_mut(bytes_pp)) {
                    write_converted(
                        src_row[sx],
                        dst,
                        self.byte_order,
                        &self.red_lut,
                        &self.green_lut,
                        &self.blue_lut
                    );
                }
            }
        }

        self.submit(conn, window, &self.staging, dw, dh, stride);
    }

    #[inline]
    /// Performs the submit operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the submit operation. Visibility: private/internal."]
    fn submit(
        &self,
        conn: &RustConnection,
        window: Window,
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize
    ) {
        let rows_per_chunk = (self.max_request_bytes / stride.max(1)).max(1);
        let mut y = 0usize;

        while y < (height as usize) {
            let rows = rows_per_chunk.min((height as usize) - y);
            let start = y * stride;
            let end = start + rows * stride;
            conn.put_image(
                ImageFormat::Z_PIXMAP,
                window,
                self.gc,
                width as u16,
                rows as u16,
                0,
                y as i16,
                0,
                self.depth,
                &bytes[start..end]
            ).expect("X11 put_image failed");
            y += rows;
        }

        conn.flush().expect("X11 framebuffer flush failed");
    }

    /// Performs the shutdown operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the shutdown operation. Visibility: parent-module-visible."]
    pub(super) fn shutdown(&mut self, conn: &RustConnection) {
        let _ = conn.free_gc(self.gc);
        let _ = conn.flush();
    }
}

#[inline]
/// Implements scale u32.
///
/// Visibility: **private**.
#[doc = "Implements the scale u32 operation. Visibility: private/internal."]
fn scale_u32(
    source: &[u32],
    sw: usize,
    target: &mut [u32],
    dw: usize,
    x_map: &[usize],
    y_map: &[usize]
) {
    if sw == dw {
        for (y, &sy) in y_map.iter().enumerate() {
            let src = &source[sy * sw..sy * sw + sw];
            let dst = &mut target[y * dw..y * dw + dw];
            dst.copy_from_slice(src);
        }
        return;
    }

    for (y, &sy) in y_map.iter().enumerate() {
        let src = &source[sy * sw..sy * sw + sw];
        let dst = &mut target[y * dw..y * dw + dw];
        for (pixel, &sx) in dst.iter_mut().zip(x_map.iter()) {
            *pixel = src[sx];
        }
    }
}

#[inline]
/// Implements as bytes.
///
/// Visibility: **private**.
#[doc = "Implements the as bytes operation. Visibility: private/internal."]
fn as_bytes(pixels: &[u32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), std::mem::size_of_val(pixels))
    }
}

/// Implements component lut.
///
/// Visibility: **private**.
#[doc = "Implements the component lut operation. Visibility: private/internal."]
fn component_lut(mask: u32) -> [u32; 256] {
    let mut lut = [0u32; 256];
    if mask == 0 {
        return lut;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    for (value, slot) in lut.iter_mut().enumerate() {
        *slot = ((((value as u32) * max + 127) / 255) << shift) & mask;
    }
    lut
}

#[inline]
/// Implements write converted.
///
/// Visibility: **private**.
#[doc = "Implements the write converted operation. Visibility: private/internal."]
fn write_converted(
    source: u32,
    dst: &mut [u8],
    order: ImageOrder,
    red: &[u32; 256],
    green: &[u32; 256],
    blue: &[u32; 256]
) {
    let r = ((source >> 16) & 0xff) as usize;
    let g = ((source >> 8) & 0xff) as usize;
    let b = (source & 0xff) as usize;
    let pixel = red[r] | green[g] | blue[b];

    match order {
        ImageOrder::LSB_FIRST => {
            let bytes = pixel.to_le_bytes();
            dst.copy_from_slice(&bytes[..dst.len()]);
        }
        ImageOrder::MSB_FIRST => {
            let bytes = pixel.to_be_bytes();
            let start = 4 - dst.len();
            dst.copy_from_slice(&bytes[start..]);
        }
        _ => unreachable!(),
    }
}
