use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use ferrous_app::{AppContext, Color, DrawContext, FerrousApp, Quat, Vec3};
use ferrous_core::scene::ElementKind;

use crate::camera::CameraController;
use crate::commands::JsCommand;
use crate::config::{CameraControlMode, EngineConfig, EngineMetrics};

pub struct WebRuntime {
    pub command_queue: Arc<Mutex<Vec<JsCommand>>>,
    pub camera_override: Arc<Mutex<Option<([f32; 3], [f32; 3])>>>,
    pub metrics: Arc<Mutex<EngineMetrics>>,
    pub enabled_plugins: Arc<Mutex<HashSet<String>>>,
    pub debug_mode: bool,
    pub camera: CameraController,
    pub scenes: HashMap<u32, Vec<String>>,
    pub active_scene: u32,
}

impl WebRuntime {
    pub fn new(
        command_queue: Arc<Mutex<Vec<JsCommand>>>,
        camera_override: Arc<Mutex<Option<([f32; 3], [f32; 3])>>>,
        metrics: Arc<Mutex<EngineMetrics>>,
        enabled_plugins: Arc<Mutex<HashSet<String>>>,
        config: EngineConfig,
        debug_mode: bool,
    ) -> Self {
        let mut scenes = HashMap::new();
        scenes.insert(1, Vec::new());
        Self {
            command_queue,
            camera_override,
            metrics,
            enabled_plugins,
            debug_mode,
            camera: CameraController::new(config),
            scenes,
            active_scene: 1,
        }
    }

    fn add_entity_to_active_scene(&mut self, name: String) {
        self.scenes
            .entry(self.active_scene)
            .or_default()
            .push(name);
    }

    fn remove_entity_from_scenes(&mut self, name: &str) {
        for entities in self.scenes.values_mut() {
            entities.retain(|entry| entry != name);
        }
    }

    fn apply_legacy_terrain(&mut self, ctx: &mut AppContext) {
        if let Some(handle) = ctx.world.find_entity_by_name("LegacyTerrain") {
            ctx.world.despawn(handle);
        }
        let handle = ctx.world.spawn_quad(
            "LegacyTerrain",
            Vec3::new(0.0, -2.5, 0.0),
            80.0,
            80.0,
            true,
        );
        ctx.world.set_color(handle, Color::rgb(0.35, 0.4, 0.38));
        self.add_entity_to_active_scene("LegacyTerrain".to_string());
    }
}

impl FerrousApp for WebRuntime {
    fn setup(&mut self, _ctx: &mut AppContext) {}

