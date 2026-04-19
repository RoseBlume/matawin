use super::{SpriteType, Vector, Sprite};
use std::collections::HashSet;
use crate::window::AppWindow;


impl Sprite {
    /// Set the sprite's velocity vector.
    ///
    /// If a velocity vector already exists, it is replaced.
    /// Otherwise, a new velocity vector is added.
    ///
    /// Values represent `(dx, dy)` in pixels per update tick.
    pub fn set_velocity(&mut self, vx: i32, vy: i32) {
        if let Some(Vector::Velocity(x, y)) =
            self.vectors.iter_mut().find(|v| matches!(v, Vector::Velocity(_, _)))
        {
            *x = vx;
            *y = vy;
        } else {
            self.vectors.push(Vector::Velocity(vx, vy));
        }
    }

    /// Update one or both components of the sprite's velocity.
    ///
    /// - `None` leaves the corresponding component unchanged.
    /// - If no velocity vector exists, one is created.
    pub fn update_velocity(&mut self, vx: Option<i32>, vy: Option<i32>) {
        if let Some(Vector::Velocity(x, y)) =
            self.vectors.iter_mut().find(|v| matches!(v, Vector::Velocity(_, _)))
        {
            if let Some(vx) = vx { *x = vx; }
            if let Some(vy) = vy { *y = vy; }
        } else {
            self.vectors.push(Vector::Velocity(
                vx.unwrap_or(0),
                vy.unwrap_or(0),
            ));
        }
    }

    /// Remove the sprite's velocity vector.
    ///
    /// After calling this method, the sprite will no longer
    /// move unless a new velocity is added.
    pub fn remove_velocity(&mut self) {
        self.vectors.retain(|v| !matches!(v, Vector::Velocity(_, _)));
    }

    /// Retrieve the current velocity of the sprite.
    ///
    /// Returns `Some((dx, dy))` if a velocity vector exists,
    /// or `None` if the sprite has no velocity.
    pub fn velocity(&self) -> Option<(i32, i32)> {
        self.vectors.iter().find_map(|v| {
            if let Vector::Velocity(x, y) = *v {
                Some((x, y))
            } else {
                None
            }
        })
    }

    /// Set the sprite's acceleration vector.
    ///
    /// If an acceleration vector already exists, it is replaced.
    /// Otherwise, a new acceleration vector is added.
    ///
    /// Values represent `(ax, ay)` in pixels per tick².
    pub fn set_acceleration(&mut self, ax: i32, ay: i32) {
        if let Some(Vector::Acceleration(x, y)) =
            self.vectors.iter_mut().find(|v| matches!(v, Vector::Acceleration(_, _)))
        {
            *x = ax;
            *y = ay;
        } else {
            self.vectors.push(Vector::Acceleration(ax, ay));
        }
    }

    /// Remove the sprite's acceleration vector.
    ///
    /// After calling this method, the sprite's velocity
    /// will no longer be modified by acceleration.
    pub fn remove_acceleration(&mut self) {
        self.vectors.retain(|v| !matches!(v, Vector::Acceleration(_, _)));
    }
}



impl AppWindow {
    /// Add one or multiple vectors to all sprites of a given type
    pub fn add_vector(&mut self, sprite_type: SpriteType, vector: Vector) {
        for sprite in self.sprites.iter_mut() {
            if sprite.sprite_type != sprite_type {
                continue;
            }

            let mut replaced = false;

            for existing in sprite.vectors.iter_mut() {
                match (existing, vector) {
                    (Vector::Velocity(vx, vy), Vector::Velocity(nx, ny)) => {
                        *vx = nx;
                        *vy = ny;
                        replaced = true;
                        break;
                    }

                    (Vector::Acceleration(ax, ay), Vector::Acceleration(nx, ny)) => {
                        *ax = nx;
                        *ay = ny;
                        replaced = true;
                        break;
                    }

                    _ => {}
                }
            }

            if !replaced {
                sprite.vectors.push(vector);
            }
        }
    }

    /// Add multiple vectors at once, preventing duplicates
    pub fn add_vectors(&mut self, sprite_type: SpriteType, vectors: impl Into<Vec<Vector>>) {
        let vecs: Vec<Vector> = vectors.into();
        for sprite in self.sprites.iter_mut() {
            if sprite.sprite_type == sprite_type {
                for &v in &vecs {
                    if !sprite.vectors.contains(&v) {
                        sprite.vectors.push(v);
                    }
                }
            }
        }
    }

    /// Remove specific vectors or all vectors of a type
    pub fn remove_vectors(&mut self, sprite_type: SpriteType, vector_to_remove: Option<Vector>) {
        for sprite in self.sprites.iter_mut() {
            if sprite.sprite_type != sprite_type { continue; }

            if let Some(v) = vector_to_remove {
                sprite.vectors.retain(|&vec| vec != v);
            } else {
                sprite.vectors.clear();
            }
        }
    }

    /// Update sprite positions based on velocity and acceleration vectors
    /// Apply vectors to update sprite positions
    pub fn apply_vectors(&mut self) {
        for sprite in self.sprites.iter_mut() {
            let mut dx = 0;
            let mut dy = 0;
            let mut seen = HashSet::new();

            // --- First pass: apply acceleration to velocity ---
            let mut accel_list = Vec::new();
            for vec in &sprite.vectors {
                if seen.contains(vec) { continue; }
                seen.insert(*vec);

                if let Vector::Acceleration(ax, ay) = vec {
                    accel_list.push((*ax, *ay));
                }
            }

            for (ax, ay) in accel_list {
                // find a velocity vector
                if let Some(Vector::Velocity(vx, vy)) = sprite.vectors.iter_mut()
                    .find(|v| matches!(v, Vector::Velocity(_, _)))
                {
                    *vx += ax;
                    *vy += ay;
                } else {
                    sprite.vectors.push(Vector::Velocity(ax, ay));
                }
            }

            // --- Second pass: apply velocity to position ---
            for vec in &sprite.vectors {
                if let Vector::Velocity(vx, vy) = vec {
                    dx += *vx;
                    dy += *vy;
                }
            }

            let (x, y) = sprite.position;
            sprite.position = ((x as i32 + dx) as usize, (y as i32 + dy) as usize);
        }
    }



}