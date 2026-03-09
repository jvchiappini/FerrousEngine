# ferrous_gpu

`ferrous_gpu` es el crate encargado de la inicialización de **WGPU** en FerrousEngine. Actúa como el único punto de entrada que depende incondicionalmente de `wgpu`, manteniendo el resto del workspace limpio de dependencias gráficas pesadas a menos que sea estrictamente necesario.

## Propósito

Centralizar la creación del `Instance`, `Adapter`, `Device` y `Queue`. Provee la estructura `EngineContext` que agrupa estos elementos para ser compartidos por el motor de renderizado y el sistema de UI.

## Componentes Clave

### `EngineContext`

Es el corazón de la comunicación con la GPU. Contiene todas las manijas necesarias para interactuar con el hardware:

```rust
pub struct EngineContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}
```

## Ejemplo de Inicialización

```rust
use ferrous_gpu::EngineContext;

// Inicializa la GPU de forma asíncrona (típicamente al inicio de la app)
let context = EngineContext::new(window_handle).await?;

// El contexto se pasa luego a los renderers
gui_renderer.init(&context.device, &context.queue);
```

## Filosofía de Diseño

- **Desacoplamiento:** Al aislar `wgpu` aquí, reducimos los tiempos de compilación de crates que solo necesitan tipos geométricos o lógica de juego.
- **Portabilidad:** Maneja internamente las diferencias de inicialización entre backends (Vulkan, Metal, DX12, WebGPU).
- **Recursos Compartidos:** Utiliza `Arc` para el `Device` y la `Queue`, permitiendo que múltiples sistemas (ej. el motor 3D y la UI) operen sobre el mismo contexto de hardware de forma segura.

---

## Further Reading
- [Renderer de la UI — ferrous_ui_render](../../ferrous_ui_render/docs/README.md)
- [Shell de la aplicación — ferrous_app](../../ferrous_app/docs/README.md)
