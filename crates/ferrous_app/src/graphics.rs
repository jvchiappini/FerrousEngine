use ferrous_core::context::EngineContext;
use ferrous_renderer::Renderer;
use winit::window::Window;

pub struct GraphicsState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: Renderer,
}

impl GraphicsState {
    pub async fn new(
        window: &Window,
        width: u32,
        height: u32,
        vsync: bool,
        sample_count: u32,
    ) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // On wasm32 we must request the WebGPU (or WebGL2) backend.
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // ── wasm32: attach the canvas before creating the surface ─────────────
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            let canvas = window
                .canvas()
                .expect("winit window has no canvas in wasm32");
            let web_window = web_sys::window().unwrap();
            let document = web_window.document().unwrap();
            let body = document.body().unwrap();
            // Append the canvas to <body> if it isn't already there.
            if canvas.parent_element().is_none() {
                body.append_child(&canvas)
                    .expect("failed to append canvas to document body");
            }
        }

        let surface = instance.create_surface(window).unwrap();
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };

        // Build the context with the surface so the adapter selection is
        // surface-compatible (avoids costly cross-bus present paths).
        let context = EngineContext::new_with_instance(instance, Some(&surface))
            .await
            .unwrap();

        let caps = surface.get_capabilities(&context.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        // PresentMode::Fifo        = hard vsync (locked to monitor refresh)
        // PresentMode::AutoNoVsync = fully uncapped, no tearing guarantee — works on all backends
        // Note: Mailbox is avoided because on NVIDIA/Vulkan it can still be
        // driver-capped to the monitor refresh rate, behaving like vsync.
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
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&context.device, &config);

        let renderer = Renderer::new(
            context,
            config.width,
            config.height,
            config.format,
            sample_count,
        );

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
