//! GPU state and startup policy.
//!
//! Three startup rules live here (prompt 17 S3):
//!
//! - **Backend order**: one instance per configured backend, in order, moving
//!   on only when no usable adapter appears. On Windows that means Vulkan alone
//!   in the common case instead of paying for DX12 and GL enumeration.
//! - **Overlap**: [`GpuProbeHandle`] creates the instance and picks the adapter
//!   on a worker while winit creates the window; the surface is bound when the
//!   window exists.
//! - **Pipeline cache**: the wgpu cache is seeded from a file keyed by adapter
//!   and driver, and written back after the first frame and at exit.
//!
//! The window itself is painted with the app background colour before any GPU
//! work (see [`crate::window_style`]), so it never flashes white.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use ferrous_gpu::EngineContext;
use ferrous_renderer::{Renderer, RendererMode};
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use crate::builder::{AppConfig, PipelineCacheConfig};
use ferrous_core::startup;

/// Version of the pipeline cache file this build writes.
#[cfg(not(target_arch = "wasm32"))]
const PIPELINE_CACHE_FORMAT: u16 = 1;
/// Magic that opens a pipeline cache file.
#[cfg(not(target_arch = "wasm32"))]
const PIPELINE_CACHE_MAGIC: &[u8; 4] = b"FPCA";

/// GPU resources for one window.
pub struct GraphicsState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: Box<Renderer>,
    /// Where the pipeline cache is persisted, when the app asked for it.
    #[cfg(not(target_arch = "wasm32"))]
    pipeline_cache: Option<PipelineCacheFile>,
    /// Whether the cache was written after the first frame.
    #[cfg(not(target_arch = "wasm32"))]
    pipeline_cache_saved: bool,
}

/// The GPU initialisation one window needs, copied from [`AppConfig`].
#[derive(Clone)]
pub(crate) struct GpuInit {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) vsync: bool,
    pub(crate) sample_count: u32,
    pub(crate) hdri_path: Option<String>,
    pub(crate) backends: Vec<wgpu::Backends>,
    pub(crate) power_preference: wgpu::PowerPreference,
    pub(crate) pipeline_cache: Option<PipelineCacheConfig>,
    pub(crate) background: wgpu::Color,
    /// Renderer mode the window starts in, so 2-D apps never build 3-D
    /// resources at startup (prompt 17 S4).
    pub(crate) mode: RendererMode,
}

#[cfg(not(target_arch = "wasm32"))]
impl GpuInit {
    /// Reads the parts of `config` the GPU initialisation needs.
    pub(crate) fn from_config(config: &AppConfig) -> Self {
        Self {
            width: config.width,
            height: config.height,
            vsync: config.vsync,
            sample_count: config.sample_count,
            hdri_path: config.hdri_path.clone(),
            backends: config.backends.clone(),
            power_preference: config.power_preference,
            pipeline_cache: config.pipeline_cache.clone(),
            background: config.background_color.to_wgpu(),
            mode: renderer_mode_from(config.mode),
        }
    }

    /// The backend attempts for this launch; `WGPU_BACKEND` wins.
    pub(crate) fn backend_order(&self) -> Vec<wgpu::Backends> {
        backend_order_from(&self.backends, wgpu::util::backend_bits_from_env())
    }

    /// The power preference for this launch; `WGPU_POWER_PREF` wins.
    pub(crate) fn power_preference(&self) -> wgpu::PowerPreference {
        wgpu::util::power_preference_from_env().unwrap_or(self.power_preference)
    }
}

/// The renderer mode an [`AppMode`](crate::builder::AppMode) maps to.
///
/// `Flat2D` apps (GUI tools, editors) construct their renderer without any 3-D
/// resource; a later `Renderer::enable_full_3d` builds them on demand.
#[must_use]
pub fn renderer_mode_from(app_mode: crate::builder::AppMode) -> RendererMode {
    match app_mode {
        crate::builder::AppMode::Flat2D => RendererMode::Flat2D,
        crate::builder::AppMode::Game3D => RendererMode::Full3D,
    }
}

