//! Implements the `renderer` module.
//!
//! This module documents both public API items and internal implementation details.

use std::{
    env,
    fs::{ remove_file, File, OpenOptions },
    io,
    os::{ fd::AsFd, unix::fs::{ FileExt, OpenOptionsExt } },
    path::PathBuf,
    process,
    sync::{ atomic::{ AtomicBool, AtomicU64, Ordering }, Arc },
};

use wayland_client::{ protocol::{ wl_buffer, wl_shm, wl_surface }, QueueHandle };

use super::WaylandState;

/// Constant used for buffer count.
///
/// Visibility: **private**.
const BUFFER_COUNT: usize = 3;

/// Static storage used for shm file counter.
///
/// Visibility: **private**.
static SHM_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Stores the state and data for buffer slot.
///
/// Visibility: **private**.
struct BufferSlot {
    /// Keep the backing file alive because frames are written into it using
    /// positional file writes.
    file: File,

    /// Stores buffer.
    ///
    /// Visibility: **private**.
    buffer: wl_buffer::WlBuffer,

    /// True while the Wayland compositor owns this buffer.
    busy: Arc<AtomicBool>,
}

/// Stores the state and data for buffer generation.
///
/// Visibility: **private**.
struct BufferGeneration {
    /// Stores size.
    ///
    /// Visibility: **private**.
    size: (u32, u32),
    /// Stores slots.
    ///
    /// Visibility: **private**.
    slots: Vec<BufferSlot>,
    /// Stores next slot.
    ///
    /// Visibility: **private**.
    next_slot: usize,
}

impl BufferGeneration {
    /// Creates a new instance.
    ///
    /// Visibility: **private**.
    #[doc = "Creates a new value. Visibility: private/internal."]
    fn new(shm: &wl_shm::WlShm, qh: &QueueHandle<WaylandState>, size: (u32, u32)) -> Self {
        let stride = size.0.checked_mul(4).expect("Wayland stride overflow");

        let bytes_per_buffer = stride
            .checked_mul(size.1)
            .expect("Wayland buffer size overflow") as usize;

        let mut slots = Vec::with_capacity(BUFFER_COUNT);

        for _ in 0..BUFFER_COUNT {
            let file = create_shm_file(bytes_per_buffer).expect(
                "Failed to create Wayland \
                     shared-memory backing file"
            );

            let pool = shm.create_pool(
                file.as_fd(),
                i32
                    ::try_from(bytes_per_buffer)
                    .expect(
                        "Wayland shared-memory \
                         buffer exceeds i32::MAX"
                    ),
                qh,
                ()
            );

            let busy = Arc::new(AtomicBool::new(false));

            let buffer = pool.create_buffer(
                0,
                i32
                    ::try_from(size.0)
                    .expect("Wayland width exceeds \
                         i32::MAX"),
                i32
                    ::try_from(size.1)
                    .expect("Wayland height exceeds \
                         i32::MAX"),
                i32
                    ::try_from(stride)
                    .expect("Wayland stride exceeds \
                         i32::MAX"),
                wl_shm::Format::Xrgb8888,
                qh,
                busy.clone()
            );

            /*
             * The wl_buffer remains valid after
             * destroying the wl_shm_pool.
             *
             * We retain our File because we use
             * it to update the same backing
             * storage with pwrite-style writes.
             */
            pool.destroy();

            slots.push(BufferSlot {
                file,
                buffer,
                busy,
            });
        }

        Self {
            size,
            slots,
            next_slot: 0,
        }
    }

    #[inline]
    /// Performs the all released operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the all released operation. Visibility: private/internal."]
    fn all_released(&self) -> bool {
        self.slots.iter().all(|slot| { !slot.busy.load(Ordering::Acquire) })
    }

    /// Returns the index of a free compositor
    /// buffer.
    ///
    /// Returning an index instead of a mutable
    /// reference prevents a long-lived borrow of
    /// the generation while the staging buffer is
    /// being generated.
    #[inline]
    #[doc = "Implements the free slot index operation. Visibility: private/internal."]
    fn free_slot_index(&mut self) -> Option<usize> {
        let len = self.slots.len();

        if len == 0 {
            return None;
        }

        let mut index = self.next_slot;

        for _ in 0..len {
            if !self.slots[index].busy.load(Ordering::Acquire) {
                self.next_slot = if index + 1 == len { 0 } else { index + 1 };

                return Some(index);
            }

            index += 1;

            if index == len {
                index = 0;
            }
        }

        None
    }
}

