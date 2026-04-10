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
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .context(ContextError::AdapterUnavailable)?;

        let info = adapter.get_info();
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
            "[WGPU] Selected Adapter: {} ({:?}) - Backend: {:?}",
            info.name, info.device_type, info.backend
        )));
        #[cfg(not(target_arch = "wasm32"))]
        println!(
            "[WGPU] Selected Adapter: {} ({:?}) - Backend: {:?}",
            info.name, info.device_type, info.backend
        );
        let backend = info.backend;

        // WebGL2 tiene límites mucho más bajos que un backend nativo;
        // usar Limits::default() en GL hace que request_device falle o
        // active rutas de validación lentas. Usamos los límites correctos.
        let mut limits = if backend == wgpu::Backend::Gl {
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
            | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING;
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

        Ok(Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            backend,
        })
    }
}
