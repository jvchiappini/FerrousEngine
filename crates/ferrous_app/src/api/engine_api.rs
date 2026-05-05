use std::collections::HashMap;
use ferrous_core::api::types::NodeId;
use ferrous_ecs::prelude::Entity;
use ferrous_core::{World, Transform};
use ferrous_core::scene::Material;
use ferrous_core::glam;

/// Public Facade [ENG-API-01, 02]
/// 
/// Interfaz para programar la lógica o integradores del motor sin conocer `wgpu` o `ferrous_ecs`.
pub struct EngineApi<'a> {
    pub world: &'a mut World,
    // Este mapa persistiría en realidad dentro del Runner o del mismo World.
    // Usaremos una abstracción donde la API recibe temporalmente el mapeo (o lo tiene incorporado).
    pub node_map: &'a mut HashMap<NodeId, Entity>,
    pub next_id: &'a mut u64,
}

impl<'a> EngineApi<'a> {
    fn generate_id(&mut self) -> NodeId {
        let id = NodeId(*self.next_id);
        *self.next_id += 1;
        id
    }

    /// Spawns a 3D Mesh into the Engine via the public API barrier.
    /// `mesh` would typically be an abstract Builder representation, here simplified as string identifier or handle.
    pub fn spawn_3d_mesh(&mut self, _mesh_name: &str, transform: Transform) -> NodeId {
        let id = self.generate_id();
        let entity = self.world.ecs.spawn((transform,));
        self.node_map.insert(id, entity);
        id
    }

    /// Spawns a generic 2D path sequence.
    pub fn spawn_2d_path(&mut self, _path: &str, transform: Transform) -> NodeId {
        let id = self.generate_id();
        let entity = self.world.ecs.spawn((transform,));
        self.node_map.insert(id, entity);
        id
    }

    /// Update the transform of an existing entity transparently.
    pub fn update_transform(&mut self, node: NodeId, new_transform: Transform) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(t) = self.world.ecs.get_mut::<Transform>(*entity) {
                *t = new_transform;
                return Ok(());
            }
        }
        Err(())
    }

    /// Hot-reloads the color.
    pub fn update_color(&mut self, node: NodeId, new_color: [f32; 4]) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mat) = self.world.ecs.get_mut::<Material>(*entity) {
                mat.base_color = new_color.into();
                return Ok(());
            }
        }
        Err(())
    }

    /// Despawns and cleans up a node safely.
    pub fn remove_node(&mut self, node: NodeId) -> Result<(), ()> {
        if let Some(entity) = self.node_map.remove(&node) {
            let _ = self.world.ecs.despawn(entity);
            return Ok(());
        }
        Err(())
    }

    pub fn set_global_light(&mut self, _dir: glam::Vec3, _color: [f32; 4]) {
        // En un caso real modificaría el recurso pertinente `DirectionalLight` o `WorldPass` config.
    }

    /// Toggles shadow casting for an entity.
    pub fn set_shadow_caster(&mut self, node: NodeId, casts_shadows: bool) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if casts_shadows {
                self.world.ecs.insert(*entity, ferrous_core::scene::ShadowCaster);
            } else {
                self.world.ecs.remove::<ferrous_core::scene::ShadowCaster>(*entity);
            }
            return Ok(());
        }
        Err(())
    }

    /// Adds a billboard constraint to an entity.
    pub fn add_billboard_constraint(&mut self, node: NodeId, mode: ferrous_core::scene::BillboardMode) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            self.world.ecs.insert(*entity, ferrous_core::scene::Billboard { mode });
            return Ok(());
        }
        Err(())
    }
}
