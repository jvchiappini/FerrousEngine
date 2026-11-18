use std::sync::Arc;

use anyhow::Context as _;
use thiserror::Error;

/// Contenedor de los objetos principales de WGPU que se comparten entre
/// distintas partes del motor.
///
/// `Instance` y `Adapter` no tienen que ser `Arc` porque no se envían entre
/// hilos con frecuencia, pero `Device` y `Queue` sí, de ahí el uso de
/// `Arc` para permitir un acceso seguro y clonable.
pub struct EngineContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    /// Backend seleccionado por wgpu (Vulkan, Metal, Dx12, WebGpu, Gl, etc.).
    pub backend: wgpu::Backend,
    /// Shared pipeline cache, passed as `cache: Some(..)` when creating render
    /// and compute pipelines. It lets the driver reuse shader compilation work
    /// between pipelines and between program runs. `None` when the device does
    /// not support [`wgpu::Features::PIPELINE_CACHE`] (only Vulkan implements it
    /// in wgpu 23, so this is `None` on Metal/DX12/WebGPU/GL).
    ///
    /// Wrapped in `Arc` so sub-renderers (the 2D and UI renderers) can hold a
    /// clone and thread it into their own pipeline creation without borrowing
    /// the whole context for their lifetime.
    pub pipeline_cache: Option<Arc<wgpu::PipelineCache>>,
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("no se pudo obtener un adaptador adecuado")]
    AdapterUnavailable,
    #[error("error al solicitar device: {0}")]
    DeviceRequest(String),
}

