// ferrous_renderer: biblioteca principal de renderizado

pub mod camera;
pub mod mesh;
pub mod pipeline;
pub mod render_target;

use crate::pipeline::FerrousPipeline;
use crate::render_target::RenderTarget;
use ferrous_gui::TextBatch;
use ferrous_core::context::EngineContext;
use wgpu::util::DeviceExt;
// re-export UI types so callers de-referencing the renderer can use them
pub use ferrous_gui::{GuiBatch, GuiQuad};

/// Rectangle region used for 3D rendering and input checks.
#[derive(Copy, Clone, Debug)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Estructura de más alto nivel que orquesta el renderizado.
///
/// Contiene el contexto de la GPU, el `RenderTarget` y el pipeline encargado
/// de dibujar la escena. Provee métodos para comenzar un frame y ejecutar la
/// secuencia de dibujo mínima.
pub struct Renderer {
    /// contexto compartido de WGPU
    pub context: EngineContext,
    /// destino en el que se renderiza
    pub render_target: RenderTarget,
    pipeline: FerrousPipeline,
    /// motor de la interfaz de usuario que se dibuja encima
    ui_renderer: ferrous_gui::GuiRenderer,
    /// dimensiones actuales del render target
    width: u32,
    height: u32,
    /// camera state used for the 3D scene
    pub camera: camera::Camera,
    camera_uniform: camera::CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// simple scene mesh (cube)
    pub mesh: mesh::Mesh,
    /// region within the window where 3D content is drawn
    pub viewport: Viewport,
    /// orbital camera state
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Renderer {
    /// Crea un `Renderer` inicializando el render target y el pipeline.
    pub fn new(
        context: EngineContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let rt = RenderTarget::new(&context.device, width, height, format);
        let pipe = FerrousPipeline::new(&context.device, format);
        let ui = ferrous_gui::GuiRenderer::new(context.device.clone(), format, 1024, width, height);

        // create camera resources
        let camera = camera::Camera {
            eye: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fovy: 45.0f32.to_radians(),
            aspect: width as f32 / height as f32,
            znear: 0.1,
            zfar: 100.0,
        };
        let mut camera_uniform = camera::CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        let camera_buffer = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Camera Uniform Buffer"),
                contents: bytemuck::bytes_of(&camera_uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let camera_bind_group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Camera Bind Group"),
                layout: &pipe.camera_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
            });

        // simple cube mesh for testing
        let mesh = mesh::Mesh::cube(&context.device);
        // default viewport is full render target
        let viewport = Viewport {
            x: 0,
            y: 0,
            width,
            height,
        };
        let yaw = 0.0;
        let pitch = 0.0;
        let distance = 5.0;
        Self {
            context,
            render_target: rt,
            pipeline: pipe,
            ui_renderer: ui,
            width,
            height,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            mesh,
            viewport,
            yaw,
            pitch,
            distance,
        }
    }

