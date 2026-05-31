use std::collections::HashMap;
use ferrous_core::{World, Transform, Color};
use ferrous_core::api::types::NodeId;
use ferrous_ecs::prelude::Entity;
use ferrous_ecs::system::System;
use ferrous_gpu::EngineContext;
use ferrous_renderer::{Renderer as GpuRenderer, RendererMode};
use ferrous_core::scene::{Material, Billboard, BillboardMode, ShadowCaster, Element, ElementKind, AnimatorSystem};
use ferrous_core::scene::world::MaterialComponent;
use ferrous_core::scene::world::types::PathData;
use ferrous_core::scene::world::types::PathCommand;
use ferrous_core::glam;
use ferrous_2d::render::types::ShapeInstance;

/// Error type for the Engine SDK.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("GPU context initialization failed: {0}")]
    GpuContext(#[from] anyhow::Error),
    #[error("Failed to build renderer")]
    InitFailed,
}

/// Professional Render SDK entry point.
/// 
/// The `Renderer` provides a high-level facade for managing a 3D scene,
/// handling the underlying ECS world, GPU resources, and frame execution.
/// 
/// # Example
/// ```rust,ignore
/// let mut renderer = Renderer::builder()
///     .with_dimensions(1280, 720)
///     .with_title("Professional App")
///     .build()?;
/// 
/// let mesh = renderer.spawn_mesh("cube", Transform::identity());
/// renderer.update_color(mesh, [1.0, 0.0, 0.0, 1.0]);
/// ```
pub struct Renderer {
    pub world: World,
    pub node_map: HashMap<NodeId, Entity>,
    next_id: u64,
    pub gpu: GpuRenderer,
    animator_system: ferrous_core::scene::AnimatorSystem,
}

/// Configuration options for the `Renderer`.
pub struct RendererConfig {
    pub width: u32,
    pub height: u32,
    pub headless: bool,
    pub target_fps: u32,
    pub title: String,
    pub background_color: Color,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            headless: false,
            target_fps: 60,
            title: "Ferrous Engine".to_string(),
            background_color: Color::rgb(0.1, 0.1, 0.1),
        }
    }
}

/// Fluent builder for creating a [`Renderer`].
pub struct RendererBuilder {
    config: RendererConfig,
}

impl RendererBuilder {
    pub fn new() -> Self {
        Self {
            config: RendererConfig::default(),
        }
    }

    /// Sets the viewport dimensions.
    pub fn with_dimensions(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    /// Enables or disables headless mode (for CLI tools or servers).
    pub fn with_headless_mode(mut self, headless: bool) -> Self {
        self.config.headless = headless;
        self
    }

    /// Sets the target frames per second.
    pub fn with_fps(mut self, fps: u32) -> Self {
        self.config.target_fps = fps;
        self
    }

    /// Sets the window title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.config.title = title.to_string();
        self
    }

    /// Initializes the GPU and returns a fully-functional `Renderer`.
    pub fn build(self) -> Result<Renderer, EngineError> {
        #[cfg(not(target_arch = "wasm32"))]
        let context = pollster::block_on(EngineContext::new())
            .map_err(EngineError::GpuContext)?;
        
        #[cfg(target_arch = "wasm32")]
        let context = {
            log::error!("Async initialization on wasm32 not yet supported via Builder::build()");
            panic!("Async initialization on wasm32 not yet supported");
        };

        let mut gpu = GpuRenderer::new(
            context,
            self.config.width,
            self.config.height,
            wgpu::TextureFormat::Rgba8Unorm, // Use RGBA8 for headless readback compatibility
            4,
            None,
        );

        if self.config.headless {
            use ferrous_renderer::resources::readback::ReadbackFrameManager;
            gpu.readback_manager = Some(ReadbackFrameManager::new(
                &gpu.context.device,
                self.config.width,
                self.config.height,
            ));
        }

        Ok(Renderer {
            world: World::new(),
            node_map: HashMap::new(),
            next_id: 1,
            gpu,
            animator_system: ferrous_core::scene::AnimatorSystem,
        })
    }
}

