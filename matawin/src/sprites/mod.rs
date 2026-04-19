use crate::image::{
    load_sprite_sheet,
    load_image_2d
};
use crate::window::AppWindow;
mod vectors;



/// Motion-related vectors applied to a sprite.
///
/// Vectors are evaluated by the engine to update sprite movement
/// and physics-like behavior.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vector {
    /// Constant velocity applied every update tick.
    ///
    /// Values represent `(dx, dy)` in pixels per frame.
    Velocity(i32, i32),

    /// Acceleration applied to velocity each update tick.
    ///
    /// Values represent `(ax, ay)` in pixels per frame².
    Acceleration(i32, i32),
}


/// Rendering data for a sprite.
///
/// Determines how a sprite is drawn to the screen.
#[derive(Clone)]
pub enum SpriteRender {
    /// Solid-color rectangle fill.
    ///
    /// The contained value is a packed color (ARGB or RGB,
    /// depending on renderer configuration).
    Color(u32),

    /// Static bitmap sprite.
    ///
    /// Pixels are stored in row-major order.
    /// A value of `0` is treated as transparent.
    Bitmap {
        /// 2D pixel buffer: rows → columns.
        pixels: Vec<Vec<u32>>,
    },

    /// Animated bitmap sprite.
    ///
    /// Frames are cycled automatically using a fixed frame delay.
    AnimatedBitmap {
        /// Animation frames stored as 2D pixel buffers.
        frames: Vec<Vec<Vec<u32>>>,

        /// Index of the currently displayed frame.
        frame_index: usize,

        /// Number of ticks between frame changes.
        frame_delay: u32,

        /// Internal frame timer.
        frame_timer: u32,
    },
}



/// Logical category of a sprite.
///
/// Used for collision detection, game rules,
/// and sprite management.
#[derive(Clone, PartialEq, Eq, Debug, Copy)]
pub enum SpriteType {
    /// Player-controlled entity.
    Player,

    /// Hostile or AI-controlled entity.
    Enemy,

    /// Projectile such as bullets or spells.
    Projectile,

    /// Static solid object that blocks movement.
    Wall,

    /// Non-interactive visual overlay.
    Overlay,

    /// User-defined sprite category.
    ///
    /// Useful for custom logic without modifying the enum.
    Custom(&'static str),
}


/// A renderable and interactive game entity.
///
/// Sprites represent all visible objects in the world,
/// including players, enemies, projectiles, and environment objects.
pub struct Sprite {
    /// Logical classification of the sprite.
    pub sprite_type: SpriteType,

    /// Current health value.
    ///
    /// When health reaches zero or below, the sprite is considered dead.
    pub health: i32,

    /// Top-left position in screen coordinates.
    pub position: (usize, usize),

    /// Logical size of the sprite in pixels.
    pub size: (usize, usize),

    /// Rendering data used to draw the sprite.
    pub render: SpriteRender,

    /// Whether the sprite blocks movement.
    pub is_solid: bool,

    /// Motion vectors applied to the sprite.
    ///
    /// Includes velocity and acceleration components.
    pub vectors: Vec<Vector>,
}




impl Sprite {
    // Multiply size by a defined amount
    pub fn upscale(&mut self, factor: usize) {
        if factor <= 1 {
            return;
        }

        match &mut self.render {
            SpriteRender::Bitmap { pixels } => {
                let original = pixels.clone();
                let h = original.len();
                let w = if h > 0 { original[0].len() } else { 0 };

                let mut upscaled = vec![vec![0u32; w * factor]; h * factor];

                for y in 0..h {
                    for x in 0..w {
                        let color = original[y][x];
                        if color == 0 { continue; }

                        for sy in 0..factor {
                            for sx in 0..factor {
                                upscaled[y * factor + sy][x * factor + sx] = color;
                            }
                        }
                    }
                }

                *pixels = upscaled;
                self.size = (w * factor, h * factor);
            }

            SpriteRender::AnimatedBitmap { frames, .. } => {
                for frame in frames.iter_mut() {
                    let original = frame.clone();
                    let h = original.len();
                    let w = if h > 0 { original[0].len() } else { 0 };

                    let mut upscaled = vec![vec![0u32; w * factor]; h * factor];

                    for y in 0..h {
                        for x in 0..w {
                            let color = original[y][x];
                            if color == 0 { continue; }

                            for sy in 0..factor {
                                for sx in 0..factor {
                                    upscaled[y * factor + sy][x * factor + sx] = color;
                                }
                            }
                        }
                    }

                    *frame = upscaled;
                }

                if let Some(frame) = frames.first() {
                    self.size = (frame[0].len(), frame.len());
                }
            }

            SpriteRender::Color(_) => {
                // Solid-color sprites just scale logically
                self.size = (self.size.0 * factor, self.size.1 * factor);
            }
        }
    }

