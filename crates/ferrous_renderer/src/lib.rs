// ferrous_renderer: biblioteca principal de renderizado

pub mod pipeline;
pub mod render_target;

use crate::pipeline::FerrousPipeline;
use crate::render_target::RenderTarget;
use ferrous_core::context::EngineContext;

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
        Self {
            context,
            render_target: rt,
            pipeline: pipe,
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
    pub fn render_to_target(&self, encoder: &mut wgpu::CommandEncoder) {
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
        rpass.draw(0..3, 0..1);
    }
}
