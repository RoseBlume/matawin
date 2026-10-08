//! Clickable image gallery for files in
//! `workspace_manifest_dir/images/Family_Time`.
//!
//! The example resolves the Cargo workspace directory rather than relying on
//! the current working directory. Only the currently displayed image is
//! decoded, keeping memory use bounded even for a large family photo folder.

mod support;

use std::{
    error::Error,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use matawin::{
    events::{Key, WindowEvent},
    widgets::{FontRenderer, Picture, Rect, WidgetExt},
    Framebuffer, Window,
};

use support::{collect_files, workspace_manifest_dir};

/// Background colour used by the gallery.
///
/// Visibility: **private to the example binary**.
const BACKGROUND: u32 = 0xff16_181d;
/// Normal navigation-button colour.
///
/// Visibility: **private to the example binary**.
const BUTTON: u32 = 0xff34_3842;
/// Navigation-button colour while hovered.
///
/// Visibility: **private to the example binary**.
const BUTTON_HOVER: u32 = 0xff4a_5060;
/// Primary text colour.
///
/// Visibility: **private to the example binary**.
const TEXT: u32 = 0xfff2_f3f5;
/// Secondary text colour.
///
/// Visibility: **private to the example binary**.
const MUTED_TEXT: u32 = 0xffba_c0ca;

/// Calculated rectangles for the responsive gallery layout.
///
/// Visibility: **private to the example binary**.
#[derive(Debug, Clone, Copy)]
struct GalleryLayout {
    /// Area reserved for the current image.
    image: Rect,
    /// Click target for the previous-image button.
    previous: Rect,
    /// Click target for the next-image button.
    next: Rect,
    /// Area available for the current filename and counter.
    status: Rect,
}

impl GalleryLayout {
    /// Computes the gallery layout from the current native client size.
    ///
    /// Visibility: **private to the example binary**.
    fn for_size(width: u32, height: u32) -> Self {
        let width = width.max(320);
        let height = height.max(220);
        let margin = 16u32;
        let controls_height = 58u32;
        let button_width = 124u32.min(width.saturating_sub(margin * 2) / 3).max(88);
        let image_height = height
            .saturating_sub(controls_height)
            .saturating_sub(margin * 3)
            .max(1);

        let previous = Rect::new(
            margin as i32,
            (height - controls_height - margin) as i32,
            button_width,
            controls_height,
        );
        let next = Rect::new(
            width.saturating_sub(margin + button_width) as i32,
            previous.y,
            button_width,
            controls_height,
        );
        let status_x = previous.x + previous.width as i32 + 12;
        let status_right = next.x - 12;

        Self {
            image: Rect::new(
                margin as i32,
                margin as i32,
                width.saturating_sub(margin * 2),
                image_height,
            ),
            previous,
            next,
            status: Rect::new(
                status_x,
                previous.y,
                status_right.saturating_sub(status_x).max(0) as u32,
                controls_height,
            ),
        }
    }
}

/// Returns whether a path has an image extension handled by the Mata image
/// stack used by this example.
///
/// Visibility: **private to the example binary**.
fn is_image_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "bmp" | "jpeg" | "jpg" | "pgm" | "png" | "tga" | "webp"
    )
}

/// Fills a clipped rectangle in a software framebuffer.
///
/// Visibility: **private to the example binary**.
fn fill_rect(framebuffer: &mut Framebuffer, rect: Rect, color: u32) {
    let left = rect.x.max(0) as u32;
    let top = rect.y.max(0) as u32;
    let right = rect
        .x
        .saturating_add(rect.width as i32)
        .max(0)
        .min(framebuffer.width as i32) as u32;
    let bottom = rect
        .y
        .saturating_add(rect.height as i32)
        .max(0)
        .min(framebuffer.height as i32) as u32;

    if left >= right || top >= bottom {
        return;
    }

    for y in top..bottom {
        let start = y as usize * framebuffer.width as usize + left as usize;
        let end = start + (right - left) as usize;
        framebuffer.pixels[start..end].fill(color);
    }
}

/// Draws text approximately centred inside `rect` when a system font is
/// available. The gallery still works if no system TrueType font is found.
///
/// Visibility: **private to the example binary**.
fn draw_centered_text(
    font: &mut Option<FontRenderer>,
    framebuffer: &mut Framebuffer,
    text: &str,
    rect: Rect,
    point_size: u16,
    color: u32,
) {
    let Some(font) = font.as_mut() else {
        return;
    };

    let Ok((text_width, text_height)) = font.measure(text, point_size) else {
        return;
    };

    let x = rect
        .x
        .saturating_add((rect.width.saturating_sub(text_width) / 2) as i32);
    let y = rect
        .y
        .saturating_add((rect.height.saturating_sub(text_height) / 2) as i32);
    let _ = font.draw_text(framebuffer, text, x, y, point_size, color, Some(rect));
}

