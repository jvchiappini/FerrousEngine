# FerrousEngine — Web 3D Engine Roadmap (Basic & Scalable Core)
<!-- 
  ARCHIVO DE CONTEXTO PERSISTENTE
  Roadmap iterativo centrado primero en lograr un motor 3D fundacional,
  escalable y que renderice "Meshes" (mallas) de manera perfecta y
  robusta en la Web (WASM + WebGPU), antes de añadir características avanzadas.
-->

## Objetivo Principal
Establecer un motor 3D básico pero extremadamente sólido y escalable en la Web. La prioridad absoluta es cargar, procesar y renderizar Mallas (Meshes) con un pipeline PBR estándar impecable, garantizando que el `ECS`, las matemáticas (transforms) y el `wgpu` se comporten de forma estable en un canvas HTML.

---

## 🔲 FASE 1 — Infraestructura Web (El Canvas Mínimo Viable)
**Objetivo:** Lograr que la ventana de la aplicación corra en el navegador sin crashes, sincronizando el event loop asíncrono.
*   **Target WASM:** Configurar compilación `wasm32-unknown-unknown` usando `wasm-bindgen` y `console_error_panic_hook`.
*   **Canvas y Event Loop:** Adaptar el init de `winit` para adherirse al DOM (`document.getElementById("canvas")`) usando `requestAnimationFrame` sin bloquear el navegador.
*   **Contexto WebGPU:** Inicializar correctamente `ferrous_gpu` limitando los device requests a lo que soporta el API web actual. Limpiar color con un fondo fijo para confirmar el contexto funcional.

---

## 🔲 FASE 2 — Carga Básica de Mallas (Assets & Buffers)
**Objetivo:** Traer información de geometría (vertices e índices) desde un servidor local directamente a la RAM de WASM usando peticiones web asíncronas.
*   **Fetch Asíncrono:** Reemplazar `std::fs` en las rutinas de carga. Un `AssetLoader` iterativo con `web_sys::window().unwrap().fetch()`.
*   **Decodificador GLTF Base:** Extraer posiciones (`Vec3`), normales (`Vec3`), coordenadas UV (`Vec2`), e índices (`u16`/`u32`) de archivos `.glb`.
*   **Sincronización a GPU:** Subir de manera eficiente esos arreglos de mallas hacia los `Buffer` del contexto de WebGPU (VertexBuffers, IndexBuffers).

---

## 🔲 FASE 3 — ECS, Transform y Render Pipeline Minimalista
**Objetivo:** Renderizar la geometría blanca (o un color base sólido) en el mundo tridimensional, validando la cámara y matemáticas.
*   **Cámara Matemáticamente Perfecta:** Projection Matrix (Perspective) + View Matrix orbitable para navegar y poder dar vuelta alrededor de la malla.
*   **ECS Transform:** Un componente `Transform` calculando matrices 4x4 combinadas y enviándolas al shader por medio de Uniforms u Object Buffers locales para posicionar elementos.
*   **Pass Básico de Render:** Binding de un pipeline WGSL limpio (MVP -> Model, View, Projection) y dibujo (`draw_indexed`) respetando el Z-Buffer / Depth Stencil. Z-fighting no debe ocurrir.

---

## 🔲 FASE 4 — Shading PBR Estándar (Renderizado "Perfecto")
**Objetivo:** Asegurar que las mallas cargadas luzcan idénticas a como se verían en Blender o Three.js. No hay atajos, la iluminación PBR debe estar matemáticamente alineada.
*   **El Shader Maestro (`pbr.wgsl`):** Un fragment shader sólido que recoja texturas de Albedo, Normal Maps con cálculo correcto de tangentes, y la textura de Roughness/Metallic unificada.
*   **Iluminación Directa:** Una sola o múltiples luces direccionales procesando modelo matemático BRDF estándar (ej. GGX) basándose en las Normales de la malla para validar iluminación direccional dura.
*   **Soporte de Instancing (Escalabilidad):** Permitir dibujar la misma malla 1,000 veces pasando simplemente una matriz distinta por instancia para probar el desempeño real en WebGPU de cara a la escalabilidad futua (dibujar ejércitos o bosques).

---

## 🔲 FASE 5 — Sombreado Dinámico y Texturas Correctas (Opcional, Paso 2)
**Objetivo:** Profundizar la fidelidad del renderer. Si el objeto tiene sombras, luce en la escena, y sus texturas cargan con correctos mappings (sRGB).
*   **Shadow Mapping (1 Cascaded Shadow Map):** Pase de profundidad desde el punto de vista de un Sol direccional; vital para que las mallas se vean como objetos con masa reales.
*   **Manejo de Transparencias Limpio (Alpha):** Diferenciar las mallas Opacas de las de recorte ('Alpha Cutoff'/'Mask') vs `Alpha Blend` (con depth testing bloqueado o reordenando profundidad en el renderer).
*   **Cubemap / IBL (Iluminación Basada en Imágenes):** Un fondo simple (skybox HDR) generador de reflejos para validar que un metal 100% brillante luzca perfecto en ambiente web. 

---

### Criterio de Éxito de la Versión Fundacional:
Si envías el `.glb` de un **"Damaged Helmet"** (casco PBR estándar), este deberá verse con materiales correctos sin glitches visuales nativamente a 60 FPS mínimos en Chrome/Firefox sobre WASM, renderizando el ECS central iterando sus transforms cada cuadro.
A partir de aquí, las lógicas ultra complejas podrán construirse progresivamente.
