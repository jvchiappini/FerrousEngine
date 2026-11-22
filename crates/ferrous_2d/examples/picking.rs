//! Click-to-select, marquee and drag with the engine's scene picking.
//!
//! Run:
//! ```text
//! cargo run -p ferrous_2d --example picking
//! ```
//!
//! The example is headless: it shows the queries a host runs when the pointer
//! moves, a marquee is dragged, or a snap is asked for. The host still draws
//! the scene and the selection; the engine only answers "what is under this
//! point?" with geometry that matches what was drawn.

use ferrous_2d::picking::{Aabb, MarqueeMode, PickCamera, PickHit, PickItem, PickScene, PickShape};
use glam::{Affine2, Vec2};

fn main() {
    let mut scene = PickScene::new();
    // A card, a dot and a rotated star-ish polygon, in world pixels.
    scene.add(
        PickItem::new(1, PickShape::rounded_rect(Vec2::new(240.0, 120.0), 12.0))
            .transform(Affine2::from_translation(Vec2::new(200.0, 150.0))),
    );
    scene.add(
        PickItem::new(2, PickShape::circle(24.0))
            .transform(Affine2::from_translation(Vec2::new(420.0, 120.0))),
    );
    let star = (0..10)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::TAU / 10.0;
            let radius = if step % 2 == 0 { 60.0 } else { 26.0 };
            Vec2::new(angle.cos() * radius, angle.sin() * radius)
        })
        .collect();
    scene.add(
        PickItem::new(3, PickShape::polygon(star))
            .transform(Affine2::from_angle_translation(
                0.35,
                Vec2::new(240.0, 330.0),
            ))
            .z(1),
    );
    scene.rebuild();

    let camera = PickCamera {
        center: Vec2::new(320.0, 240.0),
        zoom: 1.0,
        viewport: Vec2::new(640.0, 480.0),
    };

    // A click: screen -> world -> topmost pick.
    let click = Vec2::new(200.0, 150.0);
    let hit = scene.pick(camera.screen_to_world(click), 2.0);
    println!("click at {click:?} -> {hit:?}");

    // A marquee: screen rectangle -> world rectangle -> topmost-first hits.
    let screen_area = Aabb {
        min: Vec2::new(80.0, 60.0),
        max: Vec2::new(560.0, 420.0),
    };
    let area = Aabb {
        min: camera.screen_to_world(screen_area.min),
        max: camera.screen_to_world(screen_area.max),
    };
    let mut hits: Vec<PickHit> = Vec::new();
    scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut hits);
    println!(
        "marquee -> {:?}",
        hits.iter().map(|hit| hit.id).collect::<Vec<_>>()
    );

    // A drag: the pointer follows the picked item and asks for a snap point.
    if let Some(picked) = hit {
        let grabbed = scene.items()[picked.item].transform;
        let start = camera.screen_to_world(click);
        let now = camera.screen_to_world(click + Vec2::new(30.0, 10.0));
        let moved = Affine2::from_translation(now - start) * grabbed;
        println!(
            "drag item {} to translation {:?}",
            picked.id, moved.translation
        );
        if let Some(snap) = scene.nearest_point(now, 24.0) {
            println!("snap to {:?} at distance {}", snap.point, snap.distance);
        }
    }

    // Zoom-to-cursor keeps the point under the cursor fixed; the picking math
    // is the same one the host uses for its camera.
    let mut zoomed = camera;
    zoomed.zoom_to_cursor(click, 2.0, 0.1, 8.0);
    println!(
        "zoom 2.0 keeps click at {:?}",
        zoomed.screen_to_world(click)
    );
}