impl Renderer {
    /// Creates a new builder for the Renderer.
    pub fn builder() -> RendererBuilder {
        RendererBuilder::new()
    }

    fn generate_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Spawns a 3D Mesh into the renderer.
    pub fn spawn_mesh(&mut self, mesh_name: &str, transform: Transform) -> NodeId {
        let id = self.generate_id();
        
        let descriptor = ferrous_core::scene::MaterialDescriptor::default();
        let handle = self.gpu.material_registry.create(
            &self.gpu.context.device,
            &self.gpu.context.queue,
            &descriptor,
        );

        let material = MaterialComponent {
            handle,
            descriptor,
        };

        let element = Element {
            id: id.0,
            name: format!("{}_{}", mesh_name, id.0),
            transform: transform.clone(),
            material: material.clone(),
            kind: match mesh_name {
                "cube" => ElementKind::Cube { half_extents: glam::Vec3::splat(0.5) },
                "sphere" => ElementKind::Sphere { radius: 1.0, latitudes: 32, longitudes: 32 },
                "quad" => ElementKind::Quad { width: 1.0, height: 1.0, double_sided: false },
                _ => ElementKind::Cube { half_extents: glam::Vec3::splat(0.5) },
            },
            visible: true,
            screen_space: false,
            tags: Vec::new(),
            render_handle: None,
            point_light: None,
            fill_color: None,
            stroke_color: None,
            stroke_thickness: 0.0,
            line_cap_style: 0,
        };

        let entity = self.world.ecs.spawn((transform, element, material));
        self.node_map.insert(id, entity);
        id
    }

    /// Spawns a generic 2D path.
    pub fn spawn_2d_path(&mut self, transform: Transform, path_data: ferrous_core::scene::world::types::PathData) -> NodeId {
        let id = self.generate_id();
        
        let descriptor = ferrous_core::scene::MaterialDescriptor {
            base_color: [1.0, 1.0, 1.0, 1.0],
            style_override: Some(ferrous_renderer::RenderStyle::FlatShaded),
            double_sided: true,
            ..Default::default()
        };
        let handle = self.gpu.material_registry.create(&self.gpu.context.device, &self.gpu.context.queue, &descriptor);
        let mut material = MaterialComponent { handle, descriptor };

        let element = Element {
            id: id.0,
            name: format!("path_{}", id.0),
            transform: transform.clone(),
            material: material.clone(),
            kind: ElementKind::Path,
            visible: true,
            screen_space: false,
            tags: Vec::new(),
            render_handle: None,
            point_light: None,
            fill_color: Some([1.0, 1.0, 1.0, 1.0]),
            stroke_color: Some([1.0, 1.0, 1.0, 1.0]),
            stroke_thickness: 0.1,
            line_cap_style: 0,
        };

        // v15: 2D objects use FlatShaded (unlit) style by default so they work without lights
        material.descriptor.style_override = Some(ferrous_core::scene::RenderStyle::FlatShaded);

        let entity = self.world.ecs.spawn((transform, element, material, path_data));
        self.node_map.insert(id, entity);
        self.gpu.mark_dirty();
        id
    }

    /// Spawns a filled 2D circle in the XY plane.
    pub fn spawn_2d_circle(&mut self, x: f32, y: f32, z: f32, radius: f32) -> NodeId {
        use ferrous_core::scene::world::types::{PathData, PathCommand};
        // Approximate circle with 4 bezier curves (Standard Manim/Vectorial approach)
        let c = 0.552284749831 * radius;
        let path = PathData {
            commands: vec![
                PathCommand::MoveTo(glam::Vec2::new(radius, 0.0)),
                PathCommand::CubicTo(glam::Vec2::new(radius, c), glam::Vec2::new(c, radius), glam::Vec2::new(0.0, radius)),
                PathCommand::CubicTo(glam::Vec2::new(-c, radius), glam::Vec2::new(-radius, c), glam::Vec2::new(-radius, 0.0)),
                PathCommand::CubicTo(glam::Vec2::new(-radius, -c), glam::Vec2::new(-c, -radius), glam::Vec2::new(0.0, -radius)),
                PathCommand::CubicTo(glam::Vec2::new(c, -radius), glam::Vec2::new(radius, -c), glam::Vec2::new(radius, 0.0)),
                PathCommand::Close,
            ]
        };
        self.spawn_2d_path(Transform::from_position(glam::Vec3::new(x, y, z)), path)
    }

