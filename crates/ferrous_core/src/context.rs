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

        println!(
            "[WGPU] Selected Adapter: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Engine Device"),
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
