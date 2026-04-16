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

    let renderables: Vec<(&Transform2d, &crate::components::Shape2d)> = query.iter().map(|(_, comps)| comps).collect();

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