    /// Create a solid-color rectangular sprite.
    pub fn new_color(
        position: (usize, usize),
        size: (usize, usize),
        sprite_type: SpriteType,
        health: i32,
        color: u32,
        is_solid: bool,
    ) -> Self {
        Sprite {
            sprite_type,
            health,
            position,
            size,
            render: SpriteRender::Color(color),
            is_solid,
            vectors: Vec::new(),
        }
    }

    /// Create a bitmap sprite from a 2D pixel buffer.
    ///
    /// Pixels with value `0` are treated as transparent.
    pub fn new_bitmap(
        position: (usize, usize),
        sprite_type: SpriteType,
        health: i32,
        pixels: Vec<Vec<u32>>,
        is_solid: bool,
    ) -> Self {
        let height = pixels.len();
        let width = if height > 0 { pixels[0].len() } else { 0 };

        Sprite {
            sprite_type,
            health,
            position,
            size: (width, height),
            render: SpriteRender::Bitmap { pixels },
            is_solid,
            vectors: Vec::new(),
        }
    }

    /// Create an animated bitmap sprite from preloaded frames.
    ///
    /// All frames are assumed to be the same size.
    pub fn new_animated_bitmap(
        position: (usize, usize),
        sprite_type: SpriteType,
        health: i32,
        frames: Vec<Vec<Vec<u32>>>,
        frame_delay: u32,
        is_solid: bool,
    ) -> Self {
        let (width, height) = if let Some(frame) = frames.first() {
            let h = frame.len();
            let w = if h > 0 { frame[0].len() } else { 0 };
            (w, h)
        } else {
            (0, 0)
        };

        Sprite {
            sprite_type,
            health,
            position,
            size: (width, height),
            render: SpriteRender::AnimatedBitmap {
                frames,
                frame_index: 0,
                frame_delay,
                frame_timer: 0,
            },
            is_solid,
            vectors: Vec::new(),
        }
    }

    /// Create a wall sprite (solid, indestructible).
    pub fn new_wall(
        position: (usize, usize),
        size: (usize, usize),
    ) -> Self {
        Sprite {
            sprite_type: SpriteType::Wall,
            health: i32::MAX,
            position,
            size,
            render: SpriteRender::Color(0x555555),
            is_solid: true,
            vectors: Vec::new(),
        }
    }
}

impl AppWindow {

    /// Create a bitmap sprite from an image file.
    ///
    /// The image is loaded from disk and used as the sprite's pixel data.
    /// Returns the index of the newly created sprite.
    pub fn create_bitmap_sprite_from_file(
        &mut self,
        position: (usize, usize),
        path: &str,
        sprite_type: SpriteType,
    ) -> usize {
        let bitmap = load_image_2d(path).expect("Failed to load image");
        self.create_bitmap_sprite(position, bitmap, sprite_type)
    }

