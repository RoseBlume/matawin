//! Demonstrates the arc MataWin example.
//!
//! Example code is not part of the library API, but its functions are documented for completeness.

use matawin::{
    Window, Menu, TextAlign,
    colors::{WHITE, RED, BLACK},
    sprites::{SpriteType, SpriteRender, Sprite, Vector},
    events::{WindowEvent, Key},
    audio::{Audio, Bgs},
};

use rand::Rng;
use std::{time::{Instant}, process};
const ASSET_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets"
);
const IMAGE_FORMAT: &str = "tiff";
// --- Input ---
#[derive(Default)]
struct InputState {
    left: bool,
    right: bool,
    shoot: bool,
    select: bool,
    up: bool,
    down: bool,
    pause: bool,
}

// --- Utils ---
/// Demonstrates or supports `remove_menu_text`.
///
/// Visibility: **private to the example binary**.
fn remove_menu_text(window: &mut Window, menu: &Menu, prefix: &str) {
    for i in 0..menu.options.len() {
        window.remove_text(&format!("{}_{}", prefix, i));
    }
}

// --- Menu Input ---
/// Demonstrates or supports `handle_menu_input`.
///
/// Visibility: **private to the example binary**.
fn handle_menu_input(input: &InputState, menu: &mut Menu) {
    if input.up { menu.move_up(); }
    if input.down { menu.move_down(); }
}

// --- Main Menu ---
/// Demonstrates or supports `main_menu`.
///
/// Visibility: **private to the example binary**.
fn main_menu(window: &mut Window, _bgs: &mut Bgs) -> usize {
    let mut menu = Menu::new(vec!["Play", "Exit"], RED, BLACK);
    window.set_background_color(WHITE);

    let player_index;

    loop {
        let mut input = InputState::default();

        for event in window.poll_events() {
            match event {

                WindowEvent::KeyPressed(Key::Char('w')) |
                WindowEvent::KeyPressed(Key::Up) => input.up = true,

                WindowEvent::KeyPressed(Key::Char('s')) |
                WindowEvent::KeyPressed(Key::Down) => input.down = true,

                WindowEvent::KeyPressed(Key::Enter) => input.select = true,

                _ => {}
            }
        }

        let (width, height) = window.get_size();

        handle_menu_input(&input, &mut menu);

        if input.select {
            match menu.current() {
                "Play" => {
                    let frames = (1..=3)
                        .map(|i|
                            format!("{assets}/Sprites/Animated/Ship/{format}/shipsprite{idx}.{format}",
                                assets = ASSET_DIR,
                                idx = i,
                                format = IMAGE_FORMAT
                            )
                        )
                        .collect::<Vec<_>>();

                    player_index = window.create_animated_bitmap_sprite_from_files(
                        ((width / 2).saturating_sub(32), height - 100),
                        100,
                        frames,
                        SpriteType::Player,
                        120,
                    );
                    break;
                }
                "Exit" => process::exit(0),
                _ => {}
            }
        }

        menu.draw(window, "main_option");
        // bgs.update_playlist();
        window.present();
    }

    remove_menu_text(window, &menu, "main_option");
    window.set_background_color(BLACK);

    player_index
}

// --- Spawn ---
/// Demonstrates or supports `spawn_falling`.
///
/// Visibility: **private to the example binary**.
fn spawn_falling(
    window: &mut Window,
    rng: &mut impl Rng,
    width: usize,
    sprite_type: SpriteType,
    size: (usize, usize),
    color: u32,
    health: i32,
    speed: i32,
) {
    let x = rng.random_range(50..(width - 50));

    window.sprites.push(Sprite {
        sprite_type,
        health,
        position: (x, 0),
        size,
        render: SpriteRender::Color(color),
        vectors: vec![Vector::Velocity(0, speed)],
        is_solid: false,
    });
}

