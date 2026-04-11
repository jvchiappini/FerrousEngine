//! Skeletal Animation (Skinning) components.

use serde::{Deserialize, Serialize};
use glam::{Mat4, Vec3, Quat};
use bytemuck::{Pod, Zeroable};

#[cfg(feature = "ecs")]
use ferrous_ecs::prelude::Component;

/// Maximum number of bones per skeleton (GPU limit optimization).
pub const MAX_BONES: usize = 128;

/// A single bone in a skeletal hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub parent_index: Option<usize>,
    /// Matrix that transforms from mesh-space to bone-space in bind pose.
    pub inverse_bind_matrix: Mat4,
    /// Current local transform relative to parent.
    pub local_transform: Transform,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// Skeleton component holding a hierarchy of bones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
    /// Pre-computed world-space bone matrices (mesh-space).
    /// Sent to GPU every frame.
    #[serde(skip)]
    pub joint_matrices: Vec<Mat4>,
}

impl Skeleton {
    pub fn new(bones: Vec<Bone>) -> Self {
        let count = bones.len();
        Self {
            bones,
            joint_matrices: vec![Mat4::IDENTITY; count],
        }
    }

    /// Update `joint_matrices` based on current local transforms.
    pub fn update_matrices(&mut self) {
        let mut world_transforms = vec![Mat4::IDENTITY; self.bones.len()];
        
        for i in 0..self.bones.len() {
            let local_mat = Mat4::from_scale_rotation_translation(
                self.bones[i].local_transform.scale,
                self.bones[i].local_transform.rotation,
                self.bones[i].local_transform.position,
            );

            if let Some(parent_idx) = self.bones[i].parent_index {
                world_transforms[i] = world_transforms[parent_idx] * local_mat;
            } else {
                world_transforms[i] = local_mat;
            }

            self.joint_matrices[i] = world_transforms[i] * self.bones[i].inverse_bind_matrix;
        }
    }
}

#[cfg(feature = "ecs")]
impl Component for Skeleton {}

/// Vertex influence for skinning.
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Pod, Zeroable)]
pub struct BoneInfluence {
    pub indices: [u32; 4],
    pub weights: [f32; 4],
}

/// Component for an entity that has a skinned mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinnedMesh {
    /// Reference to the skeleton entity.
    pub skeleton_entity: Option<ferrous_ecs::entity::Entity>,
}

#[cfg(feature = "ecs")]
impl Component for SkinnedMesh {}
