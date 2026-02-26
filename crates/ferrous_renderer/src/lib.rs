// ferrous_renderer: biblioteca principal de renderizado

pub mod pipeline;
pub mod render_target;

use crate::pipeline::FerrousPipeline;
use crate::render_target::RenderTarget;
use ferrous_core::context::EngineContext;
// re-export UI types so callers de-referencing the renderer can use them
pub use ferrous_gui::{GuiBatch, GuiQuad};

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
        Self {
            context,
            render_target: rt,
            pipeline: pipe,
            ui_renderer: ui,
            width,
            height,
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
    ) {
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

        drop(rpass); // cerrar el pase 3D antes de iniciar el pase UI

        if let Some(batch) = ui_batch {
            // renderizamos la UI en un pase separado que no limpia nada
            self.ui_renderer
                .render(encoder, color_view, batch, &self.context.queue);
        }
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
    }
}