    /// Spawns a filled 2D rectangle in the XY plane.
    pub fn spawn_2d_rect(&mut self, x: f32, y: f32, z: f32, width: f32, height: f32) -> NodeId {
        use ferrous_core::scene::world::types::{PathData, PathCommand};
        let hw = width * 0.5;
        let hh = height * 0.5;
        // v14: Structure normalization - Use 4 Cubics to allow morphing to Circle
        let path = PathData {
            commands: vec![
                PathCommand::MoveTo(glam::Vec2::new(-hw, -hh)),
                PathCommand::CubicTo(glam::Vec2::new(-hw, -hh), glam::Vec2::new(hw, -hh), glam::Vec2::new(hw, -hh)),
                PathCommand::CubicTo(glam::Vec2::new(hw, -hh), glam::Vec2::new(hw, hh), glam::Vec2::new(hw, hh)),
                PathCommand::CubicTo(glam::Vec2::new(hw, hh), glam::Vec2::new(-hw, hh), glam::Vec2::new(-hw, hh)),
                PathCommand::CubicTo(glam::Vec2::new(-hw, hh), glam::Vec2::new(-hw, -hh), glam::Vec2::new(-hw, -hh)),
                PathCommand::Close,
            ]
        };
        self.spawn_2d_path(Transform::from_position(glam::Vec3::new(x, y, z)), path)
    }

