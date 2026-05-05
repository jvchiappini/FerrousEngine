use ferrous_ecs::query::Query;
use glam::{Mat4, Vec3};
use crate::components::{Sprite, Transform2d};
use crate::render::{SpriteBatcher, SpriteInstance};

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
        });
    }
}

/// System to render complex Path2d components as a series of perfectly joined SDF segments.
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
        if path.commands.len() < 2 { continue; }

        let mut cursor = glam::Vec2::ZERO;

        for cmd in &path.commands {
            match cmd {
                crate::components::PathCommand::MoveTo(pos) => {
                    cursor = *pos;
                }
                crate::components::PathCommand::LineTo(target) => {
                    let start = cursor;
                    let end = *target;
                    let delta = end - start;
                    let length = delta.length();
                    
                    if length > 0.0001 {
                        let center = (start + end) * 0.5;
                        let rotation = delta.y.atan2(delta.x);
                        
                        // We use a small overlap for connectivity
                        let overlap = path.stroke_width * 0.1;
                        
                        let model = Mat4::from_scale_rotation_translation(
                            Vec3::new(transform.scale.x * (length + overlap * 2.0), transform.scale.y * path.stroke_width, 1.0),
                            glam::Quat::from_rotation_z(transform.rotation + rotation),
                            Vec3::new(
                                transform.position.x + (center.x * transform.scale.x), 
                                transform.position.y + (center.y * transform.scale.y), 
                                transform.z_index
                            ),
                        );

                        let radius = if path.cap_style == crate::components::CapStyle::Round {
                            path.stroke_width * 0.5
                        } else {
                            0.0
                        };

                        batcher.push_shape(crate::render::types::ShapeInstance {
                            transform_c0: model.x_axis.into(),
                            transform_c1: model.y_axis.into(),
                            transform_c2: model.z_axis.into(),
                            transform_c3: model.w_axis.into(),
                            color: path.stroke_color,
                            params: [
                                path.stroke_width, 
                                radius,               // radius support for caps
                                1.0,               // smoothing
                                1.0,               // is_filled = true
                            ],
                        });
                    }
                    cursor = end;
                }
            }
        }
    }
}


