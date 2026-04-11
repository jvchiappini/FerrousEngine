//! GPU Particle System Components

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ecs")]
use ferrous_ecs::prelude::Component;

/// Configuration for a GPU particle emitter.
///
/// Particles and their lifecycles are managed entirely on the GPU. This
/// component serves as a description/driver for the compute shaders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ecs", derive(Component))]
pub struct ParticleEmitter {
    /// Maximum number of active particles.
    pub max_particles: u32,
    /// Rate at which particles are spawned (particles per second).
    pub spawn_rate: f32,
    
    /// Global color for all particles from this emitter (can be modulated by life).
    pub color: [f32; 3],
    /// Starting opacity.
    pub opacity: f32,
    
    /// Particle size.
    pub size: f32,
    /// Particle lifetime in seconds.
    pub lifetime: f32,
    
    /// Initial velocity direction/spread.
    pub initial_velocity: [f32; 3],
    /// Random spread applied to initial velocity.
    pub velocity_randomness: f32,
    
    /// External force applied every frame (e.g. gravity).
    pub gravity: [f32; 3],
    
    /// Is the emitter currently active?
    pub active: bool,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            max_particles: 10_000,
            spawn_rate: 100.0,
            color: [1.0, 0.5, 0.2], // Fire-ish
            opacity: 1.0,
            size: 0.1,
            lifetime: 2.0,
            initial_velocity: [0.0, 2.0, 0.0],
            velocity_randomness: 0.5,
            gravity: [0.0, -9.81, 0.0],
            active: true,
        }
    }
}

impl ParticleEmitter {
    pub fn fire(max: u32) -> Self {
        Self {
            max_particles: max,
            spawn_rate: (max as f32) / 2.0,
            color: [1.0, 0.4, 0.1],
            size: 0.2,
            lifetime: 1.5,
            initial_velocity: [0.0, 3.0, 0.0],
            velocity_randomness: 1.0,
            ..Default::default()
        }
    }
    
    pub fn snow(max: u32) -> Self {
        Self {
            max_particles: max,
            spawn_rate: (max as f32) / 5.0,
            color: [0.9, 0.9, 1.0],
            size: 0.05,
            lifetime: 5.0,
            initial_velocity: [0.0, -1.0, 0.0],
            velocity_randomness: 2.0,
            gravity: [0.0, -0.5, 0.0],
            ..Default::default()
        }
    }
}
