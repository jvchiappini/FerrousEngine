//! Overlapping N-slot readback ring tests.
//!
//! Every test creates a windowless Pure2D harness.  When no GPU adapter is
//! available the test prints the reason and returns early instead of being
//! `#[ignore]`d, matching the pattern used by `headless_readback.rs`.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::{Pure2dHarness, ReadbackError};
use glam::Vec2;
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const WIDTH: u32 = 32;
const HEIGHT: u32 = 32;

const COLORS: [[f32; 4]; 10] = [
    [1.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0, 1.0],
    [1.0, 0.0, 1.0, 1.0],
    [1.0, 1.0, 1.0, 1.0],
    [0.5, 0.0, 0.0, 1.0],
    [0.0, 0.5, 0.0, 1.0],
    [0.0, 0.0, 0.5, 1.0],
];

const EXPECTED: [[u8; 3]; 10] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
    [255, 0, 255],
    [255, 255, 255],
    [128, 0, 0],
    [0, 128, 0],
    [0, 0, 128],
];

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn assert_rgb_near(actual: [u8; 4], expected: [u8; 3], what: &str) {
    for (channel, (&actual, &expected)) in actual[..3].iter().zip(expected.iter()).enumerate() {
        assert!(
            actual.abs_diff(expected) <= 4,
            "{what}: channel {channel} = {actual}, expected {expected} (full pixel {actual:?})"
        );
    }
}

fn draw_solid(h: &mut Pure2dHarness, color: [f32; 4]) {
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(64.0), color));
    h.render_frame();
}

#[test]
fn ring_sizes_2_3_and_4_return_frames_in_submission_order() {
    let _guard = GPU_LOCK.lock().unwrap();
    for slots in [2usize, 3, 4] {
        let mut h = match Pure2dHarness::with_readback_ring(
            WIDTH,
            HEIGHT,
            TextureFormat::Rgba8Unorm,
            slots,
        ) {
            Ok(h) => h,
            Err(err) => {
                println!("[skip] headless GPU unavailable: {err}");
                return;
            }
        };

        let mut frame = Vec::new();
        let mut collected: Vec<(u64, [u8; 4])> = Vec::new();

        for color in &COLORS {
            draw_solid(&mut h, *color);
            if let Some(frame_index) = h
                .readback_frame(&mut frame)
                .expect("readback_frame must not fail")
            {
                collected.push((frame_index, at(&frame, WIDTH, WIDTH / 2, HEIGHT / 2)));
            }
        }
        h.flush_readbacks(|frame_index, bytes| {
            collected.push((frame_index, at(bytes, WIDTH, WIDTH / 2, HEIGHT / 2)));
        })
        .expect("flush must succeed");

        assert_eq!(
            collected.len(),
            COLORS.len(),
            "slots={slots}: every submitted frame must come back exactly once"
        );
        for (expected_index, (frame_index, pixel)) in collected.iter().enumerate() {
            assert_eq!(
                *frame_index, expected_index as u64,
                "slots={slots}: frames must come out in submission order"
            );
            assert_rgb_near(
                *pixel,
                EXPECTED[expected_index],
                &format!("slots={slots} frame={expected_index}"),
            );
        }
        assert_eq!(
            h.pending_readbacks(),
            0,
            "slots={slots}: flushing must leave no pending frame"
        );
    }
}

#[test]
fn busy_when_every_slot_is_in_flight_changes_nothing() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = match Pure2dHarness::with_readback_ring(WIDTH, HEIGHT, TextureFormat::Rgba8Unorm, 2)
    {
        Ok(h) => h,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };
    draw_solid(&mut h, COLORS[0]);
    assert_eq!(h.submit_readback().expect("first submit"), 0);
    assert_eq!(h.submit_readback().expect("second submit"), 1);

    let pending = h.pending_readbacks();
    let next = h.next_frame_index();
    let allocated = h.readback_slots_allocated();
    assert_eq!(
        h.submit_readback(),
        Err(ReadbackError::Busy),
        "a full ring must report Busy"
    );
    assert_eq!(h.pending_readbacks(), pending, "Busy must not enqueue");
    assert_eq!(
        h.next_frame_index(),
        next,
        "Busy must not consume a frame index"
    );
    assert_eq!(
        h.readback_slots_allocated(),
        allocated,
        "Busy must not allocate a slot"
    );

    let mut out = Vec::new();
    while h.take_readback(&mut out).expect("drain ring") {}
}