/// Wayland CPU framebuffer renderer using
/// ordinary file I/O rather than mmap.
///
/// Each compositor buffer is backed by an
/// ordinary user-owned temporary file.
///
/// The compositor maps that file through
/// `wl_shm`, while Matawin updates it using
/// positional file writes.
///
/// This avoids:
///
/// - `memmap2`
/// - direct mmap calls
/// - libc mmap bindings
/// - root privileges
/// - sudo
///
/// A single reusable staging framebuffer is
/// retained by the renderer, so presenting a
/// frame performs no heap allocation.
pub(super) struct WaylandRenderer {
    /// Stores shm.
    ///
    /// Visibility: **private**.
    shm: wl_shm::WlShm,

    /// Stores qh.
    ///
    /// Visibility: **private**.
    qh: QueueHandle<WaylandState>,

    /// Stores framebuffer size.
    ///
    /// Visibility: **private**.
    framebuffer_size: (u32, u32),

    /// Stores target size.
    ///
    /// Visibility: **private**.
    target_size: (u32, u32),

    /// Stores x map.
    ///
    /// Visibility: **private**.
    x_map: Vec<usize>,

    /// Stores y map.
    ///
    /// Visibility: **private**.
    y_map: Vec<usize>,

    /// Reusable scaled framebuffer.
    ///
    /// This replaces the writable mmap.
    staging: Vec<u32>,

    /// Stores the generation currently attached or ready for presentation.
    ///
    /// Visibility: **private**.
    current: Option<BufferGeneration>,

    /// Stores older buffer generations until the compositor releases them.
    ///
    /// Visibility: **private**.
    retired: Vec<BufferGeneration>,
}

impl WaylandRenderer {
    /// Creates a new instance.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Creates a new value. Visibility: parent-module-visible."]
    pub(super) fn new(
        shm: wl_shm::WlShm,
        qh: QueueHandle<WaylandState>,
        framebuffer_size: (u32, u32),
        target_size: (u32, u32)
    ) -> Self {
        let target_len = target_size.0.checked_mul(target_size.1).unwrap_or(0) as usize;

        let mut renderer = Self {
            shm,
            qh,
            framebuffer_size,
            target_size,
            x_map: Vec::new(),
            y_map: Vec::new(),
            staging: vec![
                0;
                target_len
            ],
            current: None,
            retired: Vec::new(),
        };

        renderer.rebuild_scale_maps();

        renderer
    }

    /// Performs the resize framebuffer operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Resizes framebuffer. Visibility: parent-module-visible."]
    pub(super) fn resize_framebuffer(&mut self, size: (u32, u32)) {
        if self.framebuffer_size == size {
            return;
        }

        self.framebuffer_size = size;
        self.rebuild_scale_maps();
    }

    /// Performs the resize target operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Resizes target. Visibility: parent-module-visible."]
    pub(super) fn resize_target(&mut self, size: (u32, u32)) {
        if self.target_size == size {
            return;
        }

        self.target_size = size;

        let new_len = size.0.checked_mul(size.1).unwrap_or(0) as usize;

        self.staging.resize(new_len, 0);

        self.rebuild_scale_maps();

        if let Some(generation) = self.current.take() {
            self.retired.push(generation);
        }

        self.collect_retired();
    }

    /// Performs the rebuild scale maps operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the rebuild scale maps operation. Visibility: private/internal."]
    fn rebuild_scale_maps(&mut self) {
        let (sw, sh) = self.framebuffer_size;

        let (tw, th) = self.target_size;

        self.x_map.clear();
        self.y_map.clear();

        if tw == 0 || th == 0 || sw == 0 || sh == 0 || (sw == tw && sh == th) {
            return;
        }

        self.x_map.reserve(tw as usize);

        self.y_map.reserve(th as usize);

        for x in 0..tw as usize {
            self.x_map.push((x * (sw as usize)) / (tw as usize));
        }

        for y in 0..th as usize {
            self.y_map.push((y * (sh as usize)) / (th as usize));
        }
    }