    /// Spawns a thick 2D line segment.
    pub fn spawn_2d_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32) -> NodeId {
        let id = self.spawn_2d_shape("line", Transform::default(), ElementKind::Line2D { x0: x1, y0: y1, x1: x2, y1: y2, thickness });
        // Important: Lines MUST NOT have fill_color or lyon produces empty meshes
        if let Some(entity) = self.node_map.get(&id) {
            if let Some(mut el) = self.world.ecs.get_mut::<Element>(*entity) {
                el.fill_color = None;
            }
        }
        let _ = self.set_stroke(id, [1.0, 1.0, 1.0, 1.0], thickness);
        id
    }

    /// Internal helper shared by all 2D spawn methods.
    ///
    /// Sets `style_override = FlatShaded` so the object renders as solid color
    /// without PBR lighting (i.e. *unlit* for the shading model), while still
    /// being a normal ECS entity that participates in the shadow pass when
    /// the `ShadowCaster` component is present.
    fn spawn_2d_shape(&mut self, label: &str, transform: Transform, kind: ElementKind) -> NodeId {
        let id = self.generate_id();

        // Flat material: pure base_color, no PBR light response.
        // The style_override is picked up by `sync_world` → frame_builder.
        use ferrous_core::scene::RenderStyle;
        let descriptor = ferrous_core::scene::MaterialDescriptor {
            base_color: [1.0, 1.0, 1.0, 1.0],
            style_override: Some(RenderStyle::FlatShaded),
            double_sided: true, // 2D shapes are viewable from both sides
            ..ferrous_core::scene::MaterialDescriptor::default()
        };

        let handle = self.gpu.material_registry.create(
            &self.gpu.context.device,
            &self.gpu.context.queue,
            &descriptor,
        );

        let material = MaterialComponent {
            handle,
            descriptor,
        };

        let element = Element {
            id: id.0,
            name: format!("{}_{}", label, id.0),
            transform: transform.clone(),
            material: material.clone(),
            kind,
            visible: true,
            screen_space: false,
            tags: Vec::new(),
            render_handle: None,
            point_light: None,
            fill_color: Some([1.0, 1.0, 1.0, 1.0]),
            stroke_color: None,
            stroke_thickness: 0.0,
            line_cap_style: 0,
        };

        let entity = self.world.ecs.spawn((transform, element, material));
        self.node_map.insert(id, entity);
        id
    }

    /// Updates the transform of an existing node.
    pub fn update_transform(&mut self, node: NodeId, new_transform: Transform) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(t) = self.world.ecs.get_mut::<Transform>(*entity) {
                *t = new_transform;
                return Ok(());
            }
        }
        Err(())
    }

    /// Updates the color of a specific node.
    pub fn update_color(&mut self, node: NodeId, new_color: [f32; 4]) -> Result<(), ()> {
        let mut updated = false;
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mat) = self.world.ecs.get_mut::<MaterialComponent>(*entity) {
                mat.descriptor.base_color = new_color;
                // Automatically switch to Blend mode if alpha is less than 1.0
                if new_color[3] < 0.999 {
                    mat.descriptor.alpha_mode = ferrous_core::scene::AlphaMode::Blend;
                } else {
                    mat.descriptor.alpha_mode = ferrous_core::scene::AlphaMode::Opaque;
                }
                updated = true;
            }
            if let Some(el) = self.world.ecs.get_mut::<Element>(*entity) {
                el.material.descriptor.base_color = new_color;
                updated = true;
            }
        }
        if updated {
            Ok(())
        } else {
            Err(())
        }
    }

    /// Safely removes a node from the scene.
    pub fn remove_node(&mut self, node: NodeId) -> Result<(), ()> {
        if let Some(entity) = self.node_map.remove(&node) {
            let _ = self.world.ecs.despawn(entity);
            return Ok(());
        }
        Err(())
    }

    /// Removes all managed nodes from the scene.
    pub fn clear(&mut self) {
        for (_, entity) in self.node_map.drain() {
            let _ = self.world.ecs.despawn(entity);
        }
        self.next_id = 1;
        self.gpu.world_material_descs.clear();
        self.gpu.shape_batcher.clear();
    }

    pub fn set_global_light(&mut self, _dir: glam::Vec3, _color: [f32; 4]) {
        // Implementation would update the renderer's light uniform
    }

    /// Returns a mutable reference to the internal camera.
    pub fn camera_mut(&mut self) -> &mut ferrous_renderer::Camera {
        &mut self.gpu.camera_system.camera
    }

    /// Enables or disables the shadow-caster marker component.
    pub fn set_shadow_caster(&mut self, node: NodeId, casts_shadows: bool) -> Result<(), ()> {
        let e = self.node_map.get(&node).ok_or(())?;
        if casts_shadows {
            self.world.ecs.insert(*e, ferrous_core::scene::ShadowCaster);
        } else {
            self.world.ecs.remove::<ferrous_core::scene::ShadowCaster>(*e);
        }
        Ok(())
    }

    /// Configures the billboard mode for the node.
    /// Mode 0: Off, 1: Spherical, 2: Cylindrical.
    pub fn set_billboard(&mut self, node: NodeId, mode: u8) -> Result<(), ()> {
        let e = self.node_map.get(&node).ok_or(())?;
        if mode == 0 {
            self.world.ecs.remove::<ferrous_core::scene::Billboard>(*e);
        } else {
            let bm = if mode == 1 {
                ferrous_core::scene::BillboardMode::Spherical
            } else {
                ferrous_core::scene::BillboardMode::Cylindrical
            };
            self.world.ecs.insert(*e, ferrous_core::scene::Billboard { mode: bm });
        }
        Ok(())
    }

    /// Tags or untags an entity as being drawn in Screen Space using an Ortho overlay.
    pub fn set_screen_space(&mut self, node: NodeId, is_screen: bool) -> Result<(), ()> {
        let e = self.node_map.get(&node).ok_or(())?;
        // 1. Maintain the marker component for backward compatibility/queries
        if is_screen {
            self.world.ecs.insert(*e, ferrous_core::scene::ScreenSpace);
        } else {
            self.world.ecs.remove::<ferrous_core::scene::ScreenSpace>(*e);
        }
        
        // 2. Direct flag in Element for 100% reliable rendering detection in FrameBuilder
        if let Some(elem) = self.world.ecs.get_mut::<ferrous_core::scene::Element>(*e) {
            elem.screen_space = is_screen;
        }
        Ok(())
    }

    /// Adds a billboard constraint to a node.
    pub fn add_billboard_constraint(&mut self, node: NodeId, mode: BillboardMode) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            self.world.ecs.insert(*entity, Billboard { mode });
            return Ok(());
        }
        Err(())
    }

    /// Sync ECS 2D entities to the shape batcher for Pure2D rendering.
    /// Called every frame before rendering when in Pure2D mode.
    pub fn sync_ecs_to_shape_batcher(&mut self) {
        self.gpu.shape_batcher.clear();

        // Collect (entity, element_clone, pos) tuples to avoid borrow conflicts
        // between self.node_map/self.world (immutable) and self.gpu (mutable).
        let mut batch: Vec<(Entity, Element, glam::Vec3)> = Vec::new();
        for (_node_id, entity) in &self.node_map {
            if let Some(el) = self.world.ecs.get::<Element>(*entity) {
                if el.visible {
                    if let Some(t) = self.world.ecs.get::<Transform>(*entity) {
                        batch.push((*entity, el.clone(), t.position));
                    }
                }
            }
        }

        for (entity, element, pos) in &batch {
            match &element.kind {
                ElementKind::Line2D { x0, y0, x1, y1, thickness } => {
                    let color = element.stroke_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    self.gpu.draw_2d_shape(ShapeInstance::line_with_cap(
                        glam::Vec2::new(pos.x + x0, pos.y + y0),
                        glam::Vec2::new(pos.x + x1, pos.y + y1),
                        *thickness,
                        color,
                        element.line_cap_style,
                    ));
                }
                ElementKind::Circle2D { radius, .. } => {
                    if let Some(fill) = element.fill_color {
                        self.gpu.draw_2d_shape(ShapeInstance::circle_z(
                            glam::Vec2::new(pos.x, pos.y), pos.z, *radius, fill,
                        ));
                    }
                }
                ElementKind::Rect2D { width, height } => {
                    if let Some(fill) = element.fill_color {
                        self.gpu.draw_2d_shape(ShapeInstance::rect_z(
                            glam::Vec2::new(pos.x, pos.y), pos.z,
                            glam::Vec2::new(*width, *height), fill,
                        ));
                    }
                }
                ElementKind::Path => {
                    // Only use the shape batcher for Paths in Pure2D mode.
                    // In all other modes, Paths are rendered via lyon tessellation
                    // in sync_world / frame_builder (path_to_mesh). Using the batcher
                    // fallback here would over-draw with broken straight-line strokes.
                    if self.gpu.mode == RendererMode::Pure2D {
                        self.sync_path_to_batcher(*entity, element, *pos);
                    }
                }
                _ => {}
            }
        }
    }

    fn sync_path_to_batcher(&mut self, entity: Entity, element: &Element, pos: glam::Vec3) {
        let path_data = match self.world.ecs.get::<PathData>(entity) {
            Some(p) => p,
            None => return,
        };
        let cmds = &path_data.commands;

        // Try to classify as circle (4 CubicTo arcs + Close)
        if let Some(radius) = classify_circle_path(cmds) {
            if let Some(fill) = element.fill_color {
                self.gpu.draw_2d_shape(ShapeInstance::circle_z(
                    glam::Vec2::new(pos.x, pos.y), pos.z, radius, fill,
                ));
            }
            if let Some(stroke) = element.stroke_color {
                let segments = 48;
                let center = glam::Vec2::new(pos.x, pos.y);
                for i in 0..segments {
                    let t0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
                    let t1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
                    let from = center + glam::Vec2::new(t0.cos() * radius, t0.sin() * radius);
                    let to = center + glam::Vec2::new(t1.cos() * radius, t1.sin() * radius);
                    self.gpu.draw_2d_shape(ShapeInstance::line(from, to, 2.0, stroke));
                }
            }
            return;
        }

        // Try to classify as rect (4 degenerate CubicTo + Close)
        if let Some((width, height)) = classify_rect_path(cmds) {
            if let Some(fill) = element.fill_color {
                self.gpu.draw_2d_shape(ShapeInstance::rect_z(
                    glam::Vec2::new(pos.x, pos.y), pos.z,
                    glam::Vec2::new(width, height), fill,
                ));
            }
            if let Some(stroke) = element.stroke_color {
                let hw = width * 0.5;
                let hh = height * 0.5;
                let pts = [
                    glam::Vec2::new(pos.x - hw, pos.y - hh),
                    glam::Vec2::new(pos.x + hw, pos.y - hh),
                    glam::Vec2::new(pos.x + hw, pos.y + hh),
                    glam::Vec2::new(pos.x - hw, pos.y + hh),
                    glam::Vec2::new(pos.x - hw, pos.y - hh),
                ];
                for i in 0..4 {
                    self.gpu.draw_2d_shape(ShapeInstance::line(pts[i], pts[i + 1], 2.0, stroke));
                }
            }
            return;
        }

        // Fallback: render stroke as line segments (for Plots, Arrows, etc.)
        if let Some(stroke) = element.stroke_color {
            let thickness = element.stroke_thickness.max(1.0);
            let mut prev: Option<glam::Vec2> = None;
            for cmd in cmds {
                match cmd {
                    PathCommand::MoveTo(p) => { prev = None; }
                    PathCommand::LineTo(p) => {
                        if let Some(start) = prev {
                            self.gpu.draw_2d_shape(ShapeInstance::line(
                                glam::Vec2::new(pos.x + start.x, pos.y + start.y),
                                glam::Vec2::new(pos.x + p.x, pos.y + p.y),
                                thickness, stroke,
                            ));
                        }
                        prev = Some(*p);
                    }
                    PathCommand::CubicTo(_, _, end) => {
                        if let Some(start) = prev {
                            self.gpu.draw_2d_shape(ShapeInstance::line(
                                glam::Vec2::new(pos.x + start.x, pos.y + start.y),
                                glam::Vec2::new(pos.x + end.x, pos.y + end.y),
                                thickness, stroke,
                            ));
                        }
                        prev = Some(*end);
                    }
                    PathCommand::Close => { prev = None; }
                }
            }
        }
    }

    /// Executes a single render pass and returns the raw RGBA8 pixels.
    /// Only works if the renderer was initialized in headless mode.
    pub fn render_frame(&mut self, t: f64) -> Option<Vec<u8>> {
        // v11: Run the animation sequencer system
        let mut resources = ferrous_ecs::prelude::ResourceMap::new();
        resources.insert(ferrous_core::Time {
            delta: 0.0, // not used by animator
            elapsed: t,
            frame_count: 0,
            fps: 0.0,
        });
        
        self.animator_system.run(&mut self.world.ecs, &mut resources);

        // 1. Sync ECS world to GPU (skip in Pure2D — 2D entities use shape batcher instead)
        if self.gpu.mode != RendererMode::Pure2D {
            self.gpu.sync_world(&self.world);
        }

        // 1b. Sync ECS 2D entities to shape batcher (used by Pure2D mode)
        self.sync_ecs_to_shape_batcher();

        // 2. Begin encoding
        let mut encoder = self.gpu.begin_frame();

        // 3. Render scene to internal target
        self.gpu.render_to_internal_target(&mut encoder, None);

        // 4. Copy to readback buffer if available
        if let Some(readback) = &self.gpu.readback_manager {
            readback.copy_to_buffer(&mut encoder, self.gpu.render_target.color_texture());
            
            self.gpu.context.queue.submit(Some(encoder.finish()));

            // 5. Poll and map (blocking)
            #[cfg(not(target_arch = "wasm32"))]
            {
                use pollster::FutureExt;
                return readback.poll_and_map(&self.gpu.context.device).block_on().ok();
            }
        }

        None
    }

    /// Like `render_frame`, but writes pixels into a reusable buffer instead of allocating a new `Vec`.
    /// The buffer is cleared and reused; callers should pre-allocate sufficient capacity.
    /// Returns `true` on success.
    pub fn render_frame_into(&mut self, t: f64, out: &mut Vec<u8>) -> bool {
        let mut resources = ferrous_ecs::prelude::ResourceMap::new();
        resources.insert(ferrous_core::Time {
            delta: 0.0,
            elapsed: t,
            frame_count: 0,
            fps: 0.0,
        });
        self.animator_system.run(&mut self.world.ecs, &mut resources);

        if self.gpu.mode != RendererMode::Pure2D {
            self.gpu.sync_world(&self.world);
        }
        self.sync_ecs_to_shape_batcher();

        let mut encoder = self.gpu.begin_frame();
        self.gpu.render_to_internal_target(&mut encoder, None);

        if let Some(readback) = &self.gpu.readback_manager {
            readback.copy_to_buffer(&mut encoder, self.gpu.render_target.color_texture());
            self.gpu.context.queue.submit(Some(encoder.finish()));

            #[cfg(not(target_arch = "wasm32"))]
            {
                use pollster::FutureExt;
                return readback.poll_and_map_into(&self.gpu.context.device, out).block_on().is_ok();
            }
        }
        false
    }

    /// Updates the endpoints of an existing 2D line without removing/re-spawning it.
    /// Useful for line-progress animations where the line grows from start to end.
    pub fn update_line_endpoints(&mut self, node: NodeId, ax: f32, ay: f32, bx: f32, by: f32) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mut elem) = self.world.ecs.get_mut::<Element>(*entity) {
                if let ElementKind::Line2D { x0, y0, x1, y1, .. } = &mut elem.kind {
                    *x0 = ax;
                    *y0 = ay;
                    *x1 = bx;
                    *y1 = by;
                    self.gpu.mark_dirty();
                    return Ok(());
                }
            }
        }
        Err(())
    }

    /// Sets the line cap style for a 2D Line2D entity.
    /// cap: 0 = Flat, 1 = Round, 2 = Square.
    pub fn set_line_cap(&mut self, node: NodeId, cap: u8) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mut elem) = self.world.ecs.get_mut::<Element>(*entity) {
                elem.line_cap_style = cap.min(2);
                self.gpu.mark_dirty();
                return Ok(());
            }
        }
        Err(())
    }

    /// Sets the stroke style for a 2D entity.
    pub fn set_stroke(&mut self, node: NodeId, color: [f32; 4], thickness: f32) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mut elem) = self.world.ecs.get_mut::<Element>(*entity) {
                elem.stroke_color = Some(color);
                elem.stroke_thickness = thickness;
                let is_path = matches!(elem.kind, ElementKind::Path);
                if let ElementKind::Line2D { thickness: ref mut t, .. } = elem.kind {
                    *t = thickness;
                }
                // v15: Sync with material for proper rendering
                // Only sync if not a Path, since Paths embed color in geometry.
                if !is_path {
                    elem.material.descriptor.base_color = color;
                    if let Some(mut m) = self.world.ecs.get_mut::<MaterialComponent>(*entity) {
                        m.descriptor.base_color = color;
                    }
                }
                self.gpu.mark_dirty();
                return Ok(());
            }
        }
        Err(())
    }

    /// Removes fill color, useful for open paths like Plots
    pub fn remove_fill(&mut self, node: NodeId) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mut elem) = self.world.ecs.get_mut::<Element>(*entity) {
                elem.fill_color = None;
                self.gpu.mark_dirty();
                return Ok(());
            }
        }
        Err(())
    }

    /// Sets the fill color for a 2D entity.
    pub fn set_fill(&mut self, node: NodeId, color: [f32; 4]) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mut elem) = self.world.ecs.get_mut::<Element>(*entity) {
                elem.fill_color = Some(color);
                let is_path = matches!(elem.kind, ElementKind::Path);
                // v15: Sync with material for proper rendering
                // Only sync if not a Path, since Paths embed color in geometry.
                if !is_path {
                    elem.material.descriptor.base_color = color;
                    if let Some(mut m) = self.world.ecs.get_mut::<MaterialComponent>(*entity) {
                        m.descriptor.base_color = color;
                    }
                }
                self.gpu.mark_dirty();
                return Ok(());
            }
        }
        Err(())
    }

    /// Set Z-Index for 2D ordering.
    pub fn set_z_index(&mut self, node: NodeId, z: i32) -> Result<(), ()> {
        use ferrous_core::scene::world::types::ZIndex;
        let e = self.node_map.get(&node).ok_or(())?;
        self.world.ecs.insert(*e, ZIndex(z));
        self.gpu.mark_dirty();
        Ok(())
    }

    /// Sets the visibility of a node.
    pub fn set_visible(&mut self, node: NodeId, visible: bool) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(el) = self.world.ecs.get_mut::<Element>(*entity) {
                el.visible = visible;
                self.gpu.mark_dirty();
                return Ok(());
            }
        }
        Err(())
    }

    /// Sets the opacity of a specific node.
    pub fn set_opacity(&mut self, node: NodeId, opacity: f32) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mat) = self.world.ecs.get_mut::<MaterialComponent>(*entity) {
                mat.descriptor.opacity = opacity;
                mat.descriptor.base_color[3] = opacity;
                if opacity < 0.999 {
                    mat.descriptor.alpha_mode = ferrous_core::scene::AlphaMode::Blend;
                }
            }
            if let Some(el) = self.world.ecs.get_mut::<Element>(*entity) {
                el.material.descriptor.opacity = opacity;
                el.material.descriptor.base_color[3] = opacity;
            }
            self.gpu.mark_dirty();
            return Ok(());
        }
        Err(())
    }

    pub fn on_resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
    }

    pub fn width(&self) -> u32 { self.gpu.width() }
    pub fn height(&self) -> u32 { self.gpu.height() }
}

