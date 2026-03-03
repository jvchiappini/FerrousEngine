# FerrousEngine: Next-Step Graphics PRD (Product Requirements Document)

Este documento detalla paso a paso las "recetas" técnicas para implementar los siguientes grandes hitos gráficos del motor. Está diseñado para que cualquier agente de IA o programador pueda seguir las instrucciones secuencialmente y sin ambigüedad.

---

## Módulo 1: Geometría Compleja - Primitiva de Esfera

**Objetivo:** Permitir al motor crear formas curvas (Esferas y Cilindros) para testear correctamente el pipeline PBR (reflejos especulares y curvatura de normales), en lugar de limitarse a Cubos y Quads.

### Tarea 1.1: Generación Matemática de la Esfera (`build_uv_sphere`)
*   **Contexto:** En `crates/ferrous_renderer/src/geometry/primitives/sphere.rs` (archivo a crear).
*   **Requerimientos:**
    1.  Crear una función `sphere(device: &wgpu::Device, latitudes: u32, longitudes: u32) -> Mesh`.
    2.  Implementar la generación de vértices ($x$, $y$, $z$) iterando sobre $phi$ (latitud, de 0 a PI) y $theta$ (longitud, de 0 a 2PI).
    3.  Calcular normales (que en una esfera centrada en el origen son equivalentes a la posición normalizada).
    4.  Calcular UVs: $u = theta / 2\pi$ y $v = phi / \pi$.
    5.  Generar el arreglo de índices uniendo los vértices adyacentes formando dos triángulos (Quads) por cuadrícula. Cuidado con los polos (triángulos degenerados o tratar el anillo polar como solo triángulos).
    6.  Llamar a `compute_tangents(&mut vertices, &indices)` para inyectar correctamente los Tangents y que funcione el Normal Mapping.
*   **Validación:** La malla devuelta (`Mesh`) debe ser subida a la GPU y retornar buffers válidos, similares a `primitives::cube()`.

### Tarea 1.2: Integración en el ECS
*   **Contexto:** Modificar `crates/ferrous_core/src/scene/mod.rs` o el enum correspondiente `ElementKind`.
*   **Requerimientos:**
    1.  Añadir el enum variant: `ElementKind::Sphere { radius: f32, latitudes: u32, longitudes: u32 }`.
    2.  Actualizar la función de match de renderizado o generación de malla para llamar a `primitives::sphere` si detecta ese variant.
    3.  Añadir a `World` el helper `pub fn spawn_sphere(...) -> DefaultEntity`.
*   **Validación:** Modificar la escena en `crates/ferrous_editor/src/app.rs` para renderizar al menos dos esferas (una metálica y una de plástico) para probar que rebotan el brillo direccional correctamente.

---

## Módulo 2: Importación de Assets Carga de modelos GLTF/GLB

**Objetivo:** Parsear modelos 3D externos para renderizar mallas industriales con múltiples texturas PBR.

### Tarea 2.1: Implementación del Lector GLTF (`gltf` crate)
*   **Contexto:** Agregar dependencia `gltf = "1.0"` a `crates/ferrous_assets/Cargo.toml`. Crear el archivo `crates/ferrous_assets/src/model.rs`.
*   **Requerimientos:**
    1.  Escribir función `load_gltf(path: &Path) -> Vec<ModelData>`.
    2.  Iterar sobre arreglos dentro del GLTF (posiciones, normales, textcoords). Entrelazar (interleave) los datos en la estructura nativa actual de `ferrous_renderer::geometry::Vertex`.
    3.  Extraer metadatos de Material PBR: leer `baseColorFactor`, `metallicFactor`, `roughnessFactor` y las rutas de las imágenes ligadas.
*   **Validación:** Devolver una representación CPU del modelo que la app luego pueda mandar al Renderer, sin acoplamiento a `wgpu`.

### Tarea 2.2: Buffer Uploading y Spawn Múltiple
*   **Contexto:** Convertir `ModelData` en mallas `Mesh` cargadas.
*   **Requerimientos:**
    1.  Por cada nodo del GLTF, crear un `ElementKind::Mesh { mesh_handle }` en el `ferrous_core::World`.
    2.  Las texturas mencionadas por el GLTF deben ser leídas por `image::load`, decodificadas y pasadas a `create_texture`.
    3.  Asegurarse de que el color base del gltf y los factores de metallic se mapeen al `MaterialDescriptor` del motor.

---

## Módulo 3: Iluminación Escalable

**Objetivo:** Pasar de una única luz a N-luces puntuales (Point Lights) sin que afecte el rendimiento.