    #[inline]
    /// Performs the collect retired operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the collect retired operation. Visibility: private/internal."]
    fn collect_retired(&mut self) {
        if self.retired.is_empty() {
            return;
        }

        self.retired.retain(|generation| { !generation.all_released() });
    }

    /// Performs the ensure generation operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the ensure generation operation. Visibility: private/internal."]
    fn ensure_generation(&mut self) {
        if self.target_size.0 == 0 || self.target_size.1 == 0 {
            return;
        }

        let create = self.current
            .as_ref()
            .map(|generation| { generation.size != self.target_size })
            .unwrap_or(true);

        if !create {
            return;
        }

        if let Some(old) = self.current.take() {
            self.retired.push(old);
        }

        self.current = Some(BufferGeneration::new(&self.shm, &self.qh, self.target_size));
    }

    /// Performs the present operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the present operation. Visibility: parent-module-visible."]
    pub(super) fn present(&mut self, surface: &wl_surface::WlSurface, pixels: *const u32) {
        if self.target_size.0 == 0 || self.target_size.1 == 0 {
            return;
        }

        self.collect_retired();
        self.ensure_generation();

        /*
         * Pick a free buffer before doing any
         * scaling work. If all three buffers are
         * owned by the compositor, simply drop
         * this frame.
         */
        let slot_index = {
            let Some(generation) = self.current.as_mut() else {
                return;
            };

            let Some(index) = generation.free_slot_index() else {
                return;
            };

            index
        };

        let source_size = self.framebuffer_size;

        let target_size = self.target_size;

        let source_len = source_size.0
            .checked_mul(source_size.1)
            .expect("Source framebuffer size \
                     overflow") as usize;

        let target_len = target_size.0
            .checked_mul(target_size.1)
            .expect("Target framebuffer size \
                     overflow") as usize;

        if self.staging.len() != target_len {
            self.staging.resize(target_len, 0);
        }

        /*
         * SAFETY:
         *
         * The caller guarantees that `pixels`
         * points to at least
         *
         * framebuffer_width *
         * framebuffer_height
         *
         * valid u32 pixels.
         */
        let source = unsafe { std::slice::from_raw_parts(pixels, source_len) };

        scale_nearest(
            source,
            source_size,
            &mut self.staging,
            target_size,
            &self.x_map,
            &self.y_map
        );

        let Some(generation) = self.current.as_mut() else {
            return;
        };

        let slot = &mut generation.slots[slot_index];

        /*
         * Convert our u32 framebuffer into its
         * byte representation without copying.
         *
         * File writes ultimately operate on
         * bytes, so this merely creates another
         * view over the same Vec allocation.
         */
        let bytes = u32_slice_as_bytes(&self.staging);

        /*
         * write_all_at maps to positional writes
         * on Unix.
         *
         * It does not alter the file cursor and
         * does not require the file to be memory
         * mapped by this process.
         *
         * The compositor's MAP_SHARED mapping of
         * this same file observes updates through
         * normal kernel page-cache coherence.
         *
         * fsync/msync is not necessary for
         * Wayland presentation.
         */
        if let Err(error) = slot.file.write_all_at(bytes, 0) {
            eprintln!("Failed to write Wayland \
                 framebuffer: {error}");

            return;
        }

        /*
         * Prevent compiler/CPU ordering from
         * moving our buffer ownership handoff
         * before the completed framebuffer
         * write.
         */
        std::sync::atomic::fence(Ordering::Release);

        slot.busy.store(true, Ordering::Release);

        surface.attach(Some(&slot.buffer), 0, 0);

        surface.damage_buffer(
            0,
            0,
            i32::try_from(target_size.0).unwrap_or(i32::MAX),
            i32::try_from(target_size.1).unwrap_or(i32::MAX)
        );

        surface.commit();
    }

    /// Performs the shutdown operation.
    ///
    /// Visibility: **parent-module-visible**.
    #[doc = "Implements the shutdown operation. Visibility: parent-module-visible."]
    pub(super) fn shutdown(&mut self) {
        self.current = None;
        self.retired.clear();
        self.staging.clear();
    }
}

