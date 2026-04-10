use wasm_bindgen::prelude::*;
use ferrous_app::{App, AppContext, Color, DrawContext, FerrousApp, Vec3};
use ferrous_core::scene::ElementKind;
use ferrous_core::input::MouseButton;
use ferrous_renderer::Vertex;
use std::sync::{Arc, Mutex};

pub enum JsCommand {
    LoadMesh { path: String },
    CreateTerrain,
    ToggleDiagnostics,
    ToggleProceduralSky,
    SpawnEntity { 
        name: String, 
        kind: String, 
        position: [f32; 3], 
        color: [f32; 3] 
    },
    UpdateMaterial {
        entity_name: String,
        r: f32, g: f32, b: f32,
        metallic: f32,
        roughness: f32,
    },
    SetCamera {
        eye: [f32; 3],
        target: [f32; 3],
    },
    ClearWorld,
    FocusCanvas,
}

struct WebState {
    pub command_queue: Arc<Mutex<Vec<JsCommand>>>,
    pub terrain_mesh_key: Option<String>,
    pub floor_created: bool,
    
    // -- Sculpting Tool State --
    pub terrain_heights: Vec<f32>,
    pub terrain_size: usize,
    pub terrain_scale: f32,
    pub brush_radius: f32,
    pub brush_strength: f32,
    pub frame_delay: u32,

    // -- Fly Cam State --
    pub cam_pos: Vec3,
    pub cam_yaw: f32,
    pub cam_pitch: f32,
    pub move_speed: f32,
}

impl WebState {
    fn generate_terrain_mesh(&mut self, ctx: &AppContext) -> ferrous_renderer::Mesh {
        let size = self.terrain_size;
        let scale = self.terrain_scale;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        // 1. Initial generation or use existing
        if self.terrain_heights.is_empty() {
            self.terrain_heights = vec![-2.8; size * size];
        }

        let get_h = |tx: usize, tz: usize, heights: &[f32]| -> f32 {
            heights[tz * size + tx]
        };

        for z in 0..size {
            for x in 0..size {
                let px = (x as f32) * scale - (size as f32 * 0.5 * scale);
                let pz = (z as f32) * scale - (size as f32 * 0.5 * scale);
                let py = get_h(x, z, &self.terrain_heights);

                // Normal calculation: central differences
                let mut nx = 0.0;
                let mut nz = 0.0;
                if x > 0 && x < size - 1 && z > 0 && z < size - 1 {
                    nx = (get_h(x - 1, z, &self.terrain_heights) - get_h(x + 1, z, &self.terrain_heights)) / (2.0 * scale);
                    nz = (get_h(x, z - 1, &self.terrain_heights) - get_h(x, z + 1, &self.terrain_heights)) / (2.0 * scale);
                }
                let normal = Vec3::new(nx, 1.0, nz).normalize();

                vertices.push(Vertex::new(
                    [px, py, pz],
                    [normal.x, normal.y, normal.z],
                    [x as f32 / (size - 1) as f32, z as f32 / (size - 1) as f32],
                ));
            }
        }

        for z in 0..size - 1 {
            for x in 0..size - 1 {
                let top_left = z * size + x;
                let top_right = top_left + 1;
                let bottom_left = (z + 1) * size + x;
                let bottom_right = bottom_left + 1;

                indices.push(top_left as u32);
                indices.push(bottom_left as u32);
                indices.push(bottom_right as u32);

                indices.push(top_left as u32);
                indices.push(bottom_right as u32);
                indices.push(top_right as u32);
            }
        }

        ctx.render.create_mesh("ProceduralTerrain", vertices, indices)
    }
    fn generate_base_plane(&self, ctx: &AppContext) -> ferrous_renderer::Mesh {
        // A massive world-scale quad to simulate an infinite environment
        let size = 5000.0;
        let uv_scale = 200.0;
        let mut vertices = Vec::new();
        let normal = [0.0, 1.0, 0.0];
        
        // Define quad vertices with CCW winding for +Y visibility
        vertices.push(Vertex::new([-size, -2.8, -size], normal, [0.0, 0.0]));
        vertices.push(Vertex::new([size, -2.8, -size], normal, [uv_scale, 0.0]));
        vertices.push(Vertex::new([size, -2.8, size], normal, [uv_scale, uv_scale]));
        vertices.push(Vertex::new([-size, -2.8, size], normal, [0.0, uv_scale]));
        
        // Correct CCW winding indices for +Y visibility: 
        // Triangle 1: 0->2->1 (BL->TR->BR)
        // Triangle 2: 0->3->2 (BL->TL->TR)
        let indices = vec![0, 2, 1, 0, 3, 2];
        
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[Ferrous-Setup] Calling create_mesh for floor..."));
        
        ctx.render.create_mesh("InfiniteFloor", vertices, indices)
    }
}

