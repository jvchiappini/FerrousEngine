//! Textured triangle mesh tests for the Pure2D path.
//!
//! The mesh pipeline must be pixel-equivalent to the sprite path for an
//! undeformed grid, respect layers/masks/blend modes and in-place texture
//! updates, map arbitrary UVs (cropping), and never panic on an empty or
//! degenerate mesh.  Every test skips cleanly when no GPU adapter is
//! available, mirroring the other headless tests.

use std::sync::Mutex;

use ferrous_2d::render::{
    BlendMode2d, CompositeMode2d, Layer2d, Mask2d, MaskMode2d, MeshVertex2d, ShapeInstance,
    SpriteInstance, TextureOptions2d,
};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Quat, Vec2, Vec3};

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn harness(width: u32, height: u32, clear: wgpu::Color) -> Option<Pure2dHarness> {
    match Pure2dHarness::with_clear_color(width, height, DEFAULT_FORMAT, clear) {
        Ok(harness) => Some(harness),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            None
        }
    }
}

fn sprite(center: Vec2, size: Vec2, uv_rect: [f32; 4]) -> SpriteInstance {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(center.x, center.y, 0.0),
    );
    SpriteInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: [1.0, 1.0, 1.0, 1.0],
        uv_rect,
        properties: [0.0, 0.0, 0.0, 0.0],
    }
}

/// Model matrix of the sprite path (scale by the box size, then place it).
fn model(center: Vec2, size: Vec2) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(center.x, center.y, 0.0),
    )
}

/// A rectangular grid in the same local space as a sprite quad: positions in
/// `[-0.5, 0.5]` and uniform UVs in `[0, 1]` (v grows upwards, as in a sprite).
fn grid(
    columns: u32,
    rows: u32,
    uv: [f32; 4],
    bend: impl Fn(f32, f32) -> (f32, f32),
) -> (Vec<MeshVertex2d>, Vec<u32>) {
    let mut vertices = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let u = column as f32 / (columns - 1) as f32;
            let v = row as f32 / (rows - 1) as f32;
            let (x, y) = bend(u - 0.5, v - 0.5);
            vertices.push(MeshVertex2d::new(
                [x, y],
                [uv[0] + u * (uv[2] - uv[0]), uv[1] + v * (uv[3] - uv[1])],
            ));
        }
    }
    let mut indices = Vec::with_capacity(((columns - 1) * (rows - 1) * 6) as usize);
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

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn near(actual: [u8; 4], expected: [u8; 4], tolerance: u8, what: &str) {
    for (channel, (&got, &want)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            got.abs_diff(want) <= tolerance,
            "{what}: channel {channel} = {got}, expected {want} +/- {tolerance} ({actual:?})"
        );
    }
}

/// A 4x4 texture with four distinct quadrants, so a crop or an orientation
/// error is visible.
fn quadrant_texture(harness: &mut Pure2dHarness) -> Result<u32, ferrous_2d::render::TextureError> {
    let mut data = Vec::with_capacity(4 * 4 * 4);
    for y in 0..4 {
        for x in 0..4 {
            let color = match (x >= 2, y >= 2) {
                (false, false) => [255, 0, 0, 255],
                (true, false) => [0, 255, 0, 255],
                (false, true) => [0, 0, 255, 255],
                (true, true) => [255, 255, 0, 255],
            };
            data.extend_from_slice(&color);
        }
    }
    harness.register_texture_2d(4, 4, &data, TextureOptions2d::pixel_art())
}

/// An undeformed 2x2 (and a denser 4x4) grid samples exactly the sprite's
/// pixels.  The documented tolerance is +/- 3 per channel: the mesh and the
/// sprite use different triangle diagonals (top-right/bottom-left versus
/// bottom-right/top-left) and the hardware may take a slightly different
/// sample position on the shared edge.  The UV mapping is affine, so the
/// interior is mathematically identical.
#[test]
fn an_undeformed_grid_matches_the_sprite() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(32, 32, wgpu::Color::BLACK) else {
        return;
    };
    let texture = quadrant_texture(&mut h).unwrap();
    let size = Vec2::new(24.0, 20.0);
    let uv = [0.0, 0.0, 1.0, 1.0];

    h.begin_frame();
    h.draw_sprite(texture, sprite(Vec2::ZERO, size, uv));
    h.render_frame_blocking();
    let reference = h.read_pixels();

    for (columns, rows) in [(2, 2), (4, 4), (8, 8)] {
        let (vertices, indices) = grid(columns, rows, uv, |x, y| (x, y));
        h.begin_frame();
        assert!(h.draw_textured_mesh(
            texture,
            &vertices,
            &indices,
            model(Vec2::ZERO, size),
            BlendMode2d::Normal
        ));
        h.render_frame_blocking();
        let pixels = h.read_pixels();
        for (index, (got, want)) in pixels
            .chunks_exact(4)
            .zip(reference.chunks_exact(4))
            .enumerate()
        {
            let got = [got[0], got[1], got[2], got[3]];
            let want = [want[0], want[1], want[2], want[3]];
            let what = format!("{columns}x{rows} grid pixel {index}");
            near(got, want, 3, &what);
        }
    }
}

