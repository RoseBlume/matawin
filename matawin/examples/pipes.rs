//! Demonstrates the pipes MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use matawin::{
    Window,
    colors::{BLACK, WHITE, GREEN},
    sprites::{Sprite, SpriteType, SpriteRender, Vector},
    events::{WindowEvent, Key},
    image::load_image_2d,
    audio::Bgs
};



use std::{
    process,
    time::{Duration, Instant},
    sync::LazyLock,
    collections::HashMap
};

const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

const BIRD_X: u32 = 150;
const BIRD_SIZE: u32 = 48;

const GRAVITY: i32 = 1;
const FLAP_FORCE: i32 = -12;

const PIPE_WIDTH: u32 = 80;
const PIPE_SPEED: i32 = -5;
const PIPE_GAP: u32 = 180;

const ASSET_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets"
);
/// Demonstrates or supports `upscale_nearest`.
///
/// Visibility: **private to the example binary**.
fn upscale_nearest(
    image: Vec<Vec<u32>>,
    target_width: usize,
    target_height: usize,
) -> Vec<Vec<u32>> {
    let src_height = image.len();
    let src_width = image[0].len();

    let mut result = vec![vec![0; target_width]; target_height];

    for y in 0..target_height {
        let src_y = y * src_height / target_height;

        for x in 0..target_width {
            let src_x = x * src_width / target_width;

            result[y][x] = image[src_y][src_x];
        }
    }

    result
}

// Use by doing sprite.set_sprite_pixels(*SPRITE_CACHE.get("wing-down").unwrap()) or similar. This way we only load each sprite once.
static SPRITE_CACHE: LazyLock<HashMap<String, Vec<Vec<u32>>>> = LazyLock::new(|| {
    let mut sprite_cache = HashMap::new();

    sprite_cache.insert(
        "wing-down".to_string(),
        upscale_nearest(
            load_image_2d(
                &format!(
                    "{}/Sprites/Animated/bird/png/wing-down.png",
                    ASSET_DIR
                )
            )
            .expect("Failed to load wing-down sprite"),
            48,
            48,
        ),
    );

    sprite_cache.insert(
        "wing-up".to_string(),
        upscale_nearest(
            load_image_2d(
                &format!(
                    "{}/Sprites/Animated/bird/png/wing-up.png",
                    ASSET_DIR
                )
            )
            .expect("Failed to load wing-up sprite"),
            48,
            48,
        ),
    );

    sprite_cache.insert(
        "wing-flat".to_string(),
        upscale_nearest(
            load_image_2d(
                &format!(
                    "{}/Sprites/Animated/bird/png/wing-flat.png",
                    ASSET_DIR
                )
            )
            .expect("Failed to load wing-flat sprite"),
            48,
            48,
        ),
    );

    sprite_cache
});

struct Lcg {
    state: u64,
}

impl Lcg {
    /// Demonstrates or supports `new`.
    ///
    /// Visibility: **private to the example binary**.
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Demonstrates or supports `next`.
    ///
    /// Visibility: **private to the example binary**.
    fn next(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);

        (self.state >> 32) as u32
    }

    /// Demonstrates or supports `range`.
    ///
    /// Visibility: **private to the example binary**.
    fn range(&mut self, min: u32, max: u32) -> u32 {
        min + (self.next() as u32 % (max - min))
    }
}

/// Demonstrates or supports `spawn_pipe_pair`.
///
/// Visibility: **private to the example binary**.
fn spawn_pipe_pair(window: &mut Window, rng: &mut Lcg) {
    let gap_y = rng.range(150, WINDOW_HEIGHT - 150);

    let top_height = gap_y.saturating_sub(PIPE_GAP / 2);
    let bottom_y = gap_y + PIPE_GAP / 2;
    let bottom_height = WINDOW_HEIGHT.saturating_sub(bottom_y);

    window.sprites.push(Sprite {
        sprite_type: SpriteType::Enemy,
        health: 1,
        position: (WINDOW_WIDTH as usize, 0),
        size: (PIPE_WIDTH as usize, top_height as usize),
        render: SpriteRender::Color(GREEN),
        vectors: vec![Vector::Velocity(PIPE_SPEED, 0)],
        is_solid: true,
    });

    window.sprites.push(Sprite {
        sprite_type: SpriteType::Enemy,
        health: 1,
        position: (WINDOW_WIDTH as usize, bottom_y as usize),
        size: (PIPE_WIDTH as usize, bottom_height as usize),
        render: SpriteRender::Color(GREEN),
        vectors: vec![Vector::Velocity(PIPE_SPEED, 0)],
        is_solid: true,
    });

    // Invisible score trigger
    window.sprites.push(Sprite {
        sprite_type: SpriteType::Custom("Score"),
        health: 1,
        position: (
            WINDOW_WIDTH as usize + PIPE_WIDTH as usize / 2,
            0,
        ),
        size: (1, 1),
        render: SpriteRender::Color(BLACK),
        vectors: vec![Vector::Velocity(PIPE_SPEED, 0)],
        is_solid: false,
    });
}