impl FerrousApp for WebState {
    fn setup(&mut self, _ctx: &mut AppContext) {
        // Minimal setup for WASM stability
    }

    fn update(&mut self, ctx: &mut AppContext) {
        // 0. Focus Logic
        let has_focus = {
            let mut q = self.command_queue.lock().unwrap();
            let mut focus = false;
            q.retain(|cmd| match cmd {
                JsCommand::FocusCanvas => { focus = true; false },
                _ => true,
            });
            focus
        };
        if has_focus {
             // Optional: visual cue?
        }

        // 1. Fly Camera & Input
        let dt = ctx.time.delta;
        let mut move_dir = Vec3::ZERO;
        
        use ferrous_core::input::KeyCode;
        if ctx.input.is_key_down(KeyCode::KeyW) { move_dir.z -= 1.0; }
        if ctx.input.is_key_down(KeyCode::KeyS) { move_dir.z += 1.0; }
        if ctx.input.is_key_down(KeyCode::KeyA) { move_dir.x -= 1.0; }
        if ctx.input.is_key_down(KeyCode::KeyD) { move_dir.x += 1.0; }
        if ctx.input.is_key_down(KeyCode::Space) { move_dir.y += 1.0; }
        if ctx.input.is_key_down(KeyCode::ShiftLeft) { move_dir.y -= 1.0; }

        if move_dir.length_squared() > 0.0 {
            let rotation = ferrous_app::Quat::from_rotation_y(self.cam_yaw) * ferrous_app::Quat::from_rotation_x(self.cam_pitch);
            let world_move = rotation * move_dir.normalize();
            self.cam_pos += world_move * self.move_speed * dt;
        }

        // Mouse rotation (only if middle mouse or some other trigger? or just always?)
        // Let's use Right Click for rotation since Left Click is sculpting.
        if ctx.input.is_button_down(MouseButton::Right) {
             let delta = ctx.input.mouse_delta();
             self.cam_yaw -= delta.0 * 0.005;
             self.cam_pitch -= delta.1 * 0.005;
             self.cam_pitch = self.cam_pitch.clamp(-1.4, 1.4);
        }

        let forward = ferrous_app::Quat::from_rotation_y(self.cam_yaw) * ferrous_app::Quat::from_rotation_x(self.cam_pitch) * Vec3::new(0.0, 0.0, -1.0);
        ctx.render.set_camera_eye(self.cam_pos);
        ctx.render.renderer_mut().camera_mut().target = self.cam_pos + forward;

        // 2. Lazy Scene Setup (Ground & Lights)
        if !self.floor_created {
             ctx.render.set_directional_light([0.5, -1.0, 0.5], [1.0, 1.0, 1.0], 1.2);
             
             let floor = self.generate_base_plane(ctx);
             ctx.render.register_mesh("infinite_floor", floor);
             ctx.world.spawn("InfiniteGround")
                .with_kind(ElementKind::Mesh { asset_key: "infinite_floor".to_string() })
                .with_color(Color::rgb(0.2, 0.22, 0.25))
                .build();
             
             self.floor_created = true;
             log::info!("[Ferrous] Lazy Scene Setup Complete");
        }

        // 2. Handle JS Commands
        let commands = {
            let mut queue = self.command_queue.lock().unwrap();
            queue.drain(..).collect::<Vec<_>>()
        };
        
        for cmd in commands {
            match cmd {
                JsCommand::CreateTerrain => {
                    if self.terrain_mesh_key.is_none() {
                        let terrain = self.generate_terrain_mesh(ctx);
                        ctx.render.register_mesh("sculptable_terrain", terrain);
                        ctx.world.spawn("Terrain")
                            .with_kind(ElementKind::Mesh { asset_key: "sculptable_terrain".to_string() })
                            .with_color(Color::rgb(0.4, 0.45, 0.42))
                            .build();
                        self.terrain_mesh_key = Some("sculptable_terrain".to_string());
                        log::info!("[Ferrous] Sculptable Terrain Created");
                    }
                }
                JsCommand::ToggleProceduralSky => {
                    ctx.render.renderer_mut().set_sky_procedural();
                }
                JsCommand::SpawnEntity { name, kind, position, color } => {
                    let mut b = ctx.world.spawn(&name);
                    match kind.as_str() {
                        "Cube" => { b = b.with_kind(ElementKind::Cube { half_extents: Vec3::splat(0.5) }); },
                        mesh_key => { b = b.with_kind(ElementKind::Mesh { asset_key: mesh_key.to_string() }); }
                    }
                    b.with_position(Vec3::from_array(position))
                     .with_color(Color::rgb(color[0], color[1], color[2]))
                     .build();
                    log::info!("[Ferrous] Spawned entity '{}' ({})", name, kind);
                }
                JsCommand::ClearWorld => {
                    ctx.world.clear();
                    self.terrain_mesh_key = None;
                    self.floor_created = false;
                    log::info!("[Ferrous] World cleared");
                }
                JsCommand::SetCamera { eye, target } => {
                    ctx.render.set_camera_eye(Vec3::from_array(eye));
                    ctx.render.renderer_mut().camera_mut().target = Vec3::from_array(target);
                }
                JsCommand::UpdateMaterial { entity_name, r, g, b, metallic, roughness } => {
                    if let Some(handle) = ctx.world.find_entity_by_name(&entity_name) {
                        ctx.world.set_color(handle, Color::rgb(r, g, b));
                        // Update material descriptor if it has one
                        let mut desc = ctx.world.get_material_descriptor(handle).cloned().unwrap_or_default();
                        desc.base_color = [r, g, b, 1.0];
                        desc.metallic = metallic;
                        desc.roughness = roughness;
                        ctx.world.set_material_descriptor(handle, desc);
                    }
                }
                _ => {}
            }
        }

        // 3. Real-time Sculpting Logic
        if self.terrain_mesh_key.is_some() {
            let m_pos = ctx.input.mouse_pos_f32();
            let (ray_o, ray_d) = ctx.render.renderer().get_ray(m_pos);
            
            // Intersect ray with Ground Plane at Y = -2.8
            if ray_d.y.abs() > 0.001 {
                let t = (-2.8 - ray_o.y) / ray_d.y;
                if t > 0.0 {
                    let hit_p = ray_o + ray_d * t;
                    
                    // -- Visual Feedback: Draw the Brush Circle --
                    ctx.render.renderer_mut().queue_gizmo(ferrous_app::GizmoDraw {
                        transform: ferrous_app::Mat4::from_translation(hit_p + ferrous_app::Vec3::Y * 0.1),
                        mode: ferrous_core::scene::GizmoMode::Rotate, // Use rotate mode ring as brush cursor
                        highlighted_axis: Some(ferrous_core::scene::Axis::Y),
                        highlighted_plane: None,
                        style: {
                            let mut st = ferrous_core::scene::GizmoStyle::default();
                            st.arm_length = self.brush_radius;
                            st
                        },
                    });

                    // Sculpting application
                    let is_raising = ctx.input.is_button_down(MouseButton::Left);
                    let is_lowering = ctx.input.is_button_down(MouseButton::Right);
                    
                    if is_raising || is_lowering {
                        let sign = if is_raising { 1.0 } else { -1.0 };
                        let radius_sq = self.brush_radius * self.brush_radius;
                        let mut changed = false;

                        for z in 0..self.terrain_size {
                            for x in 0..self.terrain_size {
                                let px = (x as f32) * self.terrain_scale - (self.terrain_size as f32 * 0.5 * self.terrain_scale);
                                let pz = (z as f32) * self.terrain_scale - (self.terrain_size as f32 * 0.5 * self.terrain_scale);
                                
                                let dx = px - hit_p.x;
                                let dz = pz - hit_p.z;
                                let d_sq = dx * dx + dz * dz;

                                if d_sq < radius_sq {
                                    let dist = d_sq.sqrt();
                                    let falloff = (1.0 - dist / self.brush_radius).powf(2.0);
                                    self.terrain_heights[z * self.terrain_size + x] += sign * self.brush_strength * falloff;
                                    changed = true;
                                }
                            }
                        }

                        if changed {
                            let new_mesh = self.generate_terrain_mesh(ctx);
                            ctx.render.register_mesh("sculptable_terrain", new_mesh);
                        }
                    }
                }
            }
        }
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        let (w, h) = (dc.ctx.window_size.0 as f32, dc.ctx.window_size.1 as f32);
        dc.gui.rect_r(w - 200.0, 20.0, 180.0, 90.0, [0.07, 0.09, 0.15, 0.90], 8.0);
        dc.gui.draw_text(dc.font, "WorldPainter Tool", [w - 185.0, 35.0], 11.0, [0.4, 0.7, 1.0, 1.0]);
        dc.gui.draw_text(dc.font, "L-Click: Raise | R-Click: Lower", [w - 185.0, 55.0], 9.0, [0.8, 0.8, 0.8, 1.0]);
        dc.gui.draw_text(dc.font, &format!("Brush Size: {}m", self.brush_radius), [w - 185.0, 75.0], 9.0, [0.1, 0.9, 0.5, 0.8]);
        dc.gui.draw_text(dc.font, &format!("Strength: {:.2}", self.brush_strength), [w - 185.0, 90.0], 9.0, [0.1, 0.9, 0.5, 0.8]);

        // Version Indicator - High Visibility
        dc.gui.draw_text(dc.font, "Ferrous WebEngine v1.0.1 (COMMUNICATOR)", [w - 230.0, h - 30.0], 12.0, [1.0, 1.0, 1.0, 0.9]);
    }
}

