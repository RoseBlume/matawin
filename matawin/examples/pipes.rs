use matawin::{
    Window, Menu, TextAlign,
    colors::{WHITE, RED, BLACK},
    sprites::{SpriteType, SpriteRender, Sprite, Vector},
    events::{WindowEvent, Key},
    audio::{Audio, Bgs},
};

use std::{process, thread, time::{Duration, Instant}};
use rand::Rng;

// --- Input State ---
#[derive(Default)]
struct InputState {
    flap: bool,
    pause: bool,
    quit: bool,
}

// --- Main ---
fn main() {
    // --- Audio ---
    let bgs = Bgs::play("assets/audio/bgs/Crimson Turn-Based Clash.wav");
    let audio = Audio::new();

    // --- Window ---
    let mut window = Window::new("Flappy Bird Clone", (400, 600));
    window.set_background_color(BLACK);
    window.set_maximizable(false);
    window.set_resizable(false);
    // --- Menu ---
    let mut menu = Menu::new(vec!["Play", "Exit"], RED, WHITE);
    let mut player_index: usize;
    let mut rng = rand::rng();

    // --- Menu Loop ---
    loop {
        let events = window.poll_events();
        let mut main: bool = true;
        for event in events {
            match event {
                WindowEvent::Close => {
                    std::process::exit(0);
                },
                WindowEvent::KeyPressed(Key::Char('w')) |
                WindowEvent::KeyPressed(Key::Up) => menu.move_up(),

                WindowEvent::KeyPressed(Key::Char('s')) |
                WindowEvent::KeyPressed(Key::Down) => menu.move_down(),

                WindowEvent::KeyPressed(Key::Enter) => {
                    match menu.current() {
                        "Play" => {
                            main = false;
                            break;
                        }
                        "Exit" => return,
                        _ => {}
                    }
                }

                _ => {}
            }
        }
        if main == false {
            remove_menu_text(&mut window, &menu, "main_menu");
            player_index = window.sprites.len();
            window.sprites.push(Sprite {
                sprite_type: SpriteType::Player,
                health: 1,
                position: (100, 300),
                size: (30, 30),
                render: SpriteRender::Color(WHITE),
                vectors: vec![Vector::Velocity(0, 0)],
                is_solid: false,
            });
            break;
        }
        menu.draw(&mut window, "main_menu");
        window.present();
    }

    // --- Game State ---
    let mut score = 0;
    let mut last_pipe_spawn = Instant::now();
    let pipe_interval = 2.0;
    let pipe_gap = 150;
    let mut paused = false;
    let mut player_dead = false;

    let score_id = "score";
    window.show_text(score_id, &format!("Score: {}", score), (10, 10), 4, WHITE, TextAlign::AutoFit);

    // --- Game Loop ---
    loop {
        let mut input = InputState::default();

        // --- Events ---
        for event in window.poll_events() {
            match event {
                WindowEvent::Close => process::exit(0),
                WindowEvent::KeyPressed(Key::Escape) => input.pause = true,
                WindowEvent::KeyPressed(Key::Space) => input.flap = true,
                WindowEvent::KeyPressed(Key::Char('c')) => input.quit = true,
                _ => {}
            }
        }

        // --- Global Actions ---
        if input.pause {
            paused = !paused;
        }

        if input.quit {
            process::exit(0);
        }

        if paused {
            window.present();
            thread::sleep(Duration::from_millis(30));
            continue;
        }

        let (width, height) = window.get_size();

        // --- Flap ---
        if !player_dead && input.flap {
            window.sprites[player_index].set_velocity(0, -12);
            audio.play("assets/audio/sfx/shoot.wav");
        }

        // --- Gravity ---
        if !player_dead {
            window.sprites[player_index].set_acceleration(0, 1);
        }

        // --- Spawn Pipes ---
        if last_pipe_spawn.elapsed().as_secs_f32() > pipe_interval {
            spawn_pipe(&mut window, &mut rng, width, height, pipe_gap);
            last_pipe_spawn = Instant::now();
        }

        // --- Physics ---
        window.apply_vectors();

        // --- Collision ---
        if !player_dead {
            window.change_health_offscreen(SpriteType::Player, -100);
            window.change_health_on_collision(
                SpriteType::Player,
                SpriteType::Custom("Pipe"),
                -1
            );

            if window.sprites[player_index].health <= 0 {
                player_dead = true;
                audio.play("assets/audio/sfx/hit.wav");
            }
        }

        // --- Cleanup ---
        window.remove_if_out_of_screen(SpriteType::Custom("Pipe"));

        // --- Score ---
        score += 1;
        window.update_text(score_id, &format!("Score: {}", score));

        // --- Draw ---
        window.present();

        // --- Death Reset ---
        if player_dead {
            thread::sleep(Duration::from_secs(1));

            window.sprites.clear();
            score = 0;
            player_dead = false;
            last_pipe_spawn = Instant::now();

            player_index = window.sprites.len();
            window.sprites.push(Sprite {
                sprite_type: SpriteType::Player,
                health: 1,
                position: (100, 300),
                size: (30, 30),
                render: SpriteRender::Color(WHITE),
                vectors: vec![Vector::Velocity(0, 0)],
                is_solid: false,
            });
        }
    }
}

// --- Pipe Spawning ---
fn spawn_pipe(window: &mut Window, rng: &mut impl Rng, width: usize, height: usize, gap: usize) {
    let gap_y = rng.random_range(50..(height - gap - 50));

    window.sprites.push(Sprite {
        sprite_type: SpriteType::Custom("Pipe"),
        health: 1,
        position: (width, 0),
        size: (50, gap_y),
        render: SpriteRender::Color(0x00FF00),
        vectors: vec![Vector::Velocity(-5, 0)],
        is_solid: false,
    });

    window.sprites.push(Sprite {
        sprite_type: SpriteType::Custom("Pipe"),
        health: 1,
        position: (width, gap_y + gap),
        size: (50, height - gap_y - gap),
        render: SpriteRender::Color(0x00FF00),
        vectors: vec![Vector::Velocity(-5, 0)],
        is_solid: false,
    });
}

// --- Remove Menu Text ---
fn remove_menu_text(window: &mut Window, menu: &Menu, prefix: &str) {
    for i in 0..menu.options.len() {
        window.remove_text(&format!("{}_{}", prefix, i));
    }
}