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
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("no se pudo obtener un adaptador adecuado")]
    AdapterUnavailable,
    #[error("error al solicitar device: {0}")]
    DeviceRequest(String),
}

impl EngineContext {
    /// Crea un `EngineContext` de forma asíncrona. Esto inicializa
    /// una instancia de WGPU, el adaptador y el par device/queue.
    ///
    /// Se utiliza una preferencia de alto rendimiento y no se asocia a una
    /// superficie porque la idea es que el renderizador sea "render-to-\
    /// texture" y la superficie la gestione el editor más adelante.
    pub async fn new() -> anyhow::Result<Self> {
        // wgpu 0.23 requires an InstanceDescriptor instead of a direct
        // Backends bitfield. We opt for all backends to allow Vulkan/Metal/
        // DX12/BrowserGPU depending on platform.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None, // headless para RTT
                force_fallback_adapter: false,
            })
            .await
            .context(ContextError::AdapterUnavailable)?;

        println!(
            "[WGPU] Selected Adapter: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Engine Device"),
                    // the fields were renamed in 0.23
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
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
        })
    }
}
