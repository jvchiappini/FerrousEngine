//! Animation keyframe, clip, player components and system.

#![cfg(feature = "ecs")]

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;

// ────────────────────────────────────────────────────────────────────────────
// Animation components

/// A single keyframe: (time_seconds, value).
#[derive(Debug, Clone, Copy)]
pub struct Keyframe<T: Clone + Copy> {
    pub time: f32,
    pub value: T,
}

/// Simple linear-interpolation animation clip for a scalar or Vec3.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// Keyframes for position (local-space).
    pub position_keys: Vec<Keyframe<glam::Vec3>>,
    /// Total duration in seconds; loops when `looping` is true.
    pub duration: f32,
    /// Whether the clip loops.
    pub looping: bool,
}

impl AnimationClip {
    /// Sample the position at `t` seconds using linear interpolation.
    pub fn sample_position(&self, t: f32) -> Option<glam::Vec3> {
        if self.position_keys.is_empty() {
            return None;
        }
        let t = if self.looping && self.duration > 0.0 {
            t % self.duration
        } else {
            t.min(self.duration)
        };
        let keys = &self.position_keys;
        if t <= keys[0].time {
            return Some(keys[0].value);
        }
        let last = keys.last().unwrap();
        if t >= last.time {
            return Some(last.value);
        }
        for i in 0..keys.len() - 1 {
            let a = &keys[i];
            let b = &keys[i + 1];
            if t >= a.time && t < b.time {
                let span = b.time - a.time;
                let alpha = if span > 0.0 { (t - a.time) / span } else { 0.0 };
                return Some(a.value.lerp(b.value, alpha));
            }
        }
        None
    }
}

/// Animation player component — attach to an entity together with a clip.
#[derive(Debug, Clone)]
pub struct AnimationPlayer {
    pub clip: AnimationClip,
    pub time: f32,
    pub playing: bool,
    pub speed: f32,
}

impl AnimationPlayer {
    pub fn new(clip: AnimationClip) -> Self {
        AnimationPlayer { clip, time: 0.0, playing: true, speed: 1.0 }
    }

    pub fn set_playing(&mut self, playing: bool) { self.playing = playing; }
    pub fn seek(&mut self, t: f32) { self.time = t.max(0.0); }
}

impl Component for AnimationPlayer {}

// ────────────────────────────────────────────────────────────────────────────
// AnimationSystem

/// Advances `AnimationPlayer` timers and applies keyframe values to `Transform`.
///
/// Register at `Stage::Update`.
pub struct AnimationSystem;

impl System for AnimationSystem {
    fn name(&self) -> &'static str { "AnimationSystem" }

    fn run(
        &mut self,
        world: &mut ferrous_ecs::world::World,
        resources: &mut ResourceMap,
    ) {
        let dt = resources
            .get::<crate::time::TimeClock>()
            .map(|c| c.at_tick().delta)
            .unwrap_or(0.0);

        let entities: Vec<ferrous_ecs::entity::Entity> = world
            .query::<AnimationPlayer>()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            let sampled_pos = {
                let player = match world.get_mut::<AnimationPlayer>(entity) {
                    Some(p) => p,
                    None => continue,
                };
                if !player.playing { continue; }
                player.time += dt * player.speed;
                player.clip.sample_position(player.time)
            };

            if let Some(pos) = sampled_pos {
                if let Some(t) = world.get_mut::<crate::transform::Transform>(entity) {
                    t.position = pos;
                }
            }
        }
    }
}