/// Order of the backend attempts, honouring an explicit override.
///
/// An empty configured list falls back to the platform default; an override
/// (from `WGPU_BACKEND`) replaces the list with exactly those backends.
#[must_use]
pub fn backend_order_from(
    configured: &[wgpu::Backends],
    override_backends: Option<wgpu::Backends>,
) -> Vec<wgpu::Backends> {
    if let Some(backends) = override_backends {
        if !backends.is_empty() {
            return vec![backends];
        }
    }
    if configured.is_empty() {
        crate::builder::default_backends()
    } else {
        configured.to_vec()
    }
}

/// Adapter features the engine renderer needs to build its pipelines.
///
/// The GUI image pass (enabled by the ssets feature) samples texture arrays,
/// which wgpu's GL backend does not expose; a backend that lacks them cannot
/// run the engine and is skipped instead of panicking inside validation.
#[cfg(feature = "assets")]
const ENGINE_ADAPTER_FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY
    .union(wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING);
#[cfg(not(feature = "assets"))]
const ENGINE_ADAPTER_FEATURES: wgpu::Features = wgpu::Features::empty();

/// The wgpu version's cache format, mixed into the file header.
#[cfg(not(target_arch = "wasm32"))]
const ENGINE_CACHE_VERSION: u32 = 1;

/// File name key of a pipeline cache: backend, adapter and driver.
///
/// A driver update, a GPU change or another backend starts from an empty cache
/// instead of feeding wgpu bytes from a different compiler.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pipeline_cache_key(info: &wgpu::AdapterInfo) -> String {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut mix = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u128::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    mix(&ENGINE_CACHE_VERSION.to_le_bytes());
    mix(&(info.backend as u32).to_le_bytes());
    mix(&info.vendor.to_le_bytes());
    mix(&info.device.to_le_bytes());
    mix(info.driver.as_bytes());
    mix(info.driver_info.as_bytes());
    format!("{hash:032x}")
}

/// Whether an adapter exposes everything the engine renderer needs.
///
/// A backend that cannot provide [`ENGINE_ADAPTER_FEATURES`] (GL in wgpu 23
/// lacks texture binding arrays) is skipped with a warning instead of failing
/// later inside a `create_bind_group_layout` validation.
fn adapter_is_usable(adapter: &wgpu::Adapter) -> bool {
    let missing = ENGINE_ADAPTER_FEATURES.difference(adapter.features());
    if !missing.is_empty() {
        log::warn!(
            "adapter {} cannot run the engine: missing {missing:?}",
            adapter.get_info().name
        );
    }
    missing.is_empty()
}

/// One persisted pipeline cache in the local app cache directory.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct PipelineCacheFile {
    path: PathBuf,
    key: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl PipelineCacheFile {
    /// The cache file for `info`, when the app enabled the cache.
    fn for_adapter(config: &PipelineCacheConfig, info: &wgpu::AdapterInfo) -> Option<Self> {
        let directory = config.resolve_directory()?;
        let key = pipeline_cache_key(info);
        Some(Self {
            path: directory.join(format!("{key}.bin")),
            key,
        })
    }

    /// Loads the payload, or `None` when the file is missing, foreign or
    /// corrupt. wgpu would ignore bad bytes anyway (`fallback: true`); the
    /// header check keeps obviously wrong files out of it.
    fn load(&self) -> Option<Vec<u8>> {
        let bytes = std::fs::read(&self.path).ok()?;
        if bytes.len() < PIPELINE_CACHE_MAGIC.len() + 2 + 4 + 32 {
            return None;
        }
        if &bytes[..4] != PIPELINE_CACHE_MAGIC {
            return None;
        }
        let format = u16::from_le_bytes([bytes[4], bytes[5]]);
        if format != PIPELINE_CACHE_FORMAT {
            return None;
        }
        let length = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
        let key_end = 10 + 32;
        if bytes.get(10..key_end)? != self.key.as_bytes() {
            return None;
        }
        let payload = bytes.get(key_end..key_end + length)?;
        if payload.len() != length {
            return None;
        }
        Some(payload.to_vec())
    }

    /// Writes the payload atomically (temporary file plus rename).
    fn save(&self, data: &[u8]) {
        let Some(parent) = self.path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let mut bytes = Vec::with_capacity(10 + 32 + data.len());
        bytes.extend_from_slice(PIPELINE_CACHE_MAGIC);
        bytes.extend_from_slice(&PIPELINE_CACHE_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.key.as_bytes());
        bytes.extend_from_slice(data);
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        if std::fs::write(&temporary, &bytes).is_err() {
            let _ = std::fs::remove_file(&temporary);
            return;
        }
        if std::fs::rename(&temporary, &self.path).is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
    }
}

