use ferrous_core::context::EngineContext;
use ferrous_renderer::Renderer;
use winit::window::Window;

pub struct GraphicsState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: Renderer,
}

impl GraphicsState {
    pub async fn new(window: &Window, width: u32, height: u32, vsync: bool) -> Self {
        let context = EngineContext::new().await.unwrap();
        let surface = context.instance.create_surface(window).unwrap();
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };

        let caps = surface.get_capabilities(&context.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let present_mode = if vsync {
            wgpu::PresentMode::Fifo
        } else {
            wgpu::PresentMode::AutoNoVsync
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&context.device, &config);

        let renderer = Renderer::new(context, config.width, config.height, config.format);

        Self {
            surface,
            config,
            renderer,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface
                .configure(&self.renderer.context.device, &self.config);
            self.renderer.resize(width, height);
        }
    }
}