    /// Create an animated sprite from multiple image files.
    ///
    /// Each file represents a single animation frame.
    /// Returns the index of the newly created sprite.
    pub fn create_animated_bitmap_sprite_from_files(
        &mut self,
        position: (usize, usize),
        health: i32,
        paths: Vec<String>,
        sprite_type: SpriteType,
        frame_delay: u32,
    ) -> usize {
        let frames: Vec<Vec<Vec<u32>>> = paths
            .iter()
            .map(|path| load_image_2d(path).expect("Failed to load image"))
            .collect();

        self.create_animated_bitmap_sprite(position, health, frames, sprite_type, frame_delay)
    }

    /// Create an animated sprite from preloaded frames.
    ///
    /// Each frame is a 2D bitmap. All frames are assumed to be the same size.
    /// Returns the index of the newly created sprite.
    pub fn create_animated_sprite(
        &mut self,
        position: (usize, usize),
        size: (usize, usize),
        sprite_type: SpriteType,
        health: i32,
        frames: Vec<Vec<Vec<u32>>>,
        frame_delay: u32,
    ) -> usize {
        self.sprites.push(Sprite {
            sprite_type,
            health,
            position,
            size,
            render: SpriteRender::AnimatedBitmap {
                frames,
                frame_index: 0,
                frame_delay,
                frame_timer: 0,
            },
            is_solid: false,
            vectors: Vec::new(),
        });
        self.sprites.len() - 1
    }

    /// Create a solid-colored rectangular sprite.
    ///
    /// Useful for debug objects, simple entities, or placeholders.
    /// Returns the index of the newly created sprite.
    pub fn create_colored_sprite(
        &mut self,
        position: (usize, usize),
        size: (usize, usize),
        sprite_type: SpriteType,
        health: i32,
        color: u32,
    ) -> usize {
        self.sprites.push(
            Sprite::new_color(position, size, sprite_type, health, color, false)
        );

        self.sprites.push(Sprite {
            sprite_type,
            health,
            position,
            size,
            render: SpriteRender::Color(color),
            is_solid: false,
            vectors: vec![Vector::Velocity(0, 0)],
        });
        self.sprites.len() - 1
    }

    /// Create a bitmap sprite from an already loaded 2D pixel buffer.
    ///
    /// Pixels with value `0` are treated as transparent.
    /// Returns the index of the newly created sprite.
    pub fn create_bitmap_sprite(
        &mut self,
        position: (usize, usize),
        bitmap: Vec<Vec<u32>>,
        sprite_type: SpriteType,
    ) -> usize {
        

        self.sprites.push(
            Sprite::new_bitmap(position, sprite_type, 1, bitmap, false)
        );

        self.sprites.len() - 1
    }

    /// Create an animated bitmap sprite from preloaded frames.
    ///
    /// Frames are advanced automatically using `frame_delay`.
    /// Returns the index of the newly created sprite.
    pub fn create_animated_bitmap_sprite(
        &mut self,
        position: (usize, usize),
        health: i32,
        bitmaps: Vec<Vec<Vec<u32>>>,
        sprite_type: SpriteType,
        frame_delay: u32,
    ) -> usize {
        self.sprites.push(
            Sprite::new_animated_bitmap(position, sprite_type, health, bitmaps, frame_delay, false)
        );

        self.sprites.len() - 1
    }

    /// Create an animated sprite from a sprite sheet.
    ///
    /// The sprite sheet is sliced into frames using the provided
    /// width and height.
    /// Returns the index of the newly created sprite.
    pub fn create_animated_sprite_from_sheet(
        &mut self,
        position: (usize, usize),
        health: i32,
        sheet: &str,
        width: u32,
        height: u32,
        sprite_type: SpriteType,
        frame_delay: u32,
    ) -> usize {
        let bitmaps = load_sprite_sheet(sheet, width, height)
            .expect("Failed to load sprite frames from sheet");

        let (width, height) = if let Some(frame) = bitmaps.first() {
            let h = frame.len();
            let w = if h > 0 { frame[0].len() } else { 0 };
            (w, h)
        } else {
            (0, 0)
        };

        self.sprites.push(Sprite {
            sprite_type,
            health,
            position,
            size: (width, height),
            render: SpriteRender::AnimatedBitmap {
                frames: bitmaps,
                frame_index: 0,
                frame_delay,
                frame_timer: 0,
            },
            is_solid: false,
            vectors: Vec::new(),
        });

        self.sprites.len() - 1
    }