/// Regression: one failed map must not poison the ring.
///
/// Before the recovery fix, `take_readback` called `wait_for_map(rx)?` *before*
/// `pop_front()` and before unmapping, so a failed map left the same pending
/// entry at the front forever: every subsequent take re-failed the same frame
/// and no slot was ever freed.  This test injects one failure and then requires
/// the next take to return frame 1, `pending_readbacks()` to reach 0, and a
/// further 20-frame run to finish without a leak.
#[test]
fn injected_map_failure_is_reported_once_and_the_ring_recovers() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = match Pure2dHarness::with_readback_ring(WIDTH, HEIGHT, TextureFormat::Rgba8Unorm, 3)
    {
        Ok(h) => h,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };

    draw_solid(&mut h, COLORS[0]);
    assert_eq!(h.submit_readback().expect("submit frame 0"), 0);
    draw_solid(&mut h, COLORS[1]);
    assert_eq!(h.submit_readback().expect("submit frame 1"), 1);

    // The failure is injected after both copies are in flight, so it must hit
    // exactly the oldest frame and leave frame 1 alone.
    h.inject_map_failure_for_test();
    let mut out = Vec::new();
    let err = h
        .take_readback(&mut out)
        .expect_err("the injected map must fail once");
    assert_eq!(
        err,
        ReadbackError::MapFailed {
            message: "injected map failure (test only)".to_string(),
            frame_index: Some(0),
        },
        "the error must carry the affected frame index"
    );
    assert_eq!(h.pending_readbacks(), 1, "only the failed frame is removed");

    assert!(h.take_readback(&mut out).expect("frame 1 still works"));
    assert_rgb_near(
        at(&out, WIDTH, WIDTH / 2, HEIGHT / 2),
        EXPECTED[1],
        "frame after the failure",
    );
    assert_eq!(h.pending_readbacks(), 0);

    // No slot leak: run 20 more frames through the same ring.
    let mut last_index = None;
    for i in 0..20usize {
        draw_solid(&mut h, COLORS[i % COLORS.len()]);
        let taken = h.readback_frame(&mut out).expect("recovered ring");
        if let Some(index) = taken {
            if let Some(previous) = last_index {
                assert_eq!(index, previous + 1, "indices stay contiguous");
            }
            last_index = Some(index);
        }
    }
    h.flush_readbacks(|_, _| {}).expect("flush recovered ring");
    assert_eq!(
        h.pending_readbacks(),
        0,
        "twenty frames must not leak a readback slot"
    );
}

#[test]
fn submit_and_non_blocking_render_never_poll_but_blocking_render_does() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = match Pure2dHarness::with_readback_ring(WIDTH, HEIGHT, TextureFormat::Rgba8Unorm, 3)
    {
        Ok(h) => h,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };
    draw_solid(&mut h, COLORS[0]);

    let before = h.blocking_polls();
    h.submit_readback().expect("submit must not block");
    assert_eq!(
        h.blocking_polls(),
        before,
        "submit_readback must only submit + map_async, never poll"
    );

    draw_solid(&mut h, COLORS[1]);
    assert_eq!(
        h.blocking_polls(),
        before,
        "render_frame must not poll Maintain::Wait"
    );

    let mut out = Vec::new();
    assert!(h.take_readback(&mut out).expect("take oldest"));
    assert!(
        h.blocking_polls() > before,
        "taking a frame waits for its submission index"
    );

    let before_blocking = h.blocking_polls();
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(64.0),
        COLORS[2],
    ));
    let _ = h.render_frame_blocking();
    assert!(
        h.blocking_polls() > before_blocking,
        "render_frame_blocking must use Maintain::Wait"
    );
}

#[test]
fn steady_state_ring_creates_no_new_staging_buffers() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = match Pure2dHarness::with_readback_ring(WIDTH, HEIGHT, TextureFormat::Rgba8Unorm, 3)
    {
        Ok(h) => h,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };
    let mut out = Vec::new();

    // Warm up: fill the ring and drain it once.
    for color in COLORS.iter().take(6) {
        draw_solid(&mut h, *color);
        let _ = h.readback_frame(&mut out).expect("warm-up readback");
    }
    h.flush_readbacks(|_, _| {}).expect("warm-up flush");
    let allocated = h.readback_slots_allocated();
    assert_eq!(
        allocated, 3,
        "the default ring allocates exactly its three slots"
    );

    for i in 0..100usize {
        draw_solid(&mut h, COLORS[i % COLORS.len()]);
        let _ = h.readback_frame(&mut out).expect("steady-state readback");
    }
    h.flush_readbacks(|_, _| {}).expect("steady-state flush");

    assert_eq!(
        h.readback_slots_allocated(),
        allocated,
        "100 frames must not create a new staging buffer"
    );
    assert_eq!(h.pending_readbacks(), 0);
}
