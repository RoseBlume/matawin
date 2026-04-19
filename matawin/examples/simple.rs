use matawin::{
    Window, Menu, TextAlign,
    colors::{WHITE, RED, BLACK},
    sprites::{SpriteType, SpriteRender, Sprite, Vector},
    events::{WindowEvent, Key},
};
use matawin::window::AppWindow;
fn main() {
    let size = (800, 600);
    let mut window = AppWindow::new( "matawin", size)
        .with_close_handler(|| {
            println!("Window closed");
            std::process::exit(0);
        })
        .with_resize_handler(|w, h| {
            println!("Window resized to {}x{}", w, h);
        })
        .with_key_pressed_handler(|key| {
            println!("Key pressed: {:?}", key);
        })
        .with_repositioned_handler(|x, y| {
            println!("Window repositioned to {}, {}", x, y);
        })
        .with_maximized_handler(|w, h| {
            println!("Window maximized to {}x{}", w, h);
        })
        .with_restored_handler(|w, h| {
            println!("Window restored to {}x{}", w, h);
        })
        .with_minimized_handler(|| {
            println!("Window minimized");
        });
    window.set_resizable(false);
    window.set_maximizable(false);


    // fb.clear(0xFF00FF00);
    window.set_decorations(true);
    window.set_title("matawin - Simple Example");
    loop {
        window.handle_events();
        window.present();
    }
}