/// Reinterpret a u32 pixel slice as bytes.
///
/// This performs no allocation and no copy.
///
/// Since this framebuffer is passed directly to
/// a native Wayland XRGB8888 buffer, native
/// byte order is intentionally preserved.
#[inline(always)]
#[doc = "Implements the u32 slice as bytes operation. Visibility: private/internal."]
fn u32_slice_as_bytes(pixels: &[u32]) -> &[u8] {
    /*
     * SAFETY:
     *
     * - u8 has alignment 1.
/// - every bit pattern is valid for u8.
/// - the resulting range covers exactly the
///   original u32 slice allocation.
/// - the lifetime remains tied to `pixels`.
     */
    unsafe {
        std::slice::from_raw_parts(
            pixels.as_ptr().cast::<u8>(),
            pixels.len() * std::mem::size_of::<u32>()
        )
    }
}

/// Implements create shm file.
///
/// Visibility: **private**.
#[doc = "Implements the create shm file operation. Visibility: private/internal."]
fn create_shm_file(len: usize) -> io::Result<File> {
    /*
     * XDG_RUNTIME_DIR belongs to the currently
     * logged-in user and normally has mode 0700.
     *
     * No root privileges are required.
     */
    let runtime_dir = env
        ::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);

    let pid = process::id();

    for _ in 0..128 {
        let id = SHM_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);

        let path = runtime_dir.join(format!(".matawin-wl-shm-\
                     {pid}-{id}"));

        let result = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)

            /*
             * The file belongs only to the
             * current user.
             */
            .mode(0o600)
            .open(&path);

        match result {
            Ok(file) => {
                if let Err(error) = file.set_len(len as u64) {
                    let _ = remove_file(&path);

                    return Err(error);
                }

                /*
                 * Unlink immediately.
                 *
                 * Both this process and the
                 * Wayland compositor continue to
                 * hold descriptors to the file,
                 * so its storage remains alive,
                 * but there is no persistent
                 * filesystem entry to clean up.
                 */
                let _ = remove_file(&path);

                return Ok(file);
            }

            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                continue;
            }

            Err(error) => {
                return Err(error);
            }
        }
    }

    Err(
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique \
             Wayland shared-memory file"
        )
    )
}

#[inline]
/// Implements scale nearest.
///
/// Visibility: **private**.
#[doc = "Implements the scale nearest operation. Visibility: private/internal."]
fn scale_nearest(
    source: &[u32],
    source_size: (u32, u32),
    target: &mut [u32],
    target_size: (u32, u32),
    x_map: &[usize],
    y_map: &[usize]
) {
    let sw = source_size.0 as usize;

    let sh = source_size.1 as usize;

    let tw = target_size.0 as usize;

    let th = target_size.1 as usize;

    debug_assert_eq!(source.len(), sw * sh);

    debug_assert_eq!(target.len(), tw * th);

    /*
     * Fastest path: dimensions match.
     *
     * This becomes one contiguous memcpy.
     */
    if sw == tw && sh == th {
        target.copy_from_slice(source);

        return;
    }

    /*
     * Width matches.
     *
     * Only vertical scaling is necessary, so
     * each destination scanline becomes one
     * contiguous memcpy.
     */
    if sw == tw {
        for (y, &sy) in y_map.iter().enumerate() {
            let src_start = sy * sw;

            let dst_start = y * tw;

            let src = &source[src_start..src_start + sw];

            let dst = &mut target[dst_start..dst_start + tw];

            dst.copy_from_slice(src);
        }

        return;
    }

    /*
     * General nearest-neighbour scaling.
     *
     * x_map and y_map are precomputed during a
     * resize, so the hot presentation loop
     * contains no divisions.
     */
    for (y, &sy) in y_map.iter().enumerate() {
        let src_start = sy * sw;

        let dst_start = y * tw;

        let src = &source[src_start..src_start + sw];

        let dst = &mut target[dst_start..dst_start + tw];

        for (pixel, &sx) in dst.iter_mut().zip(x_map.iter()) {
            *pixel = src[sx];
        }
    }
}
