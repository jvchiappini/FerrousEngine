//! Headless tests for Pure2D gradient fills (D-0030).
//!
//! A linear ramp is drawn across a rect and read back. The checks are the
//! ones the Motioner golden/banding tests rely on: exact authored sRGB stops,
//! linear-light interpolation between them (the encoded midpoint of red and
//! blue is 188, not 128) and a bounded step between neighbouring pixels.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::Pure2dHarness;
use glam::Vec2;
use wgpu::TextureFormat;

/// Serialises the headless tests: each creates a real device.
static GPU_LOCK: Mutex<()> = Mutex::new(());

const W: u32 = 256;
const H: u32 = 64;

/// Linear-light -> sRGB encoded `0..=1`.
fn srgb_encode(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// A 256 x 64 composition in world pixels, origin at the centre.
fn harness() -> Option<Pure2dHarness> {
    let mut h = Pure2dHarness::new(W, H, TextureFormat::Rgba8UnormSrgb).expect("headless gpu");

    // Two stops at t = 0.25 (red) and t = 0.75 (blue). The gradient runs from
    // x = -128 (t = 0) to x = 128 (t = 1), so the stops land at x = -64 and
    // x = 64 and the whole frame shows the ramp.
    let table: [[f32; 4]; 6] = [
        [0.0, 2.0, 2.0, 0.0],      // header: linear, 2 stops, first at slot 2
        [-128.0, 0.0, 128.0, 0.0], // geometry: start, end
        [1.0, 0.0, 0.0, 1.0],      // stop 0 colour (linear)
        [0.25, 0.0, 0.0, 0.0],     // stop 0 offset
        [0.0, 0.0, 1.0, 1.0],      // stop 1 colour
        [0.75, 0.0, 0.0, 0.0],     // stop 1 offset
    ];
    h.renderer_mut().set_2d_gradient_table(&table);
    h.begin_frame();
    h.draw_shape(
        ShapeInstance::rect(Vec2::ZERO, Vec2::new(W as f32, H as f32), [1.0; 4]).with_gradient(0),
    );
    let _ = h.render_frame();
    Some(h)
}

#[test]
fn linear_gradient_reproduces_stops_and_interpolates_in_linear_light() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(h) = harness() else {
        return;
    };
    let mut pixels = Vec::new();
    h.read_pixels_into(&mut pixels).expect("readback");

    let at = |x: u32, y: u32| -> [u8; 4] {
        let index = ((y * W + x) * 4) as usize;
        pixels[index..index + 4].try_into().unwrap()
    };

    // t = 0.25 is exactly the red stop; t = 0.75 the blue one. Allow one code
    // for dithering and half-pixel sampling.
    let red = at(63, H / 2);
    assert!(
        red[0] >= 252 && red[1] <= 2 && red[2] <= 3,
        "stop 0 must be the authored red, got {red:?}"
    );
    let blue = at(193, H / 2);
    assert!(
        blue[2] >= 252 && blue[0] <= 3 && blue[1] <= 2,
        "stop 1 must be the authored blue, got {blue:?}"
    );

    // Linear-light midpoint of #FF0000 and #0000FF is #BC00BC (188), not
    // the naive 8-bit average (128).
    let middle = at(W / 2, H / 2);
    assert!(
        (i32::from(middle[0]) - 188).abs() <= 3,
        "linear-light midpoint red channel is {}, expected 188",
        middle[0]
    );
    assert!(
        (i32::from(middle[2]) - 188).abs() <= 3,
        "linear-light midpoint blue channel is {}, expected 188",
        middle[2]
    );
}

#[test]
fn linear_gradient_has_no_visible_banding() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(h) = harness() else {
        return;
    };
    let mut pixels = Vec::new();
    h.read_pixels_into(&mut pixels).expect("readback");
    let y = H / 2;
    let channel = |x: u32| pixels[((y * W + x) * 4) as usize];
    // Skip the anti-aliased silhouette: blending into the black background is
    // a legitimate big step that has nothing to do with the ramp.
    let mut worst = 0u8;
    let mut worst_at = 0u32;
    for x in 5..W - 4 {
        let t = (f64::from(x) + 0.5) / f64::from(W);
        let f = ((t - 0.25) / 0.5).clamp(0.0, 1.0);
        let ideal = 255.0 * srgb_encode(1.0 - f);
        let error = (f64::from(channel(x)) - ideal).abs().round() as u8;
        if error > worst {
            worst = error;
            worst_at = x;
        }
    }
    // Linear-light interpolation plus the sRGB transfer is the ideal; the
    // check is that every pixel stays on that curve (dither and rounding add
    // at most a couple of codes). A plateau followed by a jump shows up as a
    // large deviation.
    assert!(
        worst <= 3,
        "pixel {worst_at} is {worst} code(s) off the linear-light ramp"
    );
}