### Tarea 3.1: Uniform Buffer a Storage Buffer
*   **Contexto:** Archivos `light.rs`, `world_pass.rs`, `pbr.wgsl`, `instanced.wgsl`.
*   **Requerimientos:**
    1.  Modificar la estructura de luz añadiendo posición y atenuación (`falloff`, `radius`).
    2.  En vez de un solo `<uniform> dir_light`, crear un arreglo en WGSL:
        ```wgsl
        struct PointLight { position: vec4<f32>, color: vec4<f32> }
        @group(3) @binding(1) var<storage, read> point_lights: array<PointLight>;
        ```
    3.  En CPU, recolectar en el loop todas las entidades que tengan un Componente de Luz y copiarlas (cast to bytes) a un `wgpu::Buffer` marcado como `STORAGE`.
*   **Validación:** El shader debe loopear a lo largo del `arrayLength(&point_lights)` y sumar las contribuciones de reflectancia difusa/especular a `Lo` (Cook-Torrance).

---

## Módulo 4: Framebuffer HDR & Post-Procesado

**Objetivo:** Pasar de LDR a HDR (High Dynamic Range) real y añadir un pipeline para post-procesado (ACESTM + Bloom + etc).

### Tarea 4.1: Render a Textura Formato `Rgba16Float`
*   **Contexto:** Reemplazar el acceso directo a la `SurfaceTexture` en el Render Pass por un Render Target interno.
*   **Requerimientos:**
    1.  En la inicialización, crear una textura del tamaño de la pantalla llamada `hdr_render_target` con formato `wgpu::TextureFormat::Rgba16Float`.
    2.  Modificar los pases de render estándar (WorldPass, InstancedPass) para que escriban en esta textura `Rgba16Float` en vez del formato de la ventana (que típicamente es sRGB LDR).
    3.  Remover todo el Tone Mapping y "Gamma Correction" de `pbr.wgsl` e `instanced.wgsl`. Deben devolver `radiance` puro y crudo sumado (que puede superar el valor de 1.0).

### Tarea 4.2: Fullscreen Triangle Pass (Resolve/Tone Mapping Pass)
*   **Contexto:** Nuevo módulo `passes/postprocess.rs`.
*   **Requerimientos:**
    1.  Crear un Bind Group que reciba el `hdr_render_target` mediante un sampler.
    2.  Hacer un pipeline sin Vertex Buffer (se generan 3 vértices en el Vertex Shader automáticamente usando `gl_VertexIndex` para dibujar un triángulo gigante que cubra la pantalla).
    3.  En el Fragment Shader `post.wgsl`:
        *  Muestrear el píxel HDR.
        *  Aplicar **ACES Filmic Tone Mapping** (matemática estándar de curva en forma de S).
        *  Escribir a la superficie LDR (la de la pantalla).
*   **Validación:** Los brillos intensos (metales reflejando la direccional) ya no se verán "planos y blancos recortados", sino que harán un gradiente suave al blanco, imitando cómo reacciona la cámara física.

---

## Módulo 5: Transparencias y Modos de Mezcla Avanzada (Alpha Blending)

**Objetivo:** Renderizar cristales y espejos transparentes correctamente ordenados según profundidad.

### Tarea 5.1: Pipeline de Mezcla Transparente
*   **Contexto:** Copiar `pipeline/pbr.rs` a `pipeline/transparent.rs`.
*   **Requerimientos:**
    1.  Cambiar la estructura de `ColorTargetState` por una mezcla estándar:
        ```rust
        blend: Some(wgpu::BlendState::ALPHA_BLENDING)
        // Src_alpha * Src_color + (1 - Src_alpha) * Dest_color
        ```
    2.  Deshabilitar la escritura a profundidad: `depth_write_enabled: false`. Se debe leer, pero no ensuciar el buffer de profundidad para que los espejos translúcidos detrás de otro objeto transparente se rendericen.

### Tarea 5.2: Ordenamiento Culling en CPU
*   **Contexto:** En el ECS, antes del renderizado.
*   **Requerimientos:**
    1.  Extraer todas las entidades con flag `ALBEDO_TRANSPARENT`.
    2.  Calcular distancia al cubo pos de la cámara (Camera Eye). Sortear la lista de renderizado Transparente desde la mayor a menor distancia (`Back-to-Front Order`).
    3.  Pase de Renderizado: Los opacos se renderizan primero (Front-to-back para optimizar la GPU z-buffer). Los transparentes de último.

---

### Criterios Generales de Éxito / Reglas de la IA:
*   **Refactor Mínimo, Impacto Máximo:** Toda nueva inclusión matemática en WGSL debe tener en cuenta el `pipeline::Layout` preexistente; no cambiar índices arbitrariamente.
*   **Manejo de Errores:** Al implementar cargadores como GLTF o Imágenes, utilizar `Result<...>` propagados y enviar avisos al Log en lugar de usar comandos `.unwrap().unwrap()` que pueden paniquear la aplicación principal.
*   **Cero Warnings de Rust:** Ejecutar frecuentemente `cargo check` durante implementaciones para verificar "unused variables" en buffers y structs de alineación WGPU `[bytemuck::Pod]`.