#[wasm_bindgen]
pub struct FerrousWebEngine {
    command_queue: Arc<Mutex<Vec<JsCommand>>>,
}

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        let _ = fern::Dispatch::new().level(log::LevelFilter::Debug)
            .chain(fern::Output::call(|record| {
                web_sys::console::log_1(&JsValue::from_str(&format!("[Ferrous] {}", record.args())));
            })).apply();
        Self { command_queue: Arc::new(Mutex::new(Vec::new())) }
    }

    #[wasm_bindgen]
    pub fn mount_and_run(&self) -> Result<(), JsValue> {
        let terrain_size = 64;
        let queue = self.command_queue.clone();
        
        let app_state = WebState {
            command_queue: self.command_queue.clone(),
            terrain_mesh_key: None,
            floor_created: false,
            brush_radius: 5.0,
            brush_strength: 2.0,
            frame_delay: 0,
            cam_pos: Vec3::new(25.0, 20.0, 25.0),
            cam_yaw: -2.3,
            cam_pitch: -0.5,
            move_speed: 15.0,
            terrain_heights: vec![-2.8; terrain_size * terrain_size],
            terrain_size,
            terrain_scale: 0.8,
        };
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[Ferrous] Pre-allocated terrain memory: {}x{}", terrain_size, terrain_size)));
        
        // Setup focus on click: whenever the user clicks, we make sure the engine "feels" active
        if let Some(win) = web_sys::window() {
            if let Some(doc) = win.document() {
                let on_click = wasm_bindgen::prelude::Closure::<dyn FnMut()>::new(move || {
                    if let Ok(mut q) = queue.try_lock() {
                        q.push(JsCommand::FocusCanvas);
                    }
                });
                doc.add_event_listener_with_callback("mousedown", on_click.as_ref().unchecked_ref()).ok();
                on_click.forget();
            }
        }

        static FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
        App::new(app_state)
            .with_background_color(Color::rgb(0.015, 0.02, 0.03))
            .with_font_bytes(FONT_BYTES)
            .with_render_quality(ferrous_core::RenderQuality::Low)
            .with_msaa(1) 
            .with_mode(ferrous_app::AppMode::Game3D)
            .run();
        Ok(())
    }

    #[wasm_bindgen]
    pub fn create_terrain(&self) {
        if let Ok(mut q) = self.command_queue.try_lock() {
            q.push(JsCommand::CreateTerrain);
        } else {
            web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str("[Ferrous] Command queue busy, retrying next tick..."));
        }
    }
    #[wasm_bindgen]
    pub fn toggle_sky(&self) {
        if let Ok(mut q) = self.command_queue.try_lock() {
            q.push(JsCommand::ToggleProceduralSky);
        }
    }

    #[wasm_bindgen]
    pub fn clear_world(&self) {
        if let Ok(mut q) = self.command_queue.try_lock() {
            q.push(JsCommand::ClearWorld);
        }
    }

    #[wasm_bindgen]
    pub fn spawn_entity(&self, name: String, kind: String, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32) {
        if let Ok(mut q) = self.command_queue.try_lock() {
            q.push(JsCommand::SpawnEntity { 
                name, 
                kind, 
                position: [x, y, z],
                color: [r, g, b]
            });
        }
    }

    #[wasm_bindgen]
    pub fn update_material(&self, name: String, r: f32, g: f32, b: f32, metal: f32, rough: f32) {
        if let Ok(mut q) = self.command_queue.try_lock() {
            q.push(JsCommand::UpdateMaterial { 
                entity_name: name,
                r, g, b,
                metallic: metal,
                roughness: rough
            });
        }
    }

    #[wasm_bindgen]
    pub fn set_camera(&self, ex: f32, ey: f32, ez: f32, tx: f32, ty: f32, tz: f32) {
        if let Ok(mut q) = self.command_queue.try_lock() {
            q.push(JsCommand::SetCamera { 
                eye: [ex, ey, ez], 
                target: [tx, ty, tz]
            });
        }
    }
}