    /// Create an indestructible wall sprite.
    ///
    /// Walls are solid and block movement.
    /// Returns the index of the newly created sprite.
    pub fn create_wall(&mut self, position: (usize, usize), size: (usize, usize)) -> usize {
        self.sprites.push(Sprite {
            sprite_type: SpriteType::Wall,
            health: i32::MAX,
            position,
            size,
            render: SpriteRender::Color(0x555555),
            is_solid: true,
            vectors: Vec::new(),
        });
        self.sprites.len() - 1
    }

    /// Advance the animation state of an animated sprite render.
    pub fn advance_animation(render: &mut SpriteRender) {
        if let SpriteRender::AnimatedBitmap {
            frames,
            frame_index,
            frame_delay,
            frame_timer,
        } = render {
            *frame_timer += 1;
            if *frame_timer >= *frame_delay {
                *frame_timer = 0;
                *frame_index = (*frame_index + 1) % frames.len();
            }
        }
    }

    /// Move a sprite to a new position.
    pub fn move_sprite(&mut self, index: usize, new_pos: (usize, usize)) {
        if let Some(sprite) = self.sprites.get_mut(index) {
            sprite.position = new_pos;
        }
    }

    /// Remove a sprite by index.
    pub fn remove_sprite(&mut self, index: usize) {
        if index < self.sprites.len() {
            self.sprites.remove(index);
        }
    }

    /// Invoke a callback for each sprite of a given type that has died.
    pub fn on_death<F>(&mut self, sprite_type: SpriteType, mut on_death: F)
    where
        F: FnMut(&mut AppWindow, usize),
    {
        for i in 0..self.sprites.len() {
            if self.sprites[i].sprite_type == sprite_type
                && self.sprites[i].health <= 0
            {
                on_death(self, i);
            }
        }
    }

    /// Invoke a callback when two sprite types collide.
    pub fn on_collision<F>(
        &mut self,
        a_type: SpriteType,
        b_type: SpriteType,
        mut on_collision: F,
    )
    where
        F: FnMut(&mut AppWindow, usize, usize),
    {
        let len = self.sprites.len();

        for i in 0..len {
            if self.sprites[i].sprite_type != a_type {
                continue;
            }

            let (x1, y1) = self.sprites[i].position;
            let (w1, h1) = self.sprites[i].size;

            for j in 0..len {
                if i == j || self.sprites[j].sprite_type != b_type {
                    continue;
                }

                let (x2, y2) = self.sprites[j].position;
                let (w2, h2) = self.sprites[j].size;

                let x1_max = x1.saturating_add(w1);
                let x2_max = x2.saturating_add(w2);
                let y1_max = y1.saturating_add(h1);
                let y2_max = y2.saturating_add(h2);

                if x1 < x2_max &&
                x1_max > x2 &&
                y1 < y2_max &&
                y1_max > y2
                {
                    on_collision(self, i, j);
                }
            }
        }
    }

    /// Change the health of sprites when they collide with another type.
    pub fn change_health_on_collision(
        &mut self,
        target_type: SpriteType,
        collider_type: SpriteType,
        health: i32,
    ) {
        let len = self.sprites.len();
        for i in 0..len {
            if self.sprites[i].sprite_type != target_type { continue; }

            for j in 0..len {
                if i == j || self.sprites[j].sprite_type != collider_type { continue; }

                let (s1, s2) = if i < j {
                    let (left, right) = self.sprites.split_at_mut(j);
                    (&mut left[i], &mut right[0])
                } else {
                    let (left, right) = self.sprites.split_at_mut(i);
                    (&mut right[0], &mut left[j])
                };

                if s1.position.0 < s2.position.0.saturating_add(s2.size.0)
                    && s1.position.0.saturating_add(s1.size.0) > s2.position.0
                    && s1.position.1 < s2.position.1.saturating_add(s2.size.1)
                    && s1.position.1.saturating_add(s1.size.1) > s2.position.1
                {
                    s1.health = s1.health.saturating_add(health);
                }
            }
        }
    }