    /// Inicia un nuevo frame devolviendo el encoder de comandos que se
    /// utilizará para grabar operaciones GPU.
    pub fn begin_frame(&mut self) -> wgpu::CommandEncoder {
        self.context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            })
    }

    /// Dibuja el contenido al render target usando el encoder proporcionado.
    ///
    /// Se crea un `RenderPass` que limpia color y profundidad y emite un
    /// `draw(0..3,0..1)` para el triángulo del shader.
    /// Dibuja la escena 3D y, opcionalmente, la interfaz de usuario encima.
    ///
    /// El parámetro `ui_batch` permite al llamador pasar un lote de quads
    /// que serán compositados sobre el color target después de renderizar
    /// la escena. Si no se proporciona, sólo se realiza el pase 3D.
    pub fn render_to_target(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ui_batch: Option<&ferrous_gui::GuiBatch>,
        text_batch: Option<&TextBatch>,
    ) {
        // update camera uniform prior to borrowing any fields
        self.update_camera_buffer();
        let color_view = &self.render_target.color_view;
        let depth_view = &self.render_target.depth_view;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        rpass.set_pipeline(&self.pipeline.pipeline);
        // bind camera uniform group at index 0
        rpass.set_bind_group(0, &self.camera_bind_group, &[]);
        // bind mesh buffers and issue indexed draw
        rpass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        rpass.draw_indexed(0..self.mesh.index_count, 0, 0..1);

        drop(rpass); // cerrar el pase 3D antes de iniciar el pase UI

        if let Some(batch) = ui_batch {
            // renderizamos la UI en un pase separado que no limpia nada
            self.ui_renderer
                .render(encoder, color_view, batch, &self.context.queue, text_batch);
        }
    }

    /// Renders the scene directly into an arbitrary texture view (typically
    /// the current swapchain frame) instead of the internal render target.
    ///
    /// This is useful when the caller already has a `TextureView` from a
    /// `Surface` and wants the triangle/UI to appear on screen.
    pub fn render_to_view(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_batch: Option<&ferrous_gui::GuiBatch>,
        text_batch: Option<&TextBatch>,
    ) {
        // reuse most of the same logic as `render_to_target`, but render
        // directly into the provided view. we still supply a depth attachment
        // from our internal render target so that the pipeline's depth format
        // matches what it was created with.
        // update camera before drawing, do this before borrowing depth_view
        self.update_camera_buffer();
        let depth_view = &self.render_target.depth_view;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass (swapchain)"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        rpass.set_pipeline(&self.pipeline.pipeline);
        rpass.set_bind_group(0, &self.camera_bind_group, &[]);
        // restrict 3D drawing to viewport area
        let vp = self.viewport;
        rpass.set_scissor_rect(vp.x, vp.y, vp.width, vp.height);
        rpass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        rpass.draw_indexed(0..self.mesh.index_count, 0, 0..1);
        drop(rpass);

        if let Some(batch) = ui_batch {
            self.ui_renderer
                .render(encoder, view, batch, &self.context.queue, text_batch);
        }
    }

    /// Provide font atlas data to the internal GUI renderer.
    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.ui_renderer.set_font_atlas(view, sampler);
    }

    /// Cambia el tamaño del render target y actualiza el renderer de UI.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        self.render_target
            .resize(&self.context.device, new_width, new_height);
        self.ui_renderer
            .resize(&self.context.queue, new_width, new_height);
        self.width = new_width;
        self.height = new_height;
        // if the viewport covered the full previous window, stretch it too
        if self.viewport.width == self.width && self.viewport.height == self.height {
            self.viewport.width = new_width;
            self.viewport.height = new_height;
            self.camera.set_aspect(new_width as f32 / new_height as f32);
        }
    }

    /// Explicitly set the 3D viewport rectangle. This will resize the internal
    /// render target to match the viewport dimensions and adjust the camera
    /// projection aspect accordingly.
    pub fn set_viewport(&mut self, vp: Viewport) {
        self.viewport = vp;
        // camera projection should use viewport aspect ratio
        self.camera.set_aspect(vp.width as f32 / vp.height as f32);
    }

    /// Write the current camera uniform values to the GPU buffer.
    fn update_camera_buffer(&mut self) {
        self.camera_uniform.update_view_proj(&self.camera);
        self.context.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera_uniform),
        );
    }

    /// Handle user input to modify the camera position. `dt` is the elapsed
    /// time since the last call in seconds.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        use ferrous_core::input::KeyCode;
        // translate along camera-relative axes
        let mut move_dir = glam::Vec3::ZERO;
        if input.is_key_pressed(KeyCode::KeyW) {
            move_dir.z += 1.0; // forward along view direction
        }
        if input.is_key_pressed(KeyCode::KeyS) {
            move_dir.z -= 1.0;
        }
        if input.is_key_pressed(KeyCode::KeyA) {
            move_dir.x -= 1.0;
        }
        if input.is_key_pressed(KeyCode::KeyD) {
            move_dir.x += 1.0;
        }
        if move_dir.length_squared() > 0.0 {
            let forward = (self.camera.target - self.camera.eye).normalize();
            let right = forward.cross(self.camera.up).normalize();
            let world_disp = (forward * move_dir.z + right * move_dir.x).normalize();
            let speed = 5.0;
            let displacement = world_disp * speed * dt;
            self.camera.eye += displacement;
            self.camera.target += displacement; // move target with eye
        }

        // handle mouse orbit when right button held
        if input.is_button_down(ferrous_core::input::MouseButton::Right) {
            let (dx, dy) = input.consume_mouse_delta();
            let sensitivity = 0.005;
            self.yaw -= dx * sensitivity; // invert horizontal drag
            self.pitch -= dy * sensitivity; // invert vertical drag
                                            // clamp pitch to avoid flipping
            let limit = std::f32::consts::FRAC_PI_2 - 0.01;
            self.pitch = self.pitch.clamp(-limit, limit);
            // recompute camera eye relative to target
            let rot = glam::Mat3::from_euler(glam::EulerRot::YXZ, self.yaw, self.pitch, 0.0);
            let offset = rot * glam::Vec3::new(0.0, 0.0, self.distance);
            self.camera.eye = self.camera.target + offset;
        }
    }
}