/// Loads `index` into the single reusable [`Picture`] instance.
///
/// Visibility: **private to the example binary**.
fn load_picture(picture: &Picture, images: &[PathBuf], index: usize) -> Result<(), Box<dyn Error>> {
    picture.set_file(&images[index])?;
    picture.set_alternative_text(images[index].file_name().and_then(|name| name.to_str()));
    Ok(())
}

/// Changes the current index by `delta`, wrapping at either end, then decodes
/// the newly selected image.
///
/// Visibility: **private to the example binary**.
fn change_image(
    picture: &Picture,
    images: &[PathBuf],
    index: &mut usize,
    delta: isize,
) -> Result<(), Box<dyn Error>> {
    let count = images.len() as isize;
    let next = (*index as isize + delta).rem_euclid(count) as usize;
    load_picture(picture, images, next)?;
    *index = next;
    Ok(())
}

/// Runs the image gallery example.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn Error>> {
    let gallery_dir = workspace_manifest_dir().join("assets_ignored").join("images").join("Family_Time");
    let images = collect_files(&gallery_dir, is_image_file)?;

    if images.is_empty() {
        return Err(format!(
            "no supported images were found in {}",
            gallery_dir.display()
        )
        .into());
    }

    let mut window = Window::new("MataWin Family Time gallery", (1000, 700));
    let (width, height) = window.client_size();
    let mut framebuffer = Framebuffer::from_color(width.max(1), height.max(1), BACKGROUND);
    let picture = Picture::new();
    picture.set_keep_aspect_ratio(true);

    let mut index = 0usize;
    load_picture(&picture, &images, index)?;

    let mut font = FontRenderer::system_default().ok();
    let mut events = Vec::with_capacity(16);
    let mut cursor = (0i32, 0i32);
    let mut previous_hover = false;
    let mut next_hover = false;
    let mut dirty = true;

    loop {
        window.poll_events_into(&mut events);

        // Query the actual client size after pumping native events. This keeps
        // the framebuffer at native resolution even when resize events are
        // coalesced during maximize/restore operations.
        let client_size = window.client_size();
        if client_size.0 > 0
            && client_size.1 > 0
            && (client_size.0 != framebuffer.width || client_size.1 != framebuffer.height)
        {
            framebuffer.resize_and_clear(client_size.0, client_size.1, BACKGROUND);
            dirty = true;
        }

        let layout = GalleryLayout::for_size(framebuffer.width, framebuffer.height);

        for event in events.iter().copied() {
            match event {
                WindowEvent::Close | WindowEvent::KeyPressed(Key::Escape) => return Ok(()),
                WindowEvent::CursorMoved(x, y) => {
                    cursor = (x, y);
                    let new_previous_hover = layout.previous.contains(x, y);
                    let new_next_hover = layout.next.contains(x, y);
                    if new_previous_hover != previous_hover || new_next_hover != next_hover {
                        previous_hover = new_previous_hover;
                        next_hover = new_next_hover;
                        dirty = true;
                    }
                }
                WindowEvent::KeyPressed(Key::MouseLeft) => {
                    if layout.previous.contains(cursor.0, cursor.1) {
                        change_image(&picture, &images, &mut index, -1)?;
                        dirty = true;
                    } else if layout.next.contains(cursor.0, cursor.1) {
                        change_image(&picture, &images, &mut index, 1)?;
                        dirty = true;
                    }
                }
                WindowEvent::KeyPressed(Key::Left) => {
                    change_image(&picture, &images, &mut index, -1)?;
                    dirty = true;
                }
                WindowEvent::KeyPressed(Key::Right) => {
                    change_image(&picture, &images, &mut index, 1)?;
                    dirty = true;
                }
                WindowEvent::Resized(_, _)
                | WindowEvent::Maximized(_, _)
                | WindowEvent::Restored(_, _) => {
                    dirty = true;
                }
                _ => {}
            }
        }

        if dirty {
            let layout = GalleryLayout::for_size(framebuffer.width, framebuffer.height);
            framebuffer.clear(BACKGROUND);
            picture.set_bounds(layout.image);
            picture.blit(&mut framebuffer);

            fill_rect(
                &mut framebuffer,
                layout.previous,
                if previous_hover { BUTTON_HOVER } else { BUTTON },
            );
            fill_rect(
                &mut framebuffer,
                layout.next,
                if next_hover { BUTTON_HOVER } else { BUTTON },
            );

            draw_centered_text(
                &mut font,
                &mut framebuffer,
                "Previous",
                layout.previous,
                18,
                TEXT,
            );
            draw_centered_text(
                &mut font,
                &mut framebuffer,
                "Next",
                layout.next,
                18,
                TEXT,
            );

            let name = images[index]
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Unnamed image");
            let status = format!("{} / {}  -  {}", index + 1, images.len(), name);
            draw_centered_text(
                &mut font,
                &mut framebuffer,
                &status,
                layout.status,
                16,
                MUTED_TEXT,
            );

            window.present(&framebuffer);
            dirty = false;
        } else {
            // Native event polling is non-blocking. Avoid burning a CPU core
            // while the gallery is idle and no redraw is required.
            thread::sleep(Duration::from_millis(8));
        }
    }
}