/// A bent grid is a documented golden: the wave lifts the top edge, the
/// bottom stays flat and the sampled pixels at fixed probes match the
/// reference raster.  Probes are chosen away from edges to keep the golden
/// stable across rasterisers.
#[test]
fn a_bent_mesh_matches_the_golden_probes() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(40, 40, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[255, 255, 255, 255], TextureOptions2d::pixel_art())
        .unwrap();
    // A 6x4 sheet: the top row rises by up to 0.35 and every row bends
    // horizontally by a quarter sine.  Positions are in local units, so the
    // model scales the box to 20x20 pixels centred at the origin.
    let (vertices, indices) = grid(6, 4, [0.0, 0.0, 1.0, 1.0], |x, y| {
        let lift = (y + 0.5) * 0.35;
        let wave = 0.18 * (x * std::f32::consts::PI).sin();
        (x + wave * (y + 0.5), y + lift)
    });
    h.begin_frame();
    assert!(h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::ZERO, Vec2::splat(20.0)),
        BlendMode2d::Normal
    ));
    h.render_frame_blocking();
    let pixels = h.read_pixels();

    // The sheet covers the centre; the lifted top edge (pixel y ~= 3) exposes
    // the clear colour above it.
    near(
        pixel(&pixels, 40, 20, 20),
        [255, 255, 255, 255],
        2,
        "centre",
    );
    near(
        pixel(&pixels, 40, 20, 1),
        [0, 0, 0, 255],
        2,
        "above the bend",
    );
    // The bottom edge is untouched by the bend and stays at y = 30.
    near(
        pixel(&pixels, 40, 20, 29),
        [255, 255, 255, 255],
        2,
        "bottom edge",
    );
    // The wave keeps the sheet clear of the far-left column.
    near(
        pixel(&pixels, 40, 4, 20),
        [0, 0, 0, 255],
        2,
        "left of the sheet",
    );
    // White coverage is a golden range: the bent silhouette is not a 20x20
    // square (400 px) but a slightly taller sheet.
    let white = pixels
        .chunks_exact(4)
        .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
        .count();
    assert!(
        (400..700).contains(&white),
        "bent mesh covers {white} white pixels"
    );
}

/// UVs are explicit: a mesh can crop a texture exactly like a sprite's
/// `uv_rect`, and the right half of the texture lands on the right half of the
/// quad.
#[test]
fn mesh_uvs_crop_exactly_like_a_sprite() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(16, 16, wgpu::Color::BLACK) else {
        return;
    };
    let texture = quadrant_texture(&mut h).unwrap();
    let size = Vec2::new(16.0, 16.0);
    // Crop the top-right green quadrant only.
    let uv = [0.5, 0.5, 1.0, 1.0];
    let (vertices, indices) = grid(2, 2, uv, |x, y| (x, y));

    h.begin_frame();
    assert!(h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::ZERO, size),
        BlendMode2d::Normal
    ));
    h.render_frame_blocking();
    let mesh = h.read_pixels();

    h.begin_frame();
    h.draw_sprite(texture, sprite(Vec2::ZERO, size, uv));
    h.render_frame_blocking();
    let reference = h.read_pixels();
    for (index, (got, want)) in mesh
        .chunks_exact(4)
        .zip(reference.chunks_exact(4))
        .enumerate()
    {
        near(
            [got[0], got[1], got[2], got[3]],
            [want[0], want[1], want[2], want[3]],
            3,
            &format!("cropped pixel {index}"),
        );
    }
}

/// A mesh samples the live texture: an in-place update is visible on the next
/// frame without re-registering anything.
#[test]
fn a_mesh_sees_in_place_texture_updates() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let red = [255, 0, 0, 255].repeat(4);
    let texture = h.register_texture_rgba8(2, 2, &red);
    let (vertices, indices) = grid(2, 2, [0.0, 0.0, 1.0, 1.0], |x, y| (x, y));
    let transform = model(Vec2::ZERO, Vec2::splat(8.0));

    h.begin_frame();
    h.draw_textured_mesh(texture, &vertices, &indices, transform, BlendMode2d::Normal);
    h.render_frame_blocking();
    near(pixel(&h.read_pixels(), 8, 4, 4), [255, 0, 0, 255], 3, "red");

    let green = [0, 255, 0, 255].repeat(4);
    h.renderer_mut()
        .update_sprite_texture(texture, 2, 2, &green)
        .unwrap();
    h.begin_frame();
    h.draw_textured_mesh(texture, &vertices, &indices, transform, BlendMode2d::Normal);
    h.render_frame_blocking();
    near(
        pixel(&h.read_pixels(), 8, 4, 4),
        [0, 255, 0, 255],
        3,
        "green after in-place update",
    );
}