// --- Main ---
/// Demonstrates or supports `main`.
///
/// Visibility: **private to the example binary**.
fn main() {
    let playlist = vec![
        format!("{}/audio/bgs/Crimson Gate Siege.wav", ASSET_DIR),
        format!("{}/audio/bgs/Crimson Turn-Based Clash.wav", ASSET_DIR)
    ];

    let mut bgs = Bgs::with_playlist(playlist, true);
    // let mut bgs = Bgs::playlist();
    // bgs.playlist(playlist);
    // bgs.playing(true);

    let mut window = Window::new("Arc Shooter", (800, 600));
    window.set_maximizable(false);
    window.set_resizable(false);
    window.set_target_fps(60);
    window.set_background_color(BLACK);

    let audio = Audio::new();
    let mut rng = rand::rng();

    let mut player_index = main_menu(&mut window, &mut bgs);

    let mut score = 0;
    let mut last_spawn = Instant::now();
    let mut last_food_spawn = Instant::now();

    let score_id = "score";
    let health_id = "health";

    window.show_text(score_id, "Score: 0", (10, 10), 4, WHITE, TextAlign::AutoFit);
    window.show_text(health_id, "Health: 0", (10, 50), 4, WHITE, TextAlign::AutoFit);

    let mut paused = false;
    let mut pause_menu = Menu::new(vec!["Resume", "Exit"], RED, WHITE);
    let mut player_dead = false;

    // --- Game Loop ---
    loop {
        let mut input = InputState::default();
        for event in window.poll_events() {
            match event {
                WindowEvent::Close => process::exit(0),

                WindowEvent::KeyHeld(Key::Char('a')) |
                WindowEvent::KeyPressed(Key::Char('a')) => input.left = true,
                WindowEvent::KeyHeld(Key::Char('d')) |
                WindowEvent::KeyPressed(Key::Char('d')) => input.right = true,
                WindowEvent::KeyPressed(Key::Space) => input.shoot = true,

                WindowEvent::KeyPressed(Key::Escape) => input.pause = true,

                WindowEvent::KeyHeld(Key::Char('w')) |
                WindowEvent::KeyPressed(Key::Char('w')) |
                WindowEvent::KeyHeld(Key::Up) |
                WindowEvent::KeyPressed(Key::Up) => input.up = true,

                WindowEvent::KeyHeld(Key::Char('s')) |
                WindowEvent::KeyPressed(Key::Char('s')) |
                WindowEvent::KeyHeld(Key::Down) |
                WindowEvent::KeyPressed(Key::Down) => input.down = true,

                WindowEvent::KeyHeld(Key::Enter) |
                WindowEvent::KeyPressed(Key::Enter) => input.select = true,

                _ => {}
            }
        }

        if input.pause {
            paused = !paused;
            remove_menu_text(&mut window, &pause_menu, "pause_option");
        }

        // --- Pause ---
        if paused {
            handle_menu_input(&input, &mut pause_menu);

            if input.select {
                match pause_menu.current() {
                    "Resume" => {
                        paused = false;
                    }
                    "Exit" => {
                        window.sprites.clear();
                        player_index = main_menu(&mut window, &mut bgs);
                        paused = false;
                    }
                    _ => {}
                }
            }

            pause_menu.draw(&mut window, "pause_option");
            if paused == false {
                remove_menu_text(&mut window, &pause_menu, "pause_option");
            }
            window.present();
            continue;
        }

        let (width, _) = window.get_size();

        // --- Player ---
        if !player_dead {
            let mut pos = window.sprites[player_index].position;

            if input.left {
                pos.0 = pos.0.saturating_sub(10);
            }
            if input.right {
                pos.0 = pos.0.saturating_add(10)
                    .min(width - window.sprites[player_index].size.0);
            }

            if input.shoot {
                window.sprites.push(Sprite {
                    sprite_type: SpriteType::Projectile,
                    health: 1,
                    position: (pos.0.saturating_add(20), pos.1),
                    size: (10, 10),
                    render: SpriteRender::Color(WHITE),
                    vectors: vec![Vector::Velocity(0, -10)],
                    is_solid: false,
                });

                audio.play(&format!("{}/audio/sfx/shoot.wav", ASSET_DIR));
            }

            window.move_sprite(player_index, pos);
        }

        // --- Spawning ---
        if last_spawn.elapsed().as_secs_f32() > 1.2 {
            spawn_falling(&mut window, &mut rng, width, SpriteType::Enemy, (50, 50), RED, 30, 10);
            last_spawn = Instant::now();
        }

        if last_food_spawn.elapsed().as_secs_f32() > 15.0 {
            spawn_falling(&mut window, &mut rng, width, SpriteType::Custom("Food"), (30, 30), 0x00FF00, 1, 10);
            last_food_spawn = Instant::now();
        }

        // --- Systems ---
        window.apply_vectors();

        window.change_health_on_collision(SpriteType::Player, SpriteType::Enemy, -10);
        if window.sprites[player_index].health == 0 {
            player_dead = true;
        }
        window.change_health_on_collision(SpriteType::Player, SpriteType::Custom("Food"), 20);
        window.remove_on_collision(SpriteType::Player, SpriteType::Custom("Food"));
        window.on_collision(SpriteType::Enemy, SpriteType::Projectile, |_, _, _| {
            score += 10;
        });
        window.remove_on_collision(SpriteType::Player, SpriteType::Enemy);
        window.change_health_on_collision(SpriteType::Enemy, SpriteType::Projectile, -10);
        window.remove_on_collision(SpriteType::Enemy, SpriteType::Projectile);
        window.on_death(SpriteType::Enemy, |_, _| {
            audio.play(format!("{}/audio/sfx/hit.wav", ASSET_DIR));
            println!("{}/audio/sfx/hit.wav", ASSET_DIR);
        });
        window.remove_on_death(SpriteType::Enemy);
        window.remove_if_out_of_screen(SpriteType::Projectile);

        // --- HUD ---
        let health = if player_dead { 0 } else { window.sprites[player_index].health };
        window.update_text(score_id, &format!("Score: {}", score));
        window.update_text(health_id, &format!("Health: {}", health));

        window.present();
    }
}
