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
        // Create the instance first so we can create the surface before the
        // adapter — this lets wgpu pick the adapter that actually supports
        // presenting to our window (critical on multi-GPU systems).
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

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
        // PresentMode::Fifo  = hard vsync (monitor refresh rate, lowest GPU usage)
        // PresentMode::Mailbox = no tearing, but uncapped GPU (use only for benchmarks)
        // PresentMode::AutoNoVsync = fully uncapped — high GPU usage even at low FPS
        //
        // We always prefer Fifo (or FifoRelaxed when available) because the
        // target_fps limiter on the CPU side is not enough: the GPU executes
        // queued work *during* the CPU sleep, so AutoNoVsync causes high GPU
        // usage regardless of how slow the CPU loop runs.
        let available = caps.present_modes.as_slice();
        let present_mode = if vsync {
            wgpu::PresentMode::Fifo
        } else if available.contains(&wgpu::PresentMode::Mailbox) {
            // Mailbox: no tearing, GPU is rate-limited by the swapchain queue
            // (at most 1 frame ahead), much lower GPU usage than AutoNoVsync.
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            // 1 = only one frame queued ahead; minimises GPU pre-work and
            // therefore GPU usage when the CPU is sleeping between frames.
            desired_maximum_frame_latency: 1,
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
