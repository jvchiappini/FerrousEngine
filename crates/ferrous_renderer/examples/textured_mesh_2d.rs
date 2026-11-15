//! Headless demo of the generic textured 2D triangle mesh.
//!
//! Renders a checker texture on a flat sheet next to the same texture on a
//! bent sheet, through the real mesh pipeline, into
//! `brain/textured_mesh_2d.png`.  Run from the engine repository root:
//!
//! ```text
//! cargo run -p ferrous_renderer --example textured_mesh_2d
//! ```
//!
//! Positions are in the mesh's local space; the draw transform maps them to
//! world space.  A grid's positions in `[-0.5, 0.5]` plus uniform UVs in
//! `[0, 1]` are pixel-identical to a sprite of the same box.

use std::fs::File;
use std::io::BufWriter;

use ferrous_2d::render::{BlendMode2d, MeshVertex2d};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Vec2, Vec3};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 320;

/// A rectangle of `columns x rows` vertices spanning `[-0.5, 0.5]`.
fn grid(
    columns: u32,
    rows: u32,
    bend: impl Fn(f32, f32) -> (f32, f32),
) -> (Vec<MeshVertex2d>, Vec<u32>) {
    let mut vertices = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let u = column as f32 / (columns - 1) as f32;
            let v = row as f32 / (rows - 1) as f32;
            let (x, y) = bend(u - 0.5, v - 0.5);
            vertices.push(MeshVertex2d::new([x, y], [u, v]));
        }
    }
    let mut indices = Vec::new();
    for row in 0..rows - 1 {
        for column in 0..columns - 1 {
            let top_left = row * columns + column;
            let top_right = top_left + 1;
            let bottom_left = top_left + columns;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }
    (vertices, indices)
}

fn model(center: Vec2, size: f32) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::new(size, size, 1.0),
        glam::Quat::IDENTITY,
        Vec3::new(center.x, center.y, 0.0),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut harness = Pure2dHarness::new(WIDTH, HEIGHT, DEFAULT_FORMAT)?;
    harness.set_clear_color(wgpu::Color {
        r: 0.02,
        g: 0.03,
        b: 0.05,
        a: 1.0,
    });

    // A 64x64 checker plus a white border: orientation and UVs are visible.
    let mut texels = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let border = x < 2 || y < 2 || x >= 62 || y >= 62;
            let cell = ((x / 8) + (y / 8)) % 2 == 0;
            let color = if border {
                [245, 245, 250, 255]
            } else if cell {
                [60, 140, 240, 255]
            } else {
                [240, 90, 70, 255]
            };
            texels.extend_from_slice(&color);
        }
    }
    let texture = harness.register_texture_rgba8(64, 64, &texels);

    harness.begin_frame();
    let (vertices, indices) = grid(2, 2, |x, y| (x, y));
    harness.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::new(-150.0, 0.0), 220.0),
        BlendMode2d::Normal,
    );

    // The same sheet, bent: the top edge lifts and every row waves.
    let (vertices, indices) = grid(8, 6, |x, y| {
        let lift = (y + 0.5) * 0.32;
        let wave = 0.14 * (x * std::f32::consts::PI).sin();
        (x + wave * (y + 0.5), y + lift)
    });
    harness.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::new(150.0, -16.0), 220.0),
        BlendMode2d::Normal,
    );

    harness.render_frame_blocking();
    let pixels = harness.read_pixels();
    std::fs::create_dir_all("brain")?;
    let file = File::create("brain/textured_mesh_2d.png")?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    println!("wrote brain/textured_mesh_2d.png");
    Ok(())
}
