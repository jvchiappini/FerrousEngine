use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use once_cell::sync::Lazy;

use ferrous_app::{AppContext, Color, DrawContext, FerrousApp, Quat, Vec3};
use ferrous_core::scene::ElementKind;
use ferrous_renderer::Vertex;

use crate::camera::CameraController;
use crate::commands::JsCommand;
use crate::config::{CameraControlMode, EngineConfig, EngineMetrics};

pub static ASSET_RESOLVERS: Lazy<Mutex<HashMap<u32, js_sys::Function>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

pub struct WebRuntime {
    pub command_queue: Arc<Mutex<Vec<JsCommand>>>,
    pub camera_override: Arc<Mutex<Option<([f32; 3], [f32; 3])>>>,
    pub metrics: Arc<Mutex<EngineMetrics>>,
    pub enabled_plugins: Arc<Mutex<HashSet<String>>>,
    pub error_callback: Arc<Mutex<Option<js_sys::Function>>>,
    pub debug_mode: bool,
    pub camera: CameraController,
    pub scenes: HashMap<u32, Vec<String>>,
    pub active_scene: u32,
    pub pending_textures: HashMap<u32, ferrous_assets::AssetHandle<ferrous_assets::ImageData>>,
    pub pending_models: HashMap<u32, ferrous_assets::AssetHandle<ferrous_assets::GltfModel>>,
    pub plugins: Vec<Box<dyn crate::plugin::WebPlugin>>,
}

impl WebRuntime {
    pub fn new(
        command_queue: Arc<Mutex<Vec<JsCommand>>>,
        camera_override: Arc<Mutex<Option<([f32; 3], [f32; 3])>>>,
        metrics: Arc<Mutex<EngineMetrics>>,
        enabled_plugins: Arc<Mutex<HashSet<String>>>,
        error_callback: Arc<Mutex<Option<js_sys::Function>>>,
        plugins: Vec<Box<dyn crate::plugin::WebPlugin>>,
        config: EngineConfig,
        debug_mode: bool,
    ) -> Self {
        Self {
            command_queue,
            camera_override,
            metrics,
            enabled_plugins,
            error_callback,
            debug_mode,
            camera: CameraController::new(config),
            scenes: HashMap::new(),
            active_scene: 1,
            pending_textures: HashMap::new(),
            pending_models: HashMap::new(),
            plugins,
        }
    }

