use ferrous_gpu::EngineContext;
use ferrous_renderer::Renderer;
use winit::window::Window;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

pub struct GraphicsState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: Box<Renderer>,
}

impl GraphicsState {
    pub async fn new(
        window: &Window,
        width: u32,
        height: u32,
        vsync: bool,
        sample_count: u32,
        hdri_path: Option<String>,
    ) -> Self {
        #[cfg(target_arch = "wasm32")]
        let backends = {
            let win = web_sys::window().expect("no window");
            let nav = win.navigator();
            let is_secure_context = win.is_secure_context();
            
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[WGPU-Init] Secure Context: {}", is_secure_context)));
            
            // Si el origen no es seguro (IP sin HTTPS), Chrome bloquea WebGPU.
            if !is_secure_context {
                web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str("[WGPU-Init] Origin is insecure. WebGPU will be disabled by the browser. Falling back to WebGL2..."));
            }

            wgpu::Backends::all()
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            #[cfg(target_arch = "wasm32")]
            backends,
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
            
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[WGPU-Init] Got canvas from winit window"));
            
            let web_window = web_sys::window().expect("no window");
            let document = web_window.document().expect("no document");

            // ── HiDPI: set the canvas buffer to physical pixels ──────────────
            // `width`/`height` CSS controls how the canvas LOOKS on screen, but
            // the actual GPU render buffer size is the canvas element attribute.
            // Winit passes physical pixels via `with_inner_size(PhysicalSize{..})`,
            // so `width`/`height` here are already DPR-scaled physical pixels.
            canvas.set_width(width);
            canvas.set_height(height);
            let dpr = web_window.device_pixel_ratio();
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
                "[WGPU-Init] Canvas buffer = {}x{} physical pixels (DPR={:.2})",
                width, height, dpr
            )));
            
            // Si el usuario proporcionó un contenedor específico, mover la canvas ahí
            if let Some(container) = document.get_element_by_id("ferrous-container") {
                 container.append_child(&canvas).ok();
            } else {
                 let body = document.body().expect("no body");
                 if canvas.parent_element().is_none() {
                     body.append_child(&canvas).expect("failed to append canvas to document body");
                     web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[WGPU-Init] Appended canvas to document body (Fallback)"));
                     
                     // Si cae al body, forzamos posición fija arriba para que no se desplace abajo
                     canvas.style().set_property("position", "fixed").ok();
                     canvas.style().set_property("top", "0").ok();
                     canvas.style().set_property("left", "0").ok();
                     canvas.style().set_property("z-index", "0").ok();
                 }
            }
            
            // CSS size = 100% of container. The browser scales our physical-pixel
            // buffer back down to CSS pixels, producing a crisp HiDPI image.
            canvas.style().set_property("width", "100%").ok();
            canvas.style().set_property("height", "100%").ok();
            canvas.style().set_property("image-rendering", "auto").ok();
            canvas.style().set_property("display", "block").ok();
            canvas.style().set_property("outline", "none").ok();
            
            // Critical for keyboard input in the browser: 
            // the canvas needs a tabindex to be focusable.
            canvas.set_attribute("tabindex", "0").ok();
            canvas.focus().ok();

            // Auto-focus on click to capture keyboard input (often blocked programmatically without interactions)
            let focus_canvas = canvas.clone();
            let on_pointer_down = wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
                focus_canvas.focus().ok();
            });
            canvas.add_event_listener_with_callback("pointerdown", on_pointer_down.as_ref().unchecked_ref()).ok();
            on_pointer_down.forget();

            // Disable browser context menu to allow right-click interaction in 3D
            let on_context_menu = wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|e: web_sys::MouseEvent| {
                e.prevent_default();
            });
            canvas.add_event_listener_with_callback("contextmenu", on_context_menu.as_ref().unchecked_ref()).ok();
            on_context_menu.forget();
        }

        let surface = instance.create_surface(window).unwrap_or_else(|e| {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!("Failed to create surface: {:?}", e)));
            panic!("Failed to create surface: {:?}", e);
        });
        
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[WGPU-Init] Surface created perfectly"));
        
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };

        // Build the context with the surface so the adapter selection is
        // surface-compatible (avoids costly cross-bus present paths).
        let context = match EngineContext::new_with_instance(instance, Some(&surface)).await {
            Ok(c) => c,
            Err(e) => {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!("EngineContext init failed: {:?}", e)));
                panic!("EngineContext init failed: {:?}", e);
            }
        };

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[WGPU-Init] Engine context created! Adapter: {:?}", context.adapter.get_info().name)));

        let caps = surface.get_capabilities(&context.adapter);
        // Prefer a non-sRGB format so that GUI colours are linear-exact:
        // writing [0.235, 0.235, 0.235] will produce #3C3C3C on screen.
        // With an sRGB surface wgpu would apply an implicit gamma conversion,
        // making every colour appear ~2× brighter than intended.
        // Fall back to the first advertised format if nothing linear is found.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                !f.is_srgb()
                    && matches!(
                        f,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                    )
            })
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
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
        
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[WGPU-Init] Configuring surface: {}x{} format={:?}", config.width, config.height, config.format)));
        
        surface.configure(&context.device, &config);

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[WGPU-Init] Surface configured. Creating Renderer..."));

        let renderer = Box::new(Renderer::new(
            context,
            config.width,
            config.height,
            config.format,
            sample_count,
            // convert Option<String> -> Option<&Path>
            hdri_path.as_ref().map(std::path::Path::new),
        ));

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[WGPU-Init] Renderer created successfully!"));

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