/// An instance and adapter chosen before the window existed.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct GpuProbe {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
}

/// Handle to the worker that creates the instance and picks the adapter.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct GpuProbeHandle {
    thread: Option<std::thread::JoinHandle<Option<GpuProbe>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl GpuProbeHandle {
    /// Starts the probe on a worker thread.
    ///
    /// Creating the Vulkan instance and enumerating adapters does not need the
    /// window, so it runs while winit creates it. The adapter is requested
    /// without a surface; the main thread re-checks it against the real surface
    /// and re-probes when it cannot present there.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn start(init: &GpuInit) -> Self {
        let order = init.backend_order();
        let power = init.power_preference();
        let thread = std::thread::Builder::new()
            .name("ferrous-gpu".to_owned())
            .spawn(move || {
                for backends in order {
                    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                        backends,
                        ..Default::default()
                    });
                    let Ok(adapter) = pollster::block_on(EngineContext::request_adapter_with(
                        &instance, None, power,
                    )) else {
                        continue;
                    };
                    startup::mark("engine: instance");
                    startup::mark("engine: adapter");
                    return Some(GpuProbe { instance, adapter });
                }
                None
            })
            .ok();
        Self { thread }
    }

    /// Waits for the probe; `None` when the worker failed or found no adapter.
    pub(crate) fn join(self) -> Option<GpuProbe> {
        self.thread?.join().ok().flatten()
    }
}

/// Everything the renderer needs, once the device and swapchain exist.
///
/// Keeping the slow `Renderer::new` out of [`GraphicsState::init_device`] lets
/// the runner build it on a worker: the event loop keeps running, the window
/// stays hidden until the first frame is ready, and the safety timeout can
/// still show the app background (prompt 17 S3b).
pub(crate) struct DeviceInit {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    context: Option<EngineContext>,
    cache_file: Option<PipelineCacheFile>,
    sample_count: u32,
    hdri_path: Option<String>,
    mode: RendererMode,
}

impl DeviceInit {
    /// Builds the renderer — the part that can take seconds on a cold driver —
    /// and returns the device parts with it.
    pub(crate) fn build_renderer(mut self) -> (Self, Box<Renderer>) {
        let Some(context) = self.context.take() else {
            panic!("the renderer was already built for this device");
        };
        let renderer = Box::new(Renderer::new_with_mode(
            context,
            self.config.width,
            self.config.height,
            self.config.format,
            self.sample_count,
            self.hdri_path.as_deref().map(std::path::Path::new),
            self.mode,
        ));
        startup::mark("engine: renderer built");
        (self, renderer)
    }

    /// The complete GPU state once the renderer exists.
    pub(crate) fn into_state(self, renderer: Box<Renderer>) -> GraphicsState {
        GraphicsState {
            surface: self.surface,
            config: self.config,
            renderer,
            pipeline_cache: self.cache_file,
            pipeline_cache_saved: false,
        }
    }
}

