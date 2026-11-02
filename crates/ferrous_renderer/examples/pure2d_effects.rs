//! Small headless demo of the Pure2D layer effects.
//!
//! Renders three cards side by side — drop shadow, blurred group and glow — into
//! `brain/pure2d_effects.png` with the real offscreen-layer path.  Run from the
//! engine repository root:
//!
//! ```text
//! cargo run -p ferrous_renderer --example pure2d_effects
//! ```

use std::fs::File;
use std::io::BufWriter;

use ferrous_2d::render::{CompositeMode2d, Effect2d, Layer2d, ShapeInstance};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::Vec2;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 260;

const BACKGROUND: [f32; 4] = [0.05, 0.06, 0.08, 1.0];
const CARD: [f32; 4] = [0.88, 0.91, 0.96, 1.0];
const ACCENT: [f32; 4] = [0.20, 0.55, 1.00, 1.0];
const WARM: [f32; 4] = [1.00, 0.42, 0.30, 1.0];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut harness = Pure2dHarness::new(WIDTH, HEIGHT, DEFAULT_FORMAT)?;
    harness.set_clear_color(wgpu::Color {
        r: f64::from(BACKGROUND[0]),
        g: f64::from(BACKGROUND[1]),
        b: f64::from(BACKGROUND[2]),
        a: 1.0,
    });

    // Left: a card with a soft, offset shadow.
    harness.push_layer_with_effects(
        Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
        &[Effect2d::DropShadow {
            color: [0.0, 0.0, 0.0, 1.0],
            offset: [10.0, 12.0],
            blur: 10.0,
            opacity: 0.7,
        }],
    )?;
    harness.draw_shape(ShapeInstance::rounded_rect(
        Vec2::new(-230.0, 0.0),
        Vec2::new(150.0, 120.0),
        18.0,
        CARD,
    ));
    harness.draw_shape(ShapeInstance::circle(Vec2::new(-230.0, 0.0), 26.0, WARM));
    harness.pop_layer()?;

    // Centre: a blurred group (two overlapping shapes read as one soft blob).
    harness.push_layer_with_effects(
        Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
        &[Effect2d::Blur { radius: 9.0 }],
    )?;
    harness.draw_shape(ShapeInstance::circle(Vec2::new(-30.0, -18.0), 46.0, ACCENT));
    harness.draw_shape(ShapeInstance::circle(Vec2::new(30.0, 18.0), 46.0, WARM));
    harness.pop_layer()?;

    // Right: a dark disc with a cyan glow.
    harness.push_layer_with_effects(
        Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
        &[Effect2d::Glow {
            color: [0.20, 0.95, 0.90, 1.0],
            radius: 14.0,
            intensity: 0.9,
        }],
    )?;
    harness.draw_shape(ShapeInstance::circle(
        Vec2::new(220.0, 0.0),
        40.0,
        [0.02, 0.05, 0.06, 1.0],
    ));
    harness.pop_layer()?;

    harness.render_frame_blocking();
    let pixels = harness.read_pixels();

    std::fs::create_dir_all("brain")?;
    let file = File::create("brain/pure2d_effects.png")?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    println!("wrote brain/pure2d_effects.png");
    Ok(())
}
