//! Prints the start-screen recipe's inspect dump and layout counters.
//!
//! ```text
//! cargo run -p guimaker --example start_screen -- 1920 1080 1.5
//! ```

use guimaker::prelude::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let width: f32 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1920.0);
    let height: f32 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1080.0);
    let scale: f32 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1.5);

    let view = start_screen(0.0);
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(width, height), scale);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view, &mut frame, &mut text);
    println!("{}", ui.inspect());
    println!("stats: {:?}", frame.stats);
    println!("commands: {}", frame.display_list.commands.len());
    println!("damage: {:?}", frame.damage_union());
}
