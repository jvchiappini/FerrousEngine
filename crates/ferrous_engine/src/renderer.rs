use std::collections::HashMap;
use ferrous_core::{World, Transform, Color};
use ferrous_core::api::types::NodeId;
use ferrous_ecs::prelude::Entity;
use ferrous_ecs::system::System;
use ferrous_gpu::EngineContext;
use ferrous_renderer::{Renderer as GpuRenderer};
use ferrous_core::scene::{Material, Billboard, BillboardMode, ShadowCaster, Element, ElementKind, AnimatorSystem};
use ferrous_core::scene::world::MaterialComponent;
use ferrous_core::glam;

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
            1,
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

        // 1. Sync ECS world to GPU
        self.gpu.sync_world(&self.world);

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

    /// Sets the stroke style for a 2D entity.
    pub fn set_stroke(&mut self, node: NodeId, color: [f32; 4], thickness: f32) -> Result<(), ()> {
        if let Some(entity) = self.node_map.get(&node) {
            if let Some(mut elem) = self.world.ecs.get_mut::<Element>(*entity) {
                elem.stroke_color = Some(color);
                elem.stroke_thickness = thickness;
                if let ElementKind::Line2D { thickness: ref mut t, .. } = elem.kind {
                    *t = thickness;
                }
                // v15: Sync with material for proper rendering
                elem.material.descriptor.base_color = color;
                if let Some(mut m) = self.world.ecs.get_mut::<MaterialComponent>(*entity) {
                    m.descriptor.base_color = color;
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
            if let Some(elem) = self.world.ecs.get_mut::<Element>(*entity) {
                elem.fill_color = Some(color);
                // v15: Sync with material for proper rendering
                elem.material.descriptor.base_color = color;
                if let Some(mut m) = self.world.ecs.get_mut::<MaterialComponent>(*entity) {
                    m.descriptor.base_color = color;
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