/// A mesh inside an offscreen layer obeys the group opacity, exactly like a
/// sprite (the layered path binds the mesh pipeline per layer format).
#[test]
fn a_mesh_inside_a_layer_gets_the_group_opacity() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[255, 255, 255, 255], TextureOptions2d::pixel_art())
        .unwrap();
    let (vertices, indices) = grid(2, 2, [0.0, 0.0, 1.0, 1.0], |x, y| (x, y));

    h.begin_frame();
    h.push_layer(Layer2d::new(0.5, CompositeMode2d::Normal, 0.0))
        .unwrap();
    h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::ZERO, Vec2::splat(8.0)),
        BlendMode2d::Normal,
    );
    h.pop_layer().unwrap();
    h.render_frame_blocking();
    near(
        pixel(&h.read_pixels(), 8, 4, 4),
        [188, 188, 188, 255],
        3,
        "layer opacity",
    );
}

/// A mesh inside a masked layer is clipped by the mask source.
#[test]
fn a_mesh_inside_a_masked_layer_is_clipped() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(16, 16, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[255, 255, 255, 255], TextureOptions2d::pixel_art())
        .unwrap();
    let (vertices, indices) = grid(2, 2, [0.0, 0.0, 1.0, 1.0], |x, y| (x, y));

    h.begin_frame();
    h.push_masked_layer(
        Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
        &[],
        Mask2d {
            mode: MaskMode2d::Alpha,
            invert: false,
        },
    )
    .unwrap();
    h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::ZERO, Vec2::splat(16.0)),
        BlendMode2d::Normal,
    );
    h.begin_mask().unwrap();
    h.draw_shape(ShapeInstance::rect(
        Vec2::new(-4.0, 0.0),
        Vec2::new(8.0, 16.0),
        [1.0, 1.0, 1.0, 1.0],
    ));
    h.end_mask().unwrap();
    h.pop_layer().unwrap();
    h.render_frame_blocking();
    let pixels = h.read_pixels();
    near(
        pixel(&pixels, 16, 4, 8),
        [255, 255, 255, 255],
        3,
        "inside the mask",
    );
    near(
        pixel(&pixels, 16, 12, 8),
        [0, 0, 0, 255],
        3,
        "outside the mask",
    );
}

/// A mesh uses its per-draw blend mode: adding a green sheet over a red shape
/// produces yellow, exactly like the additive sprite path.
#[test]
fn a_mesh_obeys_its_blend_mode() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(16, 16, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[0, 255, 0, 255], TextureOptions2d::pixel_art())
        .unwrap();
    let (vertices, indices) = grid(2, 2, [0.0, 0.0, 1.0, 1.0], |x, y| (x, y));

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(16.0),
        [1.0, 0.0, 0.0, 1.0],
    ));
    h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::ZERO, Vec2::splat(16.0)),
        BlendMode2d::Add,
    );
    h.render_frame_blocking();
    near(
        pixel(&h.read_pixels(), 16, 8, 8),
        [255, 255, 0, 255],
        3,
        "additive mesh",
    );
}

/// Meshes sharing a texture and blend mode collapse into one draw call, and an
/// empty or degenerate mesh neither draws nor panics.
#[test]
fn empty_and_degenerate_meshes_draw_nothing() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[255, 255, 255, 255], TextureOptions2d::pixel_art())
        .unwrap();
    let transform = model(Vec2::ZERO, Vec2::splat(8.0));

    h.begin_frame();
    assert!(!h.draw_textured_mesh(texture, &[], &[], transform, BlendMode2d::Normal));
    assert!(!h.draw_textured_mesh(texture, &[], &[0, 1, 2], transform, BlendMode2d::Normal));
    let (mut vertices, indices) = grid(2, 2, [0.0, 0.0, 1.0, 1.0], |x, y| (x, y));
    assert!(!h.draw_textured_mesh(texture, &vertices, &[], transform, BlendMode2d::Normal));
    assert!(!h.draw_textured_mesh(
        texture,
        &vertices,
        &[0, 1, 9],
        transform,
        BlendMode2d::Normal
    ));
    // A degenerate (zero-area) triangle is accepted and covers nothing.
    vertices[1].position = vertices[0].position;
    vertices[2].position = vertices[0].position;
    assert!(h.draw_textured_mesh(texture, &vertices, &indices, transform, BlendMode2d::Normal));
    let report = h.render_frame_blocking();
    // Nothing painted: the clear colour survives everywhere.
    let pixels = h.read_pixels();
    assert!(pixels
        .chunks_exact(4)
        .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    assert!(report.draw_calls <= 1);

    // Two undeformed sheets sharing a texture coalesce into one draw call.
    let (vertices, indices) = grid(2, 2, [0.0, 0.0, 1.0, 1.0], |x, y| (x, y));
    h.begin_frame();
    assert!(h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::new(-2.0, 0.0), Vec2::splat(4.0)),
        BlendMode2d::Normal
    ));
    assert!(h.draw_textured_mesh(
        texture,
        &vertices,
        &indices,
        model(Vec2::new(2.0, 0.0), Vec2::splat(4.0)),
        BlendMode2d::Normal
    ));
    let report = h.render_frame_blocking();
    assert_eq!(report.draw_calls, 1, "meshes sharing a texture batch");
}