impl GraphicsState {
    /// Desktop initialisation up to the device and the swapchain (the fast
    /// part, on the caller's thread).
    ///
    /// Uses `probe` when it can present on the window, otherwise selects an
    /// adapter here, backend by backend. The renderer — the slow part on a cold
    /// driver, and the only part that can take seconds on DX12 — is built by
    /// [`DeviceInit::build_renderer`], so the caller can run it on a worker and
    /// keep its event loop responsive (prompt 17 S3b).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn init_device(
        window: &Window,
        init: &GpuInit,
        probe: Option<GpuProbe>,
        clear_frame: bool,
    ) -> DeviceInit {
        let power = init.power_preference();
        let mut chosen: Option<(wgpu::Instance, wgpu::Adapter, wgpu::Surface<'static>)> = None;

        if let Some(probe) = probe {
            match probe.instance.create_surface(window) {
                Ok(surface) => {
                    // SAFETY: the window outlives the surface (the runner owns
                    // both and drops the GPU state last); wgpu requires the
                    // 'static surface form for this pattern.
                    let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };
                    if adapter_is_usable(&probe.adapter)
                        && probe.adapter.is_surface_supported(&surface)
                    {
                        chosen = Some((probe.instance, probe.adapter, surface));
                    } else {
                        log::warn!(
                            "the probed adapter {} cannot present on this window; re-probing",
                            probe.adapter.get_info().name
                        );
                    }
                }
                Err(error) => {
                    log::warn!("cannot create the surface from the probed instance: {error}")
                }
            }
        }

        if chosen.is_none() {
            for backends in init.backend_order() {
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends,
                    ..Default::default()
                });
                startup::mark("engine: instance");
                let surface = match instance.create_surface(window) {
                    Ok(surface) => surface,
                    Err(error) => {
                        log::warn!("cannot create a surface for {backends:?}: {error}");
                        continue;
                    }
                };
                // SAFETY: see above.
                let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };
                match pollster::block_on(EngineContext::request_adapter_with(
                    &instance,
                    Some(&surface),
                    power,
                )) {
                    Ok(adapter) if adapter_is_usable(&adapter) => {
                        startup::mark("engine: adapter");
                        chosen = Some((instance, adapter, surface));
                        break;
                    }
                    Ok(adapter) => {
                        log::warn!(
                            "skipping {backends:?}: {} cannot run the engine",
                            adapter.get_info().name
                        );
                    }
                    Err(error) => {
                        log::warn!("no adapter for {backends:?}: {error}");
                    }
                }
            }
        }

        let Some((instance, adapter, surface)) = chosen else {
            panic!(
                "no usable wgpu adapter: every configured backend either had no adapter or \
                 lacks {ENGINE_ADAPTER_FEATURES:?}; install a GPU driver or set WGPU_BACKEND \
                 to Vulkan or DX12"
            );
        };

        // Seed the pipeline cache from the previous run on the same adapter.
        let cache_file = init
            .pipeline_cache
            .as_ref()
            .and_then(|config| PipelineCacheFile::for_adapter(config, &adapter.get_info()));
        let cache_data = cache_file.as_ref().and_then(PipelineCacheFile::load);

        let context = match pollster::block_on(EngineContext::from_adapter_with_cache(
            instance, adapter, cache_data,
        )) {
            Ok(context) => context,
            Err(error) => panic!("GPU device creation failed: {error:?}"),
        };
        startup::mark("engine: device");

        let caps = surface.get_capabilities(&context.adapter);
        // Colour-space policy (audit #12): the engine renders in **linear**
        // light and relies on the output target's transfer function for the
        // final display encode. Prefer an `*Srgb` surface format so wgpu
        // performs that sRGB encode on present; fall back to a linear format
        // only if the surface exposes no sRGB variant.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .unwrap_or(caps.formats[0]);
        // PresentMode::Fifo        = hard vsync (locked to monitor refresh)
        // PresentMode::AutoNoVsync = fully uncapped, no tearing guarantee
        // Note: Mailbox is avoided because on NVIDIA/Vulkan it can still be
        // driver-capped to the monitor refresh rate, behaving like vsync.
        let present_mode = if init.vsync {
            wgpu::PresentMode::Fifo
        } else {
            wgpu::PresentMode::AutoNoVsync
        };

        let config = wgpu::SurfaceConfiguration {
            // `COPY_SRC` lets `AppContext::request_screenshot` read the
            // presented frame back; adapters that cannot expose it keep the
            // old attachment-only usage and simply never deliver a shot.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | if crate::screenshot::supports_copy_src(&caps) {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::empty()
                },
            format,
            width: init.width.max(1),
            height: init.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&context.device, &config);
        startup::mark("engine: surface configured");

        // The clear-only frame avoids a white or stale rectangle when the
        // window is already visible (the S3 path). While the window is hidden
        // (show-when-ready) there is nothing to see and presenting a black
        // clear frame would only give DWM a stale buffer to show on
        // `ShowWindow`.
        if clear_frame {
            present_clear(
                &surface,
                &config,
                &context.device,
                &context.queue,
                init.background,
            );
            startup::mark("engine: first clear presented");
        }

        DeviceInit {
            surface,
            config,
            context: Some(context),
            cache_file,
            sample_count: init.sample_count,
            hdri_path: init.hdri_path.clone(),
            mode: init.mode,
        }
    }

    /// Web initialisation (asynchronous; the canvas is attached first).
    #[cfg(target_arch = "wasm32")]
    pub async fn new(
        window: &Window,
        width: u32,
        height: u32,
        vsync: bool,
        sample_count: u32,
        hdri_path: Option<String>,
        mode: RendererMode,
    ) -> Self {
        let backends = {
            let win = web_sys::window().expect("no window");
            let is_secure_context = win.is_secure_context();
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
                "[WGPU-Init] Secure Context: {}",
                is_secure_context
            )));
            if !is_secure_context {
                web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(
                    "[WGPU-Init] Origin is insecure. WebGPU will be disabled by the browser. Falling back to WebGL2...",
                ));
            }
            wgpu::Backends::BROWSER_WEBGPU
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        startup::mark("engine: instance");

        {
            use winit::platform::web::WindowExtWebSys;
            let canvas = window
                .canvas()
                .expect("winit window has no canvas in wasm32");
            let web_window = web_sys::window().expect("no window");
            let document = web_window.document().expect("no document");
            canvas.set_width(width);
            canvas.set_height(height);
            if let Some(container) = document.get_element_by_id("ferrous-container") {
                container.append_child(&canvas).ok();
            } else {
                let body = document.body().expect("no body");
                if canvas.parent_element().is_none() {
                    body.append_child(&canvas)
                        .expect("failed to append canvas to document body");
                    canvas.style().set_property("position", "fixed").ok();
                    canvas.style().set_property("top", "0").ok();
                    canvas.style().set_property("left", "0").ok();
                    canvas.style().set_property("z-index", "0").ok();
                }
            }
            canvas.style().set_property("width", "100%").ok();
            canvas.style().set_property("height", "100%").ok();
            canvas.style().set_property("image-rendering", "auto").ok();
            canvas.style().set_property("display", "block").ok();
            canvas.style().set_property("outline", "none").ok();
            canvas.set_attribute("tabindex", "0").ok();
            canvas.focus().ok();
        }

        let surface = match instance.create_surface(window) {
            Ok(surface) => surface,
            Err(error) => panic!("Failed to create surface: {error:?}"),
        };
        // SAFETY: see `new_native`.
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };

        let context = match EngineContext::new_with_instance(instance, Some(&surface)).await {
            Ok(context) => context,
            Err(error) => panic!("EngineContext init failed: {error:?}"),
        };
        startup::mark("engine: device");

        let caps = surface.get_capabilities(&context.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .unwrap_or(caps.formats[0]);
        let present_mode = if vsync {
            wgpu::PresentMode::Fifo
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | if crate::screenshot::supports_copy_src(&caps) {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::empty()
                },
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&context.device, &config);

        let renderer = Box::new(Renderer::new_with_mode(
            context,
            config.width,
            config.height,
            config.format,
            sample_count,
            hdri_path.as_ref().map(std::path::Path::new),
            mode,
        ));
        startup::mark("engine: renderer built");

        Self {
            surface,
            config,
            renderer,
        }
    }

    /// Writes the pipeline cache to disk once, after the first frame.
    ///
    /// A backend without `PIPELINE_CACHE` (DX12, Metal, GL in wgpu 23) has no
    /// cache object and this is a no-op.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn save_pipeline_cache(&mut self, force: bool) {
        if self.pipeline_cache_saved && !force {
            return;
        }
        self.pipeline_cache_saved = true;
        let Some(file) = &self.pipeline_cache else {
            return;
        };
        let Some(cache) = &self.renderer.context.pipeline_cache else {
            return;
        };
        if let Some(data) = cache.get_data() {
            if !data.is_empty() {
                file.save(&data);
            }
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

/// Work that can start before the window exists (prompt 17 S3).
///
/// The GPU probe and the engine UI font bake need no window, so they run on
/// workers while winit creates the window and the event loop starts. The
/// application's own work (its font worker) is outside the engine.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct StartupWork {
    gpu: Option<GpuProbeHandle>,
    font: Option<std::thread::JoinHandle<Option<ferrous_font::BakedAtlas>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl StartupWork {
    /// Starts the probes for `config`.
    pub(crate) fn start(config: &AppConfig) -> Self {
        let gpu = Some(GpuProbeHandle::start(&GpuInit::from_config(config)));
        let font = config.font_bytes.map(|bytes| {
            let cache = config
                .font_cache
                .as_ref()
                .and_then(ferrous_font::AtlasCache::from_config);
            std::thread::Builder::new()
                .name("ferrous-ui-font".to_owned())
                .spawn(move || {
                    let _span = startup::span("engine: ui font bake (worker)");
                    ferrous_font::BakedAtlas::build_cached(
                        cache.as_ref(),
                        bytes,
                        &ferrous_font::charset::latin_western(),
                    )
                    .ok()
                })
                .ok()
        });
        Self {
            gpu,
            font: font.flatten(),
        }
    }

    /// Takes the GPU probe handle (joined by the caller once the window exists).
    pub(crate) fn take_gpu(&mut self) -> Option<GpuProbeHandle> {
        self.gpu.take()
    }

    /// Waits for the UI font bake; `None` when it failed or did not run.
    pub(crate) fn join_font(self) -> Option<ferrous_font::BakedAtlas> {
        self.font?.join().ok().flatten()
    }
}

/// Presents one clear-only frame: no pipelines, no fonts, no renderer.
#[cfg(not(target_arch = "wasm32"))]
fn present_clear(
    surface: &wgpu::Surface<'_>,
    config: &wgpu::SurfaceConfiguration,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: wgpu::Color,
) {
    let Ok(frame) = surface.get_current_texture() else {
        // A swapchain that is not ready yet: the window keeps the background
        // colour painted by the OS.
        return;
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Startup clear"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Startup clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    queue.submit(Some(encoder.finish()));
    frame.present();
    let _ = config;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init(backends: Vec<wgpu::Backends>) -> GpuInit {
        GpuInit {
            width: 800,
            height: 600,
            vsync: true,
            sample_count: 1,
            hdri_path: None,
            backends,
            power_preference: wgpu::PowerPreference::HighPerformance,
            pipeline_cache: None,
            background: wgpu::Color::BLACK,
            mode: RendererMode::Full3D,
        }
    }

    #[test]
    fn the_configured_order_is_kept() {
        let configured = [wgpu::Backends::DX12, wgpu::Backends::VULKAN];
        assert_eq!(backend_order_from(&configured, None), configured.to_vec());
        assert_eq!(
            init(configured.to_vec()).backend_order(),
            configured.to_vec()
        );
    }

    #[test]
    fn an_environment_override_replaces_the_order() {
        let configured = [wgpu::Backends::VULKAN, wgpu::Backends::DX12];
        assert_eq!(
            backend_order_from(&configured, Some(wgpu::Backends::GL)),
            vec![wgpu::Backends::GL]
        );
        // An empty override is ignored.
        assert_eq!(
            backend_order_from(&configured, Some(wgpu::Backends::empty())),
            configured.to_vec()
        );
    }

    #[test]
    fn an_empty_list_falls_back_to_the_platform_default() {
        assert_eq!(
            backend_order_from(&[], None),
            crate::builder::default_backends()
        );
    }

    #[test]
    fn the_platform_default_tries_vulkan_before_the_fallbacks() {
        let order = crate::builder::default_backends();
        #[cfg(target_os = "windows")]
        {
            assert_eq!(order[0], wgpu::Backends::VULKAN);
            assert_eq!(order[1], wgpu::Backends::DX12);
            assert_eq!(order[2], wgpu::Backends::GL);
        }
        #[cfg(target_os = "macos")]
        assert_eq!(order, vec![wgpu::Backends::METAL]);
        assert!(!order.is_empty());
    }

    fn info(driver_info: &str) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "Test GPU".to_owned(),
            vendor: 0x10de,
            device: 0x2882,
            device_type: wgpu::DeviceType::DiscreteGpu,
            driver: "NVIDIA".to_owned(),
            driver_info: driver_info.to_owned(),
            backend: wgpu::Backend::Vulkan,
        }
    }

    #[test]
    fn the_cache_key_covers_the_driver_and_backend() {
        let base = pipeline_cache_key(&info("32.0.15.9282"));
        assert_eq!(base, pipeline_cache_key(&info("32.0.15.9282")));
        assert_ne!(base, pipeline_cache_key(&info("32.0.15.9999")));

        let mut other_backend = info("32.0.15.9282");
        other_backend.backend = wgpu::Backend::Dx12;
        assert_ne!(base, pipeline_cache_key(&other_backend));

        let mut other_device = info("32.0.15.9282");
        other_device.device = 0x1234;
        assert_ne!(base, pipeline_cache_key(&other_device));
    }

    fn temp_cache(name: &str) -> (PipelineCacheFile, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "ferrous-pipeline-cache-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let config = PipelineCacheConfig::for_app("Test").with_directory(&directory);
        let file = PipelineCacheFile::for_adapter(&config, &info("1.2.3")).expect("a directory");
        (file, directory)
    }

    #[test]
    fn the_cache_file_round_trips() {
        let (file, directory) = temp_cache("roundtrip");
        assert!(file.load().is_none(), "nothing stored yet");
        file.save(b"pipeline bytes");
        assert_eq!(file.load().as_deref(), Some(&b"pipeline bytes"[..]));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_corrupt_or_foreign_cache_file_is_ignored() {
        let (file, directory) = temp_cache("corrupt");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&file.path, b"not a cache").unwrap();
        assert!(file.load().is_none());

        // A file written for another key (another driver) is rejected.
        let mut header = Vec::new();
        header.extend_from_slice(PIPELINE_CACHE_MAGIC);
        header.extend_from_slice(&PIPELINE_CACHE_FORMAT.to_le_bytes());
        header.extend_from_slice(&4u32.to_le_bytes());
        header.extend_from_slice("00000000000000000000000000000000".as_bytes());
        header.extend_from_slice(b"data");
        std::fs::write(&file.path, &header).unwrap();
        assert!(file.load().is_none());

        // A truncated payload is rejected too.
        std::fs::write(&file.path, b"FPCA\x01\x00\xff\x00\x00\x00").unwrap();
        assert!(file.load().is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
