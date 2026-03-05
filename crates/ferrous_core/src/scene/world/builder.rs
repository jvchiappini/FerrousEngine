//! Fluent entity builder returned by [`super::World::spawn`].

use glam::{Quat, Vec3};

use crate::color::Color;
use crate::scene::{MaterialDescriptor, MaterialHandle};
use crate::transform::Transform;

use super::scene::World;
use super::types::{Element, ElementKind, Handle, PointLightComponent};

/// Fluent builder returned by [`World::spawn`].
///
/// Call `.build()` to insert the entity and receive its [`Handle`].
pub struct EntityBuilder<'a> {
    pub(super) world: &'a mut World,
    pub(super) element: Element,
}

impl<'a> EntityBuilder<'a> {
    pub(super) fn new(world: &'a mut World, id: u64, name: impl Into<String>) -> Self {
        Self {
            world,
            element: Element::new(id, name),
        }
    }

    pub fn with_position(mut self, pos: Vec3) -> Self {
        self.element.transform.position = pos;
        self
    }

    pub fn with_rotation(mut self, rot: Quat) -> Self {
        self.element.transform.rotation = rot;
        self
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.element.transform.scale = scale;
        self
    }

    pub fn with_transform(mut self, t: Transform) -> Self {
        self.element.transform = t;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.element.material.descriptor.base_color = color.to_array();
        self
    }

    pub fn with_kind(mut self, kind: ElementKind) -> Self {
        self.element.kind = kind;
        self
    }

    pub fn with_material(mut self, desc: MaterialDescriptor) -> Self {
        self.element.material.descriptor = desc;
        self
    }

    pub fn with_material_handle(mut self, handle: MaterialHandle) -> Self {
        self.element.material.handle = handle;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.element.tags.push(tag.into());
        self
    }

    pub fn with_point_light(mut self, comp: PointLightComponent) -> Self {
        self.element.point_light = Some(comp);
        self
    }

    pub fn invisible(mut self) -> Self {
        self.element.visible = false;
        self
    }

    /// Finalise the builder, insert the entity, and return its [`Handle`].
    pub fn build(self) -> Handle {
        let id = self.element.id;

        let entity = self.world.ecs.spawn((
            self.element.transform,
            self.element.material.clone(),
            self.element.clone(),
        ));

        if let Some(pl) = self.element.point_light {
            self.world.ecs.insert(entity, pl);
        }

        let idx = id as usize;
        if idx >= self.world.entities.len() {
            self.world.entities.resize(idx + 1, None);
        }
        self.world.entities[idx] = Some(self.element);
        self.world.ecs_mapping.insert(id, entity);
        self.world.count += 1;
        Handle(id)
    }
}
