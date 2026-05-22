//! Animation sequencer for complex Jobs.
//! Separates declaration from execution for maximum efficiency.

#![cfg(feature = "ecs")]

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;
use crate::time::Time;
use crate::transform::Transform;
use crate::scene::material::Material;
use crate::scene::world::MaterialComponent;

/// Selects the interpolation math.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EasingType {
    Linear,
    EaseInOutCubic,
    ElasticOut,
}

/// A specific animation task.
#[derive(Debug, Clone)]
pub struct AnimJob {
    pub property: String,
    pub start_val: f32,
    pub end_val: f32,
    pub start_time: f64,
    pub duration: f64,
    pub easing: EasingType,
    // v13: Morphing support
    pub path_start: Option<crate::scene::world::types::PathData>,
    pub path_target: Option<crate::scene::world::types::PathData>,
}

/// Component that holds a list of animation jobs for an entity.
#[derive(Debug, Clone, Default)]
pub struct Animator {
    pub jobs: Vec<AnimJob>,
}

impl Component for Animator {}

/// The Animator system that processes all active animation jobs.
/// This system follows the Sequencer pattern, where jobs are scheduled
/// once and executed efficiently in Rust.
pub struct AnimatorSystem;

impl System for AnimatorSystem {
    fn name(&self) -> &'static str { "AnimatorSystem" }

    fn run(&mut self, world: &mut World, resources: &mut ResourceMap) {
        let current_time = resources.get::<crate::time::Time>()
            .map(|t| t.elapsed)
            .unwrap_or(0.0);

        let entities: Vec<Entity> = world.query::<Animator>().map(|(e, _)| e).collect();

        for entity in entities {
            let mut jobs_to_remove = Vec::new();
            
            // Step 1: Process and apply interpolation
            {
                let mut changes = Vec::new();
                
                if let Some(animator) = world.get::<Animator>(entity) {
                    for (i, job) in animator.jobs.iter().enumerate() {
                        if current_time < job.start_time {
                            continue;
                        }

                        let t = if job.duration > 0.0 {
                            ((current_time - job.start_time) / job.duration).clamp(0.0, 1.0) as f32
                        } else {
                            1.0
                        };

                        let eased_t = match job.easing {
                            EasingType::Linear => t,
                            EasingType::EaseInOutCubic => t * t * (3.0 - 2.0 * t),
                            EasingType::ElasticOut => {
                                let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                                if t == 0.0 { 0.0 }
                                else if t == 1.0 { 1.0 }
                                else { (2.0f32).powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0 }
                            }
                        };

                        let val = job.start_val + (job.end_val - job.start_val) * eased_t;
                        changes.push((job.property.clone(), val));

                        if current_time >= job.start_time + job.duration {
                            jobs_to_remove.push(i);
                        }
                    }
                }

                // Step 1.5: Apply changes
                for (property, val) in changes {
                    match property.as_str() {
                        "x" => if let Some(tr) = world.get_mut::<Transform>(entity) { tr.position.x = val; },
                        "y" => if let Some(tr) = world.get_mut::<Transform>(entity) { tr.position.y = val; },
                        "z" => if let Some(tr) = world.get_mut::<Transform>(entity) { tr.position.z = val; },
                        "scale" => if let Some(tr) = world.get_mut::<Transform>(entity) {
                             tr.scale = glam::Vec3::splat(val);
                        },
                        "opacity" => {
                             if let Some(mat) = world.get_mut::<Material>(entity) {
                                 mat.opacity = val;
                             }
                             if let Some(mc) = world.get_mut::<MaterialComponent>(entity) {
                                 mc.descriptor.opacity = val;
                                 mc.descriptor.base_color[3] = val;
                             }
                        },
                        "r" => {
                             if let Some(mat) = world.get_mut::<Material>(entity) { mat.base_color.r = val; }
                             if let Some(mc) = world.get_mut::<MaterialComponent>(entity) { mc.descriptor.base_color[0] = val; }
                        },
                        "g" => {
                             if let Some(mat) = world.get_mut::<Material>(entity) { mat.base_color.g = val; }
                             if let Some(mc) = world.get_mut::<MaterialComponent>(entity) { mc.descriptor.base_color[1] = val; }
                        },
                        "b" => {
                             if let Some(mat) = world.get_mut::<Material>(entity) { mat.base_color.b = val; }
                             if let Some(mc) = world.get_mut::<MaterialComponent>(entity) { mc.descriptor.base_color[2] = val; }
                        },
                        "a" => {
                             if let Some(mat) = world.get_mut::<Material>(entity) { mat.base_color.a = val; }
                             if let Some(mc) = world.get_mut::<MaterialComponent>(entity) { mc.descriptor.base_color[3] = val; }
                        },
                        "__path__" => {
                            // Handled separately below to avoid borrow conflicts
                        }
                        _ => {}
                    }
                }

                // Step 1.6: Apply Path Morphing
                let jobs = world.get::<Animator>(entity).map(|a| a.jobs.clone());
                if let Some(jobs) = jobs {
                    for job in &jobs {
                        if job.property == "__path__" {
                             let t = if job.duration > 0.0 {
                                ((current_time - job.start_time) / job.duration).clamp(0.0, 1.0) as f32
                             } else { 1.0 };
                             
                             let eased_t = match job.easing {
                                EasingType::Linear => t,
                                EasingType::EaseInOutCubic => t * t * (3.0 - 2.0 * t),
                                _ => t,
                             };

                             if let (Some(start), Some(end)) = (&job.path_start, &job.path_target) {
                                 let mut lerped_commands = Vec::new();
                                 let len = start.commands.len().min(end.commands.len());
                                 
                                 for i in 0..len {
                                     use crate::scene::world::types::PathCommand;
                                     let cmd = match (&start.commands[i], &end.commands[i]) {
                                         (PathCommand::MoveTo(p1), PathCommand::MoveTo(p2)) => PathCommand::MoveTo(p1.lerp(*p2, eased_t)),
                                         (PathCommand::LineTo(p1), PathCommand::LineTo(p2)) => PathCommand::LineTo(p1.lerp(*p2, eased_t)),
                                         (PathCommand::CubicTo(a1, b1, c1), PathCommand::CubicTo(a2, b2, c2)) => PathCommand::CubicTo(a1.lerp(*a2, eased_t), b1.lerp(*b2, eased_t), c1.lerp(*c2, eased_t)),
                                         (a, _) => a.clone(),
                                     };
                                     lerped_commands.push(cmd);
                                 }
                                 
                                 if let Some(path) = world.get_mut::<crate::scene::world::types::PathData>(entity) {
                                     path.commands = lerped_commands;
                                 }
                             }
                        }
                    }
                }
            }

            // Step 2: Cleanup finished jobs
            if !jobs_to_remove.is_empty() {
                if let Some(animator) = world.get_mut::<Animator>(entity) {
                    for i in jobs_to_remove.into_iter().rev() {
                        animator.jobs.remove(i);
                    }
                }
            }
        }
    }
}