impl EngineContext {
    /// Crea un `EngineContext` headless (sin surface), útil para tests y
    /// contextos de render-to-texture puros.
    pub async fn new() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        Self::new_with_instance(instance, None).await
    }

    /// Crea un `EngineContext` reutilizando una `Instance` ya existente y
    /// opcionalmente asociando una `Surface` para que el adaptador
    /// seleccionado sea garantizadamente compatible con la ventana.
    ///
    /// Usar este método cuando se renderiza a una ventana real — evita rutas
    /// de presentación costosas (copias cross-bus en sistemas multi-GPU).
    pub async fn new_with_instance(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> anyhow::Result<Self> {
        let adapter = Self::request_adapter(&instance, compatible_surface).await?;
        Self::from_adapter(instance, adapter).await
    }

    /// Selecciona el adaptador para `instance`, prefiriendo alto rendimiento y
    /// cayendo a un adaptador software cuando no hay hardware disponible.
    ///
    /// Es el primer paso de [`Self::new_with_instance`]; separarlo permite
    /// medir y trazar la selección de adaptador y la creación de dispositivo
    /// como fases distintas. `compatible_surface` fuerza un adaptador que pueda
    /// presentar en esa superficie.
    pub async fn request_adapter(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> anyhow::Result<wgpu::Adapter> {
        Self::request_adapter_with(
            instance,
            compatible_surface,
            wgpu::PowerPreference::HighPerformance,
        )
        .await
    }

    /// [`Self::request_adapter`] with an explicit power preference.
    ///
    /// `HighPerformance` picks the discrete GPU on a hybrid laptop,
    /// `LowPower` the integrated one, `None` lets wgpu decide.  The engine
    /// passes the application's preference (and `WGPU_POWER_PREF` when set).
    pub async fn request_adapter_with(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
        power_preference: wgpu::PowerPreference,
    ) -> anyhow::Result<wgpu::Adapter> {
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
        {
            Some(a) => a,
            None => {
                // Try fallback (software rendering) if hardware is not available
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::LowPower,
                        compatible_surface,
                        force_fallback_adapter: true,
                    })
                    .await
                    .context(ContextError::AdapterUnavailable)?
            }
        };
        Ok(adapter)
    }

    /// Construye dispositivo, cola y caché de pipelines sobre `adapter`.
    ///
    /// Es el segundo paso de [`Self::new_with_instance`]; `instance` se guarda
    /// en el contexto devuelto.
    pub async fn from_adapter(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
    ) -> anyhow::Result<Self> {
        Self::from_adapter_with_cache(instance, adapter, None).await
    }

    /// [`Self::from_adapter`] that seeds the pipeline cache with `cache_data`.
    ///
    /// The bytes must come from [`wgpu::PipelineCache::get_data`] of a previous
    /// run on the same adapter/driver.  wgpu tolerates stale or corrupt data
    /// (`fallback: true`), so a bad cache only costs a recompile.
    pub async fn from_adapter_with_cache(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        cache_data: Option<Vec<u8>>,
    ) -> anyhow::Result<Self> {
        let info = adapter.get_info();
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
            "[WGPU] Selected Adapter: {} ({:?}) - Backend: {:?}",
            info.name, info.device_type, info.backend
        )));
        #[cfg(not(target_arch = "wasm32"))]
        log::info!(
            "[WGPU] Selected Adapter: {} ({:?}) - Backend: {:?}",
            info.name, info.device_type, info.backend
        );
        let backend = info.backend;

        // WebGL2 tiene límites mucho más bajos que un backend nativo;
        // usar Limits::default() en GL hace que request_device falle o
        // active rutas de validación lentas. Usamos los límites correctos.
        // El GL de escritorio (OpenGL 4.3+) sí soporta storage buffers; solo
        // WebGL2 carece de ellos (prompt 17 S3: los límites de webgl2 hacían
        // fallar la creación del bind group de instancias).
        let mut limits = if backend == wgpu::Backend::Gl && cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        };

        // Solicitar límites de búfer más grandes del adaptador para soportar escenas
        // grandes de voxels, pero con un tope para WebAssembly (32-bit).
        let phys_limits = adapter.limits();
        
        #[cfg(target_arch = "wasm32")]
        {
            // WebAssembly tiene un espacio de direccionamiento de 4GB total.
            // No podemos pedir buffers de gigabytes para evitar "Memory Access Out of Bounds".
            limits.max_storage_buffer_binding_size = phys_limits.max_storage_buffer_binding_size.min(256 * 1024 * 1024); // 256MB
            limits.max_buffer_size = phys_limits.max_buffer_size.min(512 * 1024 * 1024); // 512MB
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            limits.max_storage_buffer_binding_size = phys_limits.max_storage_buffer_binding_size;
            limits.max_buffer_size = phys_limits.max_buffer_size;
        }

        // Request only the features the engine actually uses.  The texture
        // binding array features are required by the GUI renderer when the
        // `assets` feature is enabled (texture arrays in the GUI shader).
        // Both are widely supported on Vulkan/DX12/Metal; however, the WebGPU
        // backend currently panics if we even ask for BINDING_INDEXING support
        // when it's not fully conformant.
        let adapter_features = adapter.features();
        #[cfg(not(target_arch = "wasm32"))]
        let desired_features = wgpu::Features::TEXTURE_BINDING_ARRAY
            | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            // Pipeline cache is Vulkan-only in wgpu 23; requesting it when the
            // adapter exposes it is what lets `create_pipeline_cache` succeed.
            | wgpu::Features::PIPELINE_CACHE
            // Prompt 18 P0: per-pass GPU timings.  Requested only when the
            // adapter exposes it; `GpuTimings` degrades to "unsupported".
            | wgpu::Features::TIMESTAMP_QUERY;
        #[cfg(target_arch = "wasm32")]
        let desired_features = wgpu::Features::empty();

        let required_features = adapter_features & desired_features;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Engine Device"),
                    required_features,
                    required_limits: limits,
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| ContextError::DeviceRequest(e.to_string()))?;

        // Only create the cache when the device actually negotiated the
        // feature; otherwise `create_pipeline_cache` is a validation error.
        // SAFETY: `cache_data` must come from `PipelineCache::get_data` on the
        // same adapter (the engine keys its cache file by vendor/device/driver
        // and version). `fallback: true` makes wgpu ignore stale or corrupt
        // data instead of failing.
        let pipeline_cache = if device
            .features()
            .contains(wgpu::Features::PIPELINE_CACHE)
        {
            Some(Arc::new(unsafe {
                device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("Engine Pipeline Cache"),
                    data: cache_data.as_deref(),
                    fallback: true,
                })
            }))
        } else {
            None
        };

        Ok(Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            backend,
            pipeline_cache,
        })
    }
}