// ── Path classification helpers ───────────────────────────────────────────

/// Detects a circle approximation path: MoveTo(r,0) + 4 CubicTo + Close.
/// Returns the radius if the path matches.
fn classify_circle_path(cmds: &[PathCommand]) -> Option<f32> {
    if cmds.len() != 6 { return None; }
    let PathCommand::MoveTo(start) = &cmds[0] else { return None };
    let PathCommand::Close = &cmds[5] else { return None };

    let r = start.x.abs();
    if r < 0.001 { return None; }
    if start.y.abs() > 0.001 { return None; }

    let expected_ends = [
        glam::Vec2::new(0.0, r),
        glam::Vec2::new(-r, 0.0),
        glam::Vec2::new(0.0, -r),
        glam::Vec2::new(r, 0.0),
    ];

    for (i, expected) in expected_ends.iter().enumerate() {
        if let PathCommand::CubicTo(_, _, end) = &cmds[i + 1] {
            if (end - expected).length() > 0.01 {
                return None;
            }
        } else {
            return None;
        }
    }

    Some(r)
}

/// Detects a rectangle approximation path: MoveTo(-hw,-hh) + 4 degenerate
/// CubicTo (where start==end for each edge) + Close.
/// Returns (width, height) if the path matches.
fn classify_rect_path(cmds: &[PathCommand]) -> Option<(f32, f32)> {
    if cmds.len() != 6 { return None; }
    let PathCommand::MoveTo(start) = &cmds[0] else { return None };
    let PathCommand::Close = &cmds[5] else { return None };

    let hw = -start.x;
    let hh = -start.y;
    if hw <= 0.001 || hh <= 0.001 { return None; }

    let expected_ends = [
        glam::Vec2::new(hw, -hh),
        glam::Vec2::new(hw, hh),
        glam::Vec2::new(-hw, hh),
        glam::Vec2::new(-hw, -hh),
    ];

    for (i, expected) in expected_ends.iter().enumerate() {
        if let PathCommand::CubicTo(_, _, end) = &cmds[i + 1] {
            if (end - expected).length() > 0.01 {
                return None;
            }
        } else {
            return None;
        }
    }

    Some((hw * 2.0, hh * 2.0))
}