    pub fn report_error(&self, code: &str, message: &str) {
        let payload = format!(
            "{{\"code\":\"{}\",\"message\":\"{}\"}}",
            code,
            message.replace('"', "\\\"")
        );
        if let Some(callback) = self.error_callback.lock().unwrap().as_ref() {
            use wasm_bindgen::JsValue;
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&payload));
        } else {
            log::error!("[Ferrous:{}] {}", code, message);
        }
    }

    pub fn is_plugin_enabled(&self, name: &str) -> bool {
        self.enabled_plugins.lock().unwrap().contains(name)
    }

    pub fn add_entity_to_active_scene(&mut self, name: String) {
        self.scenes
            .entry(self.active_scene)
            .or_default()
            .push(name);
    }

    pub fn remove_entity_from_scenes(&mut self, name: &str) {
        for entities in self.scenes.values_mut() {
            entities.retain(|entry| entry != name);
        }
    }

    pub fn apply_legacy_terrain(&mut self, ctx: &mut AppContext) {
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

    fn ensure_fallback_scene(&mut self, ctx: &mut AppContext) {
        if !ctx.world.is_empty() {
            return;
        }

        let floor = ctx.world.spawn_box(
            "FallbackFloor",
            Vec3::new(0.0, -1.25, 0.0),
            Vec3::new(12.0, 0.25, 12.0),
        );
        ctx.world.set_color(floor, Color::rgb(0.18, 0.22, 0.3));

        let cube = ctx.world.spawn_box(
            "FallbackCube",
            Vec3::new(0.0, 0.8, 0.0),
            Vec3::new(1.8, 1.8, 1.8),
        );
        ctx.world.set_color(cube, Color::rgb(1.0, 0.25, 0.15));

        let near_cube = ctx.world.spawn_box(
            "FallbackNearCube",
            Vec3::new(0.0, 1.0, 2.2),
            Vec3::new(0.9, 0.9, 0.9),
        );
        ctx.world.set_color(near_cube, Color::rgb(1.0, 1.0, 1.0));

        let tri_vertices = vec![
            Vertex::new([-1.2, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            Vertex::new([1.2, -1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
            Vertex::new([0.0, 1.2, 0.0], [0.0, 0.0, 1.0], [0.5, 1.0]),
        ];
        let tri_indices = vec![0_u32, 1_u32, 2_u32];
        let tri_mesh = ctx
            .render
            .create_mesh("FallbackTriangleMesh", tri_vertices, tri_indices);
        ctx.render.register_mesh("fallback_triangle_mesh", tri_mesh);

        ctx.world
            .spawn("FallbackTriangle")
            .with_kind(ElementKind::Mesh {
                asset_key: "fallback_triangle_mesh".to_string(),
            })
            .with_position(Vec3::new(0.0, 1.0, -1.6))
            .with_scale(Vec3::new(2.0, 2.0, 2.0))
            .with_color(Color::rgb(1.0, 1.0, 0.0))
            .build();

        ctx.world.spawn_point_light(
            "FallbackLight",
            Vec3::new(2.5, 3.0, 1.5),
            [1.0, 1.0, 1.0],
            9.0,
            30.0,
        );

        ctx.render
            .set_directional_light([0.25, -1.0, 0.2], [1.0, 1.0, 1.0], 2.5);
        self.camera.set_camera([0.8, 1.4, 4.0], [0.0, 1.0, 0.0]);

        self.add_entity_to_active_scene("FallbackFloor".to_string());
        self.add_entity_to_active_scene("FallbackCube".to_string());
        self.add_entity_to_active_scene("FallbackNearCube".to_string());
        self.add_entity_to_active_scene("FallbackTriangle".to_string());

        log::warn!("[Ferrous] Fallback scene activated (world was empty)");
    }

    pub fn resolve_scene_export(&self, request_id: u32, json: String) {
        if let Some(resolver) = ASSET_RESOLVERS.lock().unwrap().remove(&request_id) {
            let _ = resolver.call1(&wasm_bindgen::JsValue::NULL, &wasm_bindgen::JsValue::from_str(&json));
        }
    }

    pub fn refresh_active_scene_from_world(&mut self, ctx: &AppContext) {
        let mut names = Vec::new();
        for entity in ctx.world.iter() {
            names.push(entity.name.clone());
        }
        self.scenes.insert(self.active_scene, names);
    }
}

impl FerrousApp for WebRuntime {
    fn setup(&mut self, ctx: &mut AppContext) {
        ctx.render.set_ssao(false);
        ctx.render.set_style(ferrous_renderer::RenderStyle::Pbr);
        ctx.render.set_clear_color(Color::rgb(0.08, 0.1, 0.14));
        ctx.render
            .set_directional_light([0.25, -1.0, 0.2], [1.0, 1.0, 1.0], 2.0);
        ctx.render.set_camera_eye(self.camera.eye());
        ctx.render.renderer_mut().camera_mut().target = self.camera.target();
    }

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

        if !commands.is_empty() {
            {
                let mut m = self.metrics.lock().unwrap();
                m.commands_processed += commands.len() as u64;
            }
            log::info!("[Ferrous] Processing {} queued command(s)", commands.len());
        }

        for cmd in commands {
            crate::dispatcher::CommandDispatcher::dispatch(self, ctx, cmd);
        }

        // --- Plugin Update Hooks ---
        let dt = ctx.time.delta;
        for plugin in &mut self.plugins {
            plugin.on_update(ctx, dt);
        }

        // --- Asset Polling ---
        let mut finished_textures = Vec::new();
        for (&req_id, &handle) in &self.pending_textures {
            if let ferrous_assets::AssetState::Ready(img) = ctx.asset_server.get(handle) {
                // Register texture in GPU
                let gpu_handle = ctx.render.register_texture(img.width, img.height, &img.pixels);
                
                // Notify JS
                if let Some(resolver) = ASSET_RESOLVERS.lock().unwrap().remove(&req_id) {
                    let _ = resolver.call1(&wasm_bindgen::JsValue::NULL, &wasm_bindgen::JsValue::from_f64(gpu_handle.0 as f64));
                }
                finished_textures.push(req_id);
            } else if let ferrous_assets::AssetState::Failed(e) = ctx.asset_server.get(handle) {
                self.report_error("asset.load_failed", &format!("Texture load failed: {}", e));
                finished_textures.push(req_id);
            }
        }
        for id in finished_textures { self.pending_textures.remove(&id); }

        let mut finished_models = Vec::new();
        for (&req_id, &handle) in &self.pending_models {
            if let ferrous_assets::AssetState::Ready(_model) = ctx.asset_server.get(handle) {
                // For models, we might want to return some data, but for now just success
                if let Some(resolver) = ASSET_RESOLVERS.lock().unwrap().remove(&req_id) {
                    let _ = resolver.call1(&wasm_bindgen::JsValue::NULL, &wasm_bindgen::JsValue::NULL);
                }
                finished_models.push(req_id);
            } else if let ferrous_assets::AssetState::Failed(e) = ctx.asset_server.get(handle) {
                self.report_error("asset.load_failed", &format!("Model load failed: {}", e));
                finished_models.push(req_id);
            }
        }
        for id in finished_models { self.pending_models.remove(&id); }

        self.ensure_fallback_scene(ctx);

        self.camera.update_from_input(ctx);
        ctx.render.set_camera_eye(self.camera.eye());
        ctx.render.renderer_mut().camera_mut().target = self.camera.target();
    }

    fn on_sync_world(&mut self, world: &ferrous_core::World) {
        for plugin in &mut self.plugins {
            plugin.on_sync_world(world);
        }
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        if !self.debug_mode {
            return;
        }

        let (w, h) = (dc.ctx.window_size.0 as f32, dc.ctx.window_size.1 as f32);
        dc.gui.rect_r(w - 260.0, 14.0, 245.0, 52.0, [0.08, 0.1, 0.14, 0.86], 8.0);
        dc.gui.draw_text(dc.font, "Ferrous Web Engine", [w - 246.0, 30.0], 11.0, [0.56, 0.84, 1.0, 1.0]);
        dc.gui.draw_text(dc.font, &format!("FPS: {:.0} | scene={} entities={}", dc.ctx.time.fps, self.active_scene, self.scenes.get(&self.active_scene).map(|s| s.len()).unwrap_or(0)), [w - 246.0, 48.0], 9.0, [0.9, 0.92, 0.95, 0.95]);

        let _ = h;
    }
}