    fn update(&mut self, ctx: &mut AppContext) {
        let camera_override = {
            let mut camera_override_guard = self.camera_override.lock().unwrap();
            camera_override_guard.take()
        };

        if let Some((eye, target)) = camera_override {
            self.camera.set_camera(eye, target);
        }

        let commands = {
            let mut queue = self.command_queue.lock().unwrap();
            queue.drain(..).collect::<Vec<_>>()
        };

        {
            let mut m = self.metrics.lock().unwrap();
            m.commands_processed += commands.len() as u64;
        }

        if !commands.is_empty() {
            log::info!("[Ferrous] Processing {} queued command(s)", commands.len());
        }

        for cmd in commands {
            match cmd {
                JsCommand::CreateScene { scene_id } => {
                    self.scenes.entry(scene_id).or_default();
                }
                JsCommand::SetActiveScene { scene_id } => {
                    self.scenes.entry(scene_id).or_default();
                    self.active_scene = scene_id;
                }
                JsCommand::CreateBox {
                    name,
                    position,
                    size,
                    color,
                } => {
                    let handle = ctx.world.spawn_box(
                        name.clone(),
                        Vec3::from_array(position),
                        Vec3::from_array(size),
                    );
                    ctx.world.set_color(handle, Color::rgb(color[0], color[1], color[2]));
                    self.add_entity_to_active_scene(name);
                }
                JsCommand::CreateSphere {
                    name,
                    position,
                    radius,
                    segments,
                    color,
                } => {
                    let handle = ctx.world.spawn_sphere(
                        name.clone(),
                        Vec3::from_array(position),
                        radius,
                        segments,
                    );
                    ctx.world.set_color(handle, Color::rgb(color[0], color[1], color[2]));
                    self.add_entity_to_active_scene(name);
                }
                JsCommand::SpawnEntity {
                    name,
                    kind,
                    position,
                    color,
                } => {
                    let mut builder = ctx.world.spawn(name.clone());
                    match kind.as_str() {
                        "Cube" => {
                            builder = builder.with_kind(ElementKind::Cube {
                                half_extents: Vec3::splat(0.5),
                            })
                        }
                        "Sphere" => {
                            builder = builder.with_kind(ElementKind::Sphere {
                                radius: 0.5,
                                latitudes: 12,
                                longitudes: 16,
                            })
                        }
                        mesh_key => {
                            builder = builder.with_kind(ElementKind::Mesh {
                                asset_key: mesh_key.to_string(),
                            })
                        }
                    }
                    builder
                        .with_position(Vec3::from_array(position))
                        .with_color(Color::rgb(color[0], color[1], color[2]))
                        .build();
                    self.add_entity_to_active_scene(name);
                }
                JsCommand::SetTransform {
                    name,
                    position,
                    rotation,
                    scale,
                } => {
                    if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                        ctx.world.set_position(handle, Vec3::from_array(position));
                        let rot = Quat::from_rotation_y(rotation[1])
                            * Quat::from_rotation_x(rotation[0])
                            * Quat::from_rotation_z(rotation[2]);
                        ctx.world.set_rotation(handle, rot);
                        ctx.world.set_scale(handle, Vec3::from_array(scale));
                    }
                }
                JsCommand::SetCamera { eye, target } => {
                    self.camera.set_camera(eye, target);
                }
                JsCommand::SetCameraControlMode { mode } => {
                    if let Some(parsed_mode) = CameraControlMode::parse(&mode) {
                        self.camera.set_mode(parsed_mode);
                    }
                }
                JsCommand::AddPointLight {
                    name,
                    position,
                    color,
                    intensity,
                    range,
                } => {
                    ctx.world.spawn_point_light(
                        name.clone(),
                        Vec3::from_array(position),
                        color,
                        intensity,
                        range,
                    );
                    self.add_entity_to_active_scene(name);
                }
                JsCommand::SetDirectionalLight {
                    direction,
                    color,
                    intensity,
                } => {
                    ctx.render.set_directional_light(direction, color, intensity);
                }
                JsCommand::UpdateMaterial {
                    entity_name,
                    r,
                    g,
                    b,
                    metallic,
                    roughness,
                } => {
                    if let Some(handle) = ctx.world.find_entity_by_name(&entity_name) {
                        let mut desc = ctx
                            .world
                            .get_material_descriptor(handle)
                            .cloned()
                            .unwrap_or_default();
                        desc.base_color = [r, g, b, 1.0];
                        desc.metallic = metallic;
                        desc.roughness = roughness;
                        ctx.world.set_material_descriptor(handle, desc);
                        ctx.world.set_color(handle, Color::rgb(r, g, b));
                    }
                }
                JsCommand::RemoveEntity { name } => {
                    if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                        ctx.world.despawn(handle);
                    }
                    self.remove_entity_from_scenes(&name);
                }
                JsCommand::ClearWorld => {
                    ctx.world.clear();
                    for entities in self.scenes.values_mut() {
                        entities.clear();
                    }
                }
                JsCommand::SetDebugMode { enabled } => {
                    self.debug_mode = enabled;
                    log::info!("[Ferrous] Debug mode: {}", if enabled { "on" } else { "off" });
                }
                JsCommand::EnablePlugin { name } => {
                    self.enabled_plugins.lock().unwrap().insert(name);
                }
                JsCommand::DisablePlugin { name } => {
                    self.enabled_plugins.lock().unwrap().remove(&name);
                }
                JsCommand::LegacyCreateTerrain => {
                    if self.enabled_plugins.lock().unwrap().contains("terrain") {
                        self.apply_legacy_terrain(ctx);
                    } else {
                        log::warn!("[Ferrous] terrain plugin disabled; create_terrain ignored");
                    }
                }
                JsCommand::LegacyToggleSky => {
                    if self.enabled_plugins.lock().unwrap().contains("sky") {
                        ctx.render.renderer_mut().set_sky_procedural();
                    } else {
                        log::warn!("[Ferrous] sky plugin disabled; toggle_sky ignored");
                    }
                }
            }
        }

        self.camera.update_from_input(ctx);
        ctx.render.set_camera_eye(self.camera.eye());
        ctx.render.renderer_mut().camera_mut().target = self.camera.target();
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        if !self.debug_mode {
            return;
        }

        let (w, h) = (dc.ctx.window_size.0 as f32, dc.ctx.window_size.1 as f32);
        dc.gui.rect_r(w - 260.0, 14.0, 245.0, 52.0, [0.08, 0.1, 0.14, 0.86], 8.0);
        dc.gui.draw_text(dc.font, "Ferrous Web Engine", [w - 246.0, 30.0], 11.0, [0.56, 0.84, 1.0, 1.0]);
        dc.gui.draw_text(dc.font, &format!("scene={} entities={}", self.active_scene, self.scenes.get(&self.active_scene).map(|s| s.len()).unwrap_or(0)), [w - 246.0, 48.0], 9.0, [0.9, 0.92, 0.95, 0.95]);

        let _ = h;
    }
}