    /// Remove all dead sprites of a given type.
    pub fn remove_on_death(&mut self, sprite_type: SpriteType) {
        let mut dead_indices = Vec::new();
        for (i, sprite) in self.sprites.iter().enumerate() {
            if sprite.sprite_type == sprite_type && sprite.health <= 0 {
                dead_indices.push(i);
            }
        }

        for &i in dead_indices.iter().rev() {
            self.remove_sprite(i);
        }
    }

    /// Remove sprites of a given type when they collide with another type.
    pub fn remove_on_collision(
        &mut self,
        collider_type: SpriteType,
        remove_type: SpriteType,
    ) {
        let mut dead_indices = Vec::new();
        let len = self.sprites.len();

        for i in 0..len {
            if self.sprites[i].sprite_type != remove_type {
                continue;
            }

            let (x1, y1) = self.sprites[i].position;
            let (w1, h1) = self.sprites[i].size;

            for j in 0..len {
                if i == j || self.sprites[j].sprite_type != collider_type {
                    continue;
                }

                let (x2, y2) = self.sprites[j].position;
                let (w2, h2) = self.sprites[j].size;

                if x1 < x2 + w2 &&
                   x1 + w1 > x2 &&
                   y1 < y2 + h2 &&
                   y1 + h1 > y2
                {
                    dead_indices.push(i);
                    break;
                }
            }
        }

        for &i in dead_indices.iter().rev() {
            self.remove_sprite(i);
        }
    }

    /// Remove sprites that are completely outside the screen bounds.
    pub fn remove_if_out_of_screen(&mut self, sprite_type: SpriteType) {
        let mut dead_indices = Vec::new();

        for (i, sprite) in self.sprites.iter().enumerate() {
            if sprite.sprite_type != sprite_type { continue; }

            let x = sprite.position.0 as i32;
            let y = sprite.position.1 as i32;
            let w = sprite.size.0 as i32;
            let h = sprite.size.1 as i32;

            if x + w <= 0 || x >= self.framebuffer.width as i32
                || y + h <= 0 || y >= self.framebuffer.height as i32
            {
                dead_indices.push(i);
            }
        }

        for &i in dead_indices.iter().rev() {
            self.remove_sprite(i);
        }
    }

    /// Clamp sprites of a given type so they remain inside the screen.
    pub fn prevent_leaving_screen(&mut self, sprite_type: SpriteType) {
        for sprite in self.sprites.iter_mut() {
            if sprite.sprite_type != sprite_type { continue; }

            let (w, h) = sprite.size;
            let (x, y) = sprite.position;

            let new_x = x.clamp(0, (self.framebuffer.width as usize).saturating_sub(w));
            let new_y = y.clamp(0, (self.framebuffer.height as usize).saturating_sub(h));

            sprite.position = (new_x, new_y);
        }
    }

    /// Increase a score value when sprites of a given type die.
    pub fn increment_on_sprite_death(
        &mut self,
        sprite_type: SpriteType,
        score: &mut i32,
        points: i32,
    ) {
        for sprite in self.sprites.iter_mut() {
            if sprite.sprite_type != sprite_type {
                continue;
            }

            if sprite.health <= 0 {
                *score += points;
                sprite.health = i32::MIN;
            }
        }
    }

    /// Change sprite health when fully outside the screen.
    pub fn change_health_offscreen(&mut self, sprite_type: SpriteType, health_change: i32) {
        let (width, height) = self.get_size();
        for sprite in &mut self.sprites {
            if sprite.sprite_type != sprite_type {
                continue;
            }

            let (x, y) = sprite.position;
            let (w, h) = sprite.size;

            if x + w <= 0 || x >= width || y + h <= 0 || y >= height {
                sprite.health = sprite.health.saturating_add(health_change);
            }
        }
    }
}