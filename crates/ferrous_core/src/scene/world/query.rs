//! Query, access and renderer-bridge methods for [`World`].
//!
//! Material, visibility, tags, raw element access, iteration and the renderer
//! handle slot all live here.  Core mutation (spawn/despawn/transform) lives
//! in [`super::scene`].

use crate::color::Color;
use crate::scene::{MaterialDescriptor, MaterialHandle};

use super::types::{Element, ElementKind, Handle, MaterialComponent};
use super::scene::World;

impl World {
    // ── Material ───────────────────────────────────────────────────────────

    /// Replace the material descriptor stored on an entity.
    pub fn set_material_descriptor(&mut self, handle: Handle, desc: MaterialDescriptor) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.material.descriptor = desc.clone();
            if let Some(entity) = self.ecs_mapping.get(&handle.0) {
                if let Some(m) = self.ecs.get_mut::<MaterialComponent>(*entity) {
                    m.descriptor = desc.clone();
                }
                if let Some(elem) = self.ecs.get_mut::<Element>(*entity) {
                    elem.material.descriptor = desc;
                }
            }
        }
    }

    /// Set the material handle for an entity.
    pub fn set_material_handle(&mut self, handle: Handle, mat: MaterialHandle) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.material.handle = mat;
            if let Some(entity) = self.ecs_mapping.get(&handle.0) {
                if let Some(m) = self.ecs.get_mut::<MaterialComponent>(*entity) {
                    m.handle = mat;
                }
                if let Some(elem) = self.ecs.get_mut::<Element>(*entity) {
                    elem.material.handle = mat;
                }
            }
        }
    }

    /// Convenience: tint the object by changing the base colour on its descriptor.
    pub fn set_color(&mut self, handle: Handle, color: Color) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.material.descriptor.base_color = color.to_array();
        }
    }

    // ── Visibility ──────────────────────────────────────────────────────────

    pub fn set_visible(&mut self, handle: Handle, visible: bool) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.visible = visible;
        }
    }

    // ── Tags ────────────────────────────────────────────────────────────────

    /// Add a string tag to an entity.
    pub fn add_tag(&mut self, handle: Handle, tag: impl Into<String>) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            let tag = tag.into();
            if !e.tags.contains(&tag) {
                e.tags.push(tag);
            }
        }
    }

    /// Returns `true` if the entity has the given tag.
    pub fn has_tag(&self, handle: Handle, tag: &str) -> bool {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .map(|e| e.tags.iter().any(|t| t == tag))
            .unwrap_or(false)
    }

    // ── Raw element access ──────────────────────────────────────────────────

    /// Immutable reference to an entity.
    pub fn get(&self, handle: Handle) -> Option<&Element> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
    }

    /// Mutable reference to an entity.
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut Element> {
        self.entities
            .get_mut(handle.0 as usize)
            .and_then(|o| o.as_mut())
    }

    /// Returns `true` if the world contains this handle.
    pub fn contains(&self, handle: Handle) -> bool {
        let idx = handle.0 as usize;
        idx < self.entities.len() && self.entities[idx].is_some()
    }

    // ── Iteration ───────────────────────────────────────────────────────────

    /// Iterate over all entities.
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.entities.iter().filter_map(|o| o.as_ref())
    }

    /// Mutably iterate over all entities.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.entities.iter_mut().filter_map(|o| o.as_mut())
    }

    /// Iterate over all entities that carry the given tag.
    pub fn iter_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Element> {
        self.iter().filter(move |e| e.tags.iter().any(|t| t == tag))
    }

    /// Iterate over `(Handle, &Element)` pairs.
    pub fn iter_with_handles(&self) -> impl Iterator<Item = (Handle, &Element)> {
        self.entities
            .iter()
            .enumerate()
            .filter_map(|(id, o)| o.as_ref().map(|e| (Handle(id as u64), e)))
    }

    /// Total number of entities currently alive.
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns the capacity of the underlying storage.
    pub fn capacity(&self) -> usize {
        self.entities.len()
    }

    // ── Renderer bridge ─────────────────────────────────────────────────────

    /// Internal: set the renderer handle for an entity.
    pub fn set_render_handle(&mut self, handle: Handle, rh: usize) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.render_handle = Some(rh);
        }
    }

    /// Internal: retrieve the renderer handle for an entity.
    pub fn render_handle(&self, handle: Handle) -> Option<usize> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .and_then(|e| e.render_handle)
    }
}