/// Demonstrates or supports `reset_game`.
///
/// Visibility: **private to the example binary**.
fn reset_game(
    window: &mut Window,
    bird_index: usize,
    score: &mut u32,
    game_over: &mut bool,
    last_pipe_spawn: &mut Instant,
    last_animation: &mut Instant,
) {
    // The bird is created first, so remove every pipe/score marker
    // created after it.
    window.sprites.truncate(bird_index + 1);

    let bird = &mut window.sprites[bird_index];

    bird.position = (
        BIRD_X as usize,
        WINDOW_HEIGHT as usize / 2,
    );

    bird.health = 1;

    if let Some(Vector::Velocity(vx, vy)) = bird.vectors.get_mut(0) {
        *vx = 0;
        *vy = 0;
    }

    bird.set_sprite_pixels(
        SPRITE_CACHE
            .get("wing-flat")
            .unwrap()
            .clone(),
    );

    *score = 0;
    *game_over = false;

    *last_pipe_spawn = Instant::now();
    *last_animation = Instant::now();

    window.update_text(
        "score",
        "Score: 0",
    );

    // Hide the game-over text.
    window.update_text(
        "game_over",
        "",
    );
}

/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() {
    let mut window = Window::new("Flappy Bird", (WINDOW_WIDTH, WINDOW_HEIGHT));
    // format!("{}/audio/bgs/Crimson Gate Siege.wav", ASSET_DIR),
    // let mut bgs = Bgs::with_playlist(playlist, true);
    // let mut bgs = Bgs::new();
    let playlist = vec![
        format!("{}/audio/bgs/Crimson Gate Siege.wav", ASSET_DIR)
    ];
    let mut bgs = Bgs::with_playlist(playlist, true);
    // let bgs = Bgs::play(format!("{}/audio/bgs/Crimson Gate Siege.wav", ASSET_DIR));
    window.set_target_fps(60);
    window.set_resizable(false);
    window.set_background_color(BLACK);
    let mut last_animation = Instant::now();
    let bird_index = {
        window.sprites.push(Sprite {
            sprite_type: SpriteType::Player,
            health: 1,
            position: (BIRD_X as usize, WINDOW_HEIGHT as usize / 2),
            size: (BIRD_SIZE as usize, BIRD_SIZE as usize),
            render: SpriteRender::Bitmap { pixels: SPRITE_CACHE
                    .get("wing-flat")
                    .unwrap()
                    .clone() },
            vectors: vec![Vector::Velocity(0, 0)],
            is_solid: true,
        });

        window.sprites.len() - 1
    };

    let mut rng = Lcg::new(
        Instant::now()
            .elapsed()
            .as_nanos() as u64
    );

    let mut score = 0;
    let mut game_over = false;

    let mut last_pipe_spawn = Instant::now();

    window.show_text(
        "score",
        "Score: 0",
        (10, 10),
        4,
        WHITE,
        matawin::TextAlign::AutoFit,
    );

    loop {
        let mut flap = false;

        for event in window.poll_events() {
            match event {
                WindowEvent::Close => process::exit(0),

                WindowEvent::KeyPressed(Key::Enter) if game_over => {
                    reset_game(
                        &mut window,
                        bird_index,
                        &mut score,
                        &mut game_over,
                        &mut last_pipe_spawn,
                        &mut last_animation,
                    );
                }

                WindowEvent::KeyPressed(Key::Escape) if game_over => {
                    process::exit(0);
                }

                WindowEvent::KeyPressed(Key::Space)
                | WindowEvent::KeyPressed(Key::Up)
                    if !game_over =>
                {
                    flap = true;
                }

                _ => {}
            }
        }

        if !game_over {
            if flap {
                if let Some(Vector::Velocity(_, vy)) =
                    window.sprites[bird_index].vectors.get_mut(0)
                {
                    *vy = FLAP_FORCE;
                }
            }

            if let Some(Vector::Velocity(_, vy)) =
                window.sprites[bird_index].vectors.get_mut(0)
            {
                *vy += GRAVITY;
            }

            if last_animation.elapsed() >= Duration::from_millis(75) {
                let vy = match window.sprites[bird_index].vectors.get(0) {
                    Some(Vector::Velocity(_, vy)) => *vy,
                    _ => 0,
                };

                let sprite_name = if vy < -3 {
                    "wing-up"
                } else if vy > 3 {
                    "wing-down"
                } else {
                    "wing-flat"
                };
                window.sprites[bird_index].set_sprite_pixels(
                    SPRITE_CACHE
                        .get(sprite_name)
                        .unwrap()
                        .clone(),
                );


                last_animation = Instant::now();
            }

            if last_pipe_spawn.elapsed() >= Duration::from_millis(1500) {
                spawn_pipe_pair(&mut window, &mut rng);
                last_pipe_spawn = Instant::now();
            }

            window.apply_vectors();

            // Bird bounds
            let bird_y = window.sprites[bird_index].position.1 as u32;

            if bird_y == 0 || bird_y + BIRD_SIZE as u32 >= WINDOW_HEIGHT as u32 {
                game_over = true;
            }

            // Pipe collision
            window.on_collision(
                SpriteType::Player,
                SpriteType::Enemy,
                |_, _, _| {
                    game_over = true;
                },
            );

            // Scoring
            //
            // Each pipe pair has exactly one Score marker.
            // Once that marker passes the bird's X position,
            // score it and remove it.
            window.sprites.retain(|sprite| {
                if sprite.sprite_type == SpriteType::Custom("Score")
                    && sprite.position.0 <= BIRD_X as usize
                {
                    score += 1;
                    false
                } else {
                    true
                }
            });

            window.remove_if_out_of_screen(SpriteType::Enemy);
            window.remove_if_out_of_screen(SpriteType::Custom("Score"));

            window.update_text(
                "score",
                &format!("Score: {}", score),
            );
        } else {
            window.show_text(
                "game_over",
                "GAME OVER - Enter: Play Again - Esc: Exit",
                (100, 250),
                4,
                WHITE,
                matawin::TextAlign::AutoFit,
            );
        }

        window.present();
    }
}
