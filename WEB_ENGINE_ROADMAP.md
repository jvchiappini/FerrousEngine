# FerrousEngine — Web 3D Engine Roadmap (Basic & Scalable Core)

<!-- 
  ARCHIVO DE CONTEXTO PERSISTENTE Actualizado tras completar Fase 3.
-->

## Objetivo Principal
Establecer un motor 3D básico pero extremadamente sólido y escalable en la Web. La prioridad absoluta es cargar, procesar y renderizar Mallas (Meshes) con un pipeline PBR estándar impecable, garantizando que el `ECS`, las matemáticas (transforms) y el `wgpu` se comporten de forma estable en un canvas HTML.

---

## ✅ FASE 1 — Infraestructura Web (Finalizada)
**Objetivo:** Lograr que la ventana de la aplicación corra en el navegador sin crashes.
*   **Target WASM:** Configurado con `wasm-bindgen` y reporte de errores estructurado.
*   **Canvas y Event Loop:** Sincronización perfecta de frames y auto-resize proactivo.
*   **Contexto WebGPU:** Inicialización robusta con soporte para múltiples backends.

---

## ✅ FASE 2 — Carga Básica de Mallas (Finalizada)
**Objetivo:** Traer información de geometría de manera asíncrona.
*   **Fetch Asíncrono:** Implementado en `AssetServer` para entornos web.
*   **Buffer Management:** Subida eficiente de Vertex e Index buffers a la VRAM.

---

## ✅ FASE 3 — ECS, Transform y Render Pipeline Minimalista (Finalizada)
**Objetivo:** Renderizar geometría validando la cámara y matemáticas.
*   **Cámara Orbit/Fly:** Sistema de cámaras desacoplado con control total desde JS.
*   **ECS Transform:** Sistema de jerarquías y sincronización de mundo a GPU funcionando.
*   **API Fluente:** Introducción de `JsEntity` para manipulación encadenada desde JS.

---

## 🔲 FASE 4 — Shading PBR Estándar (Finalizada ✅)
**Objetivo:** Asegurar que las mallas cargadas luzcan idénticas a como se verían en Blender.
*   **Shader Maestro (`pbr.wgsl`):** Pipeline PBR completo con soporte para Roughness/Metallic.
*   **Atmósfera:** Control dinámico de neblina y exposición HDR (ACES).
*   **Instancing Masivo:** Soporte integrado para miles de instancias por Draw Call.

---

## 🚧 FASE 5 — Sombreado Dinámico y Texturas Correctas (En Progreso)
**Objetivo:** Profundizar la fidelidad del renderer. 
*   **Shadow Mapping:** Soporte para luces puntuales y direccionales con sombras.
*   **Manejo de Transparencias:** Diferenciación entre Mask y Blend.
*   **IBL (Iluminación Basada en Imágenes):** (Siguiente Paso) Soporte para HDRI y Skyboxes dinámicos.

---

### Estatus Actual:
El motor ha superado la etapa fundacional. Posee un pipeline PBR estable, una API moderna y fluente en JavaScript, y soporte para controles atmosféricos dinámicos.

---
*Roadmap actualizado el 2026-04-11.*
