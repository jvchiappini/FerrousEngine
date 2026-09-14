//! Headless colour-space evidence for the 2D audit item **H1** (#12).
//!
//! The FerrousEngine swapchain was changed to prefer an `*Srgb` surface format
//! so wgpu applies the sRGB output encode. Before that change the native
//! swapchain was non-sRGB and everything rendered ~59 levels darker for a
//! mid-grey. This example renders the **same representative CAD floor-plan
//! scene** twice, off-screen, with the real Pure2D render path:
//!
//! * `..._before_rgba8unorm.png` — renders into a non-sRGB target, which is
//!   exactly what the pre-fix production swapchain did. This is the "before".
//! * `..._after_rgba8unorm_srgb.png` — renders into an sRGB target, matching
//!   post-fix production. This is the "after".
//!
//! Because the engine is already migrated, the "before" is reproduced by the
//! target format rather than by reverting the engine. Both images are the same
//! scene and differ only by the output encode.
//!
//! Run from the engine repository root:
//!
//! ```text
//! export CARGO_TARGET_DIR=<shared target dir>            # this VPS
//! export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export WGPU_BACKEND=vulkan
//! cargo run -p ferrous_renderer --example cad_capture
//! ```
//!
//! Output PNGs are written to `brain/`. This is evidence for human review; no
//! absolute-FPS claim is made anywhere.

use std::fs::File;
use std::io::BufWriter;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::Pure2dHarness;
use glam::Vec2;
use wgpu::TextureFormat;

const W: u32 = 960;
const H: u32 = 540;

/// Mid-grey used by the walls: `0.5` linear is the canonical probe value for
/// the sRGB mismatch (`128` pre-fix, `187` post-fix).
const WALL: [f32; 4] = [0.50, 0.50, 0.50, 1.0];
const FLOOR: [f32; 4] = [0.30, 0.32, 0.35, 1.0];
const ACCENT: [f32; 4] = [0.20, 0.60, 1.00, 1.0];
const COLUMN: [f32; 4] = [0.85, 0.85, 0.88, 1.0];
const ALERT: [f32; 4] = [0.90, 0.30, 0.25, 1.0];

/// A representative architectural floor plan, built only from the public
/// immediate-mode shape API that Koncreta's CAD viewport uses.
fn draw_plan(h: &mut Pure2dHarness) {
    // 1. Room slab: Koncreta fills rectangles via a full-thickness outline
    //    (`fill_thickness = max(w, d)`), which is what this reproduces.
    h.draw_shape(ShapeInstance::rect_outline(
        Vec2::ZERO,
        Vec2::new(700.0, 400.0),
        700.0,
        FLOOR,
    ));

    // 2. Outer walls: thick, flat-capped segments so corners meet squarely
    //    (the CAD "union" cap style).
    let (l, r, b, t) = (-350.0, 350.0, -200.0, 200.0);
    let wall = 10.0;
    for (from, to) in [
        (Vec2::new(l, b), Vec2::new(r, b)),
        (Vec2::new(r, b), Vec2::new(r, t)),
        (Vec2::new(r, t), Vec2::new(l, t)),
        (Vec2::new(l, t), Vec2::new(l, b)),
    ] {
        h.draw_shape(ShapeInstance::line_with_cap_style(
            from,
            to,
            wall,
            WALL,
            ferrous_2d::CapStyle::Flat,
        ));
    }

    // 3. Interior partition with a door gap.
    let part_x = -80.0;
    h.draw_shape(ShapeInstance::line_with_cap_style(
        Vec2::new(part_x, b),
        Vec2::new(part_x, -30.0),
        wall * 0.6,
        WALL,
        ferrous_2d::CapStyle::Flat,
    ));
    h.draw_shape(ShapeInstance::line_with_cap_style(
        Vec2::new(part_x, 70.0),
        Vec2::new(part_x, t),
        wall * 0.6,
        WALL,
        ferrous_2d::CapStyle::Flat,
    ));

    // 4. Structural columns on a grid.
    for gx in [-280.0, -80.0, 160.0, 300.0] {
        for gy in [-120.0, 60.0] {
            h.draw_shape(ShapeInstance::circle(Vec2::new(gx, gy), 14.0, COLUMN));
        }
    }

    // 5. Dimension lines (thin accent) and a red alert marker.
    h.draw_shape(ShapeInstance::line_with_cap_style(
        Vec2::new(l, -230.0),
        Vec2::new(r, -230.0),
        1.5,
        ACCENT,
        ferrous_2d::CapStyle::Flat,
    ));
    h.draw_shape(ShapeInstance::line_with_cap_style(
        Vec2::new(l, -240.0),
        Vec2::new(l, -220.0),
        1.5,
        ACCENT,
        ferrous_2d::CapStyle::Flat,
    ));
    h.draw_shape(ShapeInstance::line_with_cap_style(
        Vec2::new(r, -240.0),
        Vec2::new(r, -220.0),
        1.5,
        ACCENT,
        ferrous_2d::CapStyle::Flat,
    ));
    h.draw_shape(ShapeInstance::circle(Vec2::new(300.0, 60.0), 9.0, ALERT));

    // 6. A deliberate 0.5-grey probe patch at a known location.
    h.draw_shape(ShapeInstance::rect(
        Vec2::new(-200.0, 130.0),
        Vec2::new(60.0, 30.0),
        WALL,
    ));
}

fn write_png(path: &str, pixels: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("create {path}: {e}"))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(pixels).map_err(|e| e.to_string())?;
    Ok(())
}

fn render_capture(format: TextureFormat, path: &str) -> Result<[u8; 4], String> {
    let mut h = Pure2dHarness::new(W, H, format)?;
    h.begin_frame();
    draw_plan(&mut h);
    let report = h.render_frame();
    let pixels = h.read_pixels();
    write_png(path, &pixels)?;
    // Probe the centre of the 0.5-grey patch: world (-200, 130). Pure2D maps
    // 1 world unit to 1 pixel, origin at screen centre, Y-up. Pixel row 0 is
    // the top of the image, so image-y = H/2 - world_y.
    let px = (W as f32 / 2.0 + (-200.0)) as u32;
    let py = (H as f32 / 2.0 - 130.0) as u32;
    let probe = h.read_pixel(px, py);
    println!(
        "[H1] {:?}: draw_calls={} bytes={} probe@({px},{py})={:?} -> {path}",
        format, report.draw_calls, report.bytes_uploaded, probe
    );
    Ok(probe)
}

fn main() -> Result<(), String> {
    std::fs::create_dir_all("brain").map_err(|e| format!("mkdir brain: {e}"))?;

    let before = render_capture(
        TextureFormat::Rgba8Unorm,
        "brain/2d_h1_cad_before_rgba8unorm.png",
    )?;
    let after = render_capture(
        TextureFormat::Rgba8UnormSrgb,
        "brain/2d_h1_cad_after_rgba8unorm_srgb.png",
    )?;

    println!(
        "[H1] summary: mid-grey 0.5 -> before(non-sRGB)={:?} after(sRGB)={:?}",
        before, after
    );
    Ok(())
}
