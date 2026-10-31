use ferrous_ecs::query::Query;
use glam::{Mat4, Vec3};
use crate::components::{Sprite, Transform2d};
use crate::render::{SpriteBatcher, SpriteInstance};

/// Clears the sprite batcher and re-fills it from every `(&Transform2d, &Sprite)`
/// entity, sorted by ascending [`Transform2d::z_index`](crate::components::Transform2d::z_index).
///
/// **Deprecated / dead code.** This function is not registered in the engine's
/// `StagedScheduler` (nor in any other scheduler), so spawning `Sprite`
/// components renders nothing. The live 2D path is immediate-mode
/// (`ferrous_renderer::Renderer::draw_sprite_2d`). It is kept only for API
/// compatibility and will be removed in a future release; do not adopt it.
///
/// Migration: replace `ferrous_2d::prepare_sprites_system(&mut batcher, query)`
/// with `Renderer::draw_sprite_2d(...)` calls in your draw pass.
#[deprecated(
    since = "0.1.0",
    note = "not registered in any scheduler; use the immediate-mode Renderer::draw_sprite_2d path instead"
)]
pub fn prepare_sprites_system(
    batcher: &mut SpriteBatcher,
    query: Query<(&Transform2d, &Sprite)>,
) {
    batcher.clear();

    let mut renderables: Vec<(&Transform2d, &Sprite)> = query.iter().map(|(_, comps)| comps).collect();

    // Sort by Z index ascending for alpha blending
    renderables.sort_by(|(t_a, _), (t_b, _)| {
        t_a.z_index.partial_cmp(&t_b.z_index).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (transform, sprite) in renderables {
        let tex_id = sprite.texture_id.unwrap_or(0);
        let size = sprite.custom_size.unwrap_or(glam::Vec2::new(100.0, 100.0));

        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(transform.scale.x * size.x, transform.scale.y * size.y, 1.0),
            glam::Quat::from_rotation_z(transform.rotation),
            Vec3::new(transform.position.x, transform.position.y, transform.z_index),
        );

        let uv_r = sprite.rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let props = [if sprite.flip_x{1.0}else{0.0}, if sprite.flip_y{1.0}else{0.0}, 1.0, 0.0];
        
        batcher.push_sprite(tex_id, SpriteInstance {
            transform_c0: model.x_axis.into(),
            transform_c1: model.y_axis.into(),
            transform_c2: model.z_axis.into(),
            transform_c3: model.w_axis.into(),
            color: sprite.color.into(),
            uv_rect: uv_r,
            properties: props,
        });
    }
}

/// Clears the shape batcher and re-fills it from every `(&Transform2d, &Shape2d)`
/// entity, sorted by ascending [`Transform2d::z_index`](crate::components::Transform2d::z_index).
///
/// **Deprecated / dead code.** This function is not registered in the engine's
/// `StagedScheduler` (nor in any other scheduler), so spawning `Shape2d`
/// components renders nothing. The live 2D path is immediate-mode
/// (`ferrous_renderer::Renderer::draw_2d_shape`). It is kept only for API
/// compatibility and will be removed in a future release; do not adopt it.
///
/// Migration: replace `ferrous_2d::prepare_shapes_system(&mut batcher, query)`
/// with `Renderer::draw_2d_shape(...)` calls in your draw pass.
#[deprecated(
    since = "0.1.0",
    note = "not registered in any scheduler; use the immediate-mode Renderer::draw_2d_shape path instead"
)]
pub fn prepare_shapes_system(
    batcher: &mut crate::render::ShapeBatcher,
    query: Query<(&Transform2d, &crate::components::Shape2d)>,
) {
    batcher.clear();

    let mut renderables: Vec<(&Transform2d, &crate::components::Shape2d)> = query.iter().map(|(_, comps)| comps).collect();

    // Sort by Z index ascending for correct alpha blending
    renderables.sort_by(|(t_a, _), (t_b, _)| {
        t_a.z_index.partial_cmp(&t_b.z_index).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (transform, shape) in renderables {
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(transform.scale.x * shape.size.x, transform.scale.y * shape.size.y, 1.0),
            glam::Quat::from_rotation_z(transform.rotation),
            Vec3::new(transform.position.x, transform.position.y, transform.z_index),
        );

        batcher.push_shape(crate::render::types::ShapeInstance {
            transform_c0: model.x_axis.into(),
            transform_c1: model.y_axis.into(),
            transform_c2: model.z_axis.into(),
            transform_c3: model.w_axis.into(),
            color: shape.color.into(),
            params: [
                shape.border_thickness,
                shape.corner_radius,
                shape.smoothing,
                if shape.is_filled { 1.0 } else { 0.0 },
            ],
            ext0: [0.0; 4],
            ext1: 0.0,
        });
    }
}

/// System to render complex Path2d components as a series of SDF segments with
/// correct caps and joins.
///
/// ## Caps (Issue #5)
/// Only the first segment of a subpath receives the path's `cap_style` as a
/// *start* cap and only the last segment receives it as an *end* cap:
/// - `Flat`: the quad ends exactly on the vertex.
/// - `Round`: a disc of radius `stroke_width/2` is drawn centred on the outer
///   endpoint (exact round-cap silhouette without extending the quad).
/// - `Square`: the quad is extended `stroke_width/2` beyond the outer endpoint.
/// Intermediate segments always use flat ends — the join covers the corner, not
/// an extra cap.
///
/// ## Joins (Issue #2 — pragmatic round-only approximation)
/// Every interior vertex gets an extra disc (radius `stroke_width/2` for
/// `Round`/`Bevel`, a growing miter-like radius for `Miter`) centred on the
/// vertex.  The old fixed +10% overlap is gone: corners are filled by the join
/// discs, not by overlapping the quads, so acute zigzags no longer show gaps.
/// Exact miter/bevel tessellation is left for a future phase (Issue #2/#10).
///
/// **Deprecated / dead code.** This function is not registered in the engine's
/// `StagedScheduler` (nor in any other scheduler), so spawning `Path2d`
/// components renders nothing. The live 2D path is immediate-mode
/// (`ferrous_renderer::Renderer::draw_2d_path`). It is kept only for API
/// compatibility and will be removed in a future release; do not adopt it.
///
/// Migration: replace `ferrous_2d::prepare_paths_system(&mut batcher, query)`
/// with `Renderer::draw_2d_path(...)` calls in your draw pass.
#[deprecated(
    since = "0.1.0",
    note = "not registered in any scheduler; use the immediate-mode Renderer::draw_2d_path path instead"
)]
pub fn prepare_paths_system(
    batcher: &mut crate::render::ShapeBatcher,
    query: Query<(&Transform2d, &crate::components::Path2d)>,
) {
    // Note: We don't clear the batcher here as it's shared with Shape2d.
    // The Renderer/App should clear it once at the start of the frame.

    let mut renderables: Vec<(&Transform2d, &crate::components::Path2d)> = query.iter().map(|(_, comps)| comps).collect();

    // Sort by Z index
    renderables.sort_by(|(t_a, _), (t_b, _)| {
        t_a.z_index.partial_cmp(&t_b.z_index).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (transform, path) in renderables {
        // Split the command list into polylines (one per subpath).  A `MoveTo`
        // starts a new subpath, which also defines where start/end caps belong.
        let subpaths = extract_subpaths(&path.commands);

        for pts in &subpaths {
            let n = pts.len();
            if n < 2 { continue; }

            let half_w = path.stroke_width * 0.5;

            for i in 0..n - 1 {
                let start = pts[i];
                let end = pts[i + 1];
                let delta = end - start;
                let length = delta.length();

                if length < 0.0001 { continue; }

                let is_first = i == 0;
                let is_last = i == n - 1;
                let dir = delta / length;

                // Only Square caps extend the quad itself (width/2 on the outer
                // end, via an offset quad centre).  Round caps are separate
                // discs; Flat caps end exactly on the vertex.
                let start_ext = if is_first && path.cap_style == crate::components::CapStyle::Square { half_w } else { 0.0 };
                let end_ext = if is_last && path.cap_style == crate::components::CapStyle::Square { half_w } else { 0.0 };

                let quad_u_start = -start_ext;
                let quad_u_end = length + end_ext;
                let geom_length = quad_u_end - quad_u_start;
                // Quad centre shifted so only the capped end protrudes.
                let quad_center = start + dir * ((quad_u_start + quad_u_end) * 0.5);

                let model = Mat4::from_scale_rotation_translation(
                    Vec3::new(
                        transform.scale.x * geom_length,
                        transform.scale.y * path.stroke_width,
                        1.0,
                    ),
                    glam::Quat::from_rotation_z(transform.rotation + delta.y.atan2(delta.x)),
                    Vec3::new(
                        transform.position.x + (quad_center.x * transform.scale.x),
                        transform.position.y + (quad_center.y * transform.scale.y),
                        transform.z_index,
                    ),
                );

                // radius = 0.0: flat ends on every segment (intermediate joins
                // are filled by discs, not by rounded quads).
                batcher.push_shape(crate::render::types::ShapeInstance {
                    transform_c0: model.x_axis.into(),
                    transform_c1: model.y_axis.into(),
                    transform_c2: model.z_axis.into(),
                    transform_c3: model.w_axis.into(),
                    color: path.stroke_color,
                    params: [path.stroke_width, 0.0, 1.0, 1.0],
                    ext0: [0.0; 4],
                    ext1: 0.0,
                });

                // Round caps: a disc on the outer endpoint only.
                if is_first && path.cap_style == crate::components::CapStyle::Round {
                    push_disc(batcher, transform, path, start, half_w);
                }
                if is_last && path.cap_style == crate::components::CapStyle::Round {
                    push_disc(batcher, transform, path, end, half_w);
                }
            }

            // Joins at interior shared vertices (between segment i-1 and i).
            for i in 1..n - 1 {
                let v = pts[i];
                let in_dir = (v - pts[i - 1]).normalize_or_zero();
                let out_dir = (pts[i + 1] - v).normalize_or_zero();

                // Travel-direction angle at the vertex: 0 = straight line.
                let theta = in_dir.dot(out_dir).clamp(-1.0, 1.0).acos();
                // Skip near-straight joints (~2°) to avoid redundant discs.
                if theta < 0.035 { continue; }

                push_disc(batcher, transform, path, v, join_disc_radius(path, theta));
            }
        }
    }
}

/// Splits the raw command list into subpaths (each `MoveTo`+`LineTo`* group).
/// Points that produce no geometry (< 2 points) are dropped.
fn extract_subpaths(
    commands: &[crate::components::PathCommand],
) -> Vec<Vec<glam::Vec2>> {
    let mut subpaths: Vec<Vec<glam::Vec2>> = Vec::new();
    let mut current: Vec<glam::Vec2> = Vec::new();

    for cmd in commands {
        match cmd {
            crate::components::PathCommand::MoveTo(pos) => {
                if current.len() >= 2 {
                    subpaths.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.push(*pos);
            }
            crate::components::PathCommand::LineTo(target) => {
                current.push(*target);
            }
        }
    }
    if current.len() >= 2 {
        subpaths.push(current);
    }
    subpaths
}

/// Radius of the join disc at a vertex with travel-direction angle `theta`
/// (radians), honouring the path's `join_style`:
/// - `Round`/`Bevel`: `stroke_width/2` — the true round-join arc silhouette
///   (used as an approximation for `Bevel` too).
/// - `Miter` (sharp corners only): grows with the classic `1/sin(θ/2)` miter
///   ratio (capped at 4.0) so the disc reaches the miter point.
/// Exact miter/bevel geometry is a future improvement.
fn join_disc_radius(path: &crate::components::Path2d, theta: f32) -> f32 {
    let half_w = path.stroke_width * 0.5;
    match path.join_style {
        crate::components::JoinStyle::Miter => {
            if theta > 30_f32.to_radians() && theta < 150_f32.to_radians() {
                let ratio = (1.0 / (theta * 0.5).sin()).clamp(1.0, 4.0);
                half_w * ratio
            } else {
                half_w
            }
        }
        _ => half_w,
    }
}

/// Pushes a filled disc (radius in path-local units) centred on `center`
/// (path-local coordinates), following the same transform convention as the
/// segment quads (stroke width is scaled by `transform.scale`).
fn push_disc(
    batcher: &mut crate::render::ShapeBatcher,
    transform: &Transform2d,
    path: &crate::components::Path2d,
    center: glam::Vec2,
    radius: f32,
) {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(
            transform.scale.x * radius * 2.0,
            transform.scale.y * radius * 2.0,
            1.0,
        ),
        glam::Quat::IDENTITY,
        Vec3::new(
            transform.position.x + center.x * transform.scale.x,
            transform.position.y + center.y * transform.scale.y,
            transform.z_index,
        ),
    );

    batcher.push_shape(crate::render::types::ShapeInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: path.stroke_color,
        params: [path.stroke_width, radius, 1.0, 1.0],
        ext0: [0.0; 4],
        ext1: 0.0,
    });
}


