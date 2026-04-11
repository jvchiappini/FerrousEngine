# 🚀 Ferrous Web: Roadmap towards Next-Gen 3D Excellence

Este documento define la trayectoria técnica para evolucionar **Ferrous Web** desde un motor experimental hacia una librería de primer nivel que lidere el estándar de gráficos en la web bajo WebGPU y Rust, superando las limitaciones arquitectónicas de Three.js.

---

## 🏗️ Fase 1: Cimentación y Estabilidad (Finalizada ✅)
*Objetivo: Lograr un entorno de ejecución WebGPU/WASM robusto y sin fricción.*

- [x] **Runtime WebGPU/WASM:** Ejecución estable en navegadores modernos.
- [x] **Auto-Resizing Dinámico:** Sincronización perfecta entre el DOM y WGPU.
- [x] **Sistema de Input Unificado:** Soporte WASD y Mouse sin conflictos.
- [x] **Manejo de Errores Profesional:** Reporte de fallos y panics via JSON a JS.

---

## 🔌 Fase 2: Experiencia de Desarrollador (DX) y Bridge (Finalizada ✅)
*Objetivo: Hacer que usar Ferrous desde JavaScript sea tan fácil como Three.js.*

- [x] **Fluent JS API:** Implementación de `JsEntity` para llamadas encadenadas:
  ```javascript
  engine.createBox('MyBox', ...)
        .set_position(0, 5, 0)
        .set_material(1, 0, 0, 0.8, 0.2);
  ```
- [x] **Modular Dispatcher:** Desacoplo total entre la creación de comandos y su ejecución.
- [x] **Asset Management Pro:** Carga asíncrona de texturas y modelos con promesas (WASM Bindgen Futures integration).
- [x] **Plugin System v1:** Interfaz para que usuarios extiendan el motor (Hooks en `sync_world` y `update`).
- [ ] **Scene Persistence Pro:** Guardado y carga de escenas complejas en JSON/Binary. (Próximo paso)
---

## ✨ Fase 3: Visual Fidelity & API Expansion (Finalizada ✅)
*Objetivo: Alcanzar la paridad de características visuales con motores triple-A de escritorio.*

- [x] **Dynamic Camera Uniforms:** Soporte para exposición y niebla dinámica en buffer de 256 bytes.
- [x] **Atmospheric Controls:** Neblina por distancia y exposición HDR configurable en tiempo real.
- [x] **Post-Processing Integration:** El pass de post-procesado ahora responde a los parámetros del entorno globales.
- [x] **API Exposure:** Métodos `set_environment` y `set_exposure` disponibles en JS.

---

## ⚡ Fase 4: Advanced Material System (Finalizada ✅)
*Objetivo: Utilizar Rust y WebGPU para representar superficies complejas con alto rendimiento.*

- [x] **Material Descriptor Expansion:** Soporte para misiones, barniz (clearcoat) y opacidad.
- [x] **Dynamic Material Updates:** Modificación de propiedades en tiempo real via API Fluente (`set_clearcoat`, `set_opacity`, `set_texture`).
- [x] **Asset Management Pro:** Carga asíncrona de texturas y modelos con promesas de JS.
- [x] **Plugin System v1:** Soporte para hooks de ciclo de vida (`on_update`, `on_sync_world`). Los usuarios de JS pueden registrar plugins vía `engine.register_plugin`.
- [x] **Optimized PBR Shader:** Implementación de nuevos lóbulos especulares y blending en WGSL.

---

## 🚀 Fase 5: GPU-Driven Features & Optimization (Próxima 🚧)
*Objetivo: Aprovechar Compute Shaders para masividad extrema.*

- [ ] **WebGPU Compute Skinning:** Animación de miles de personajes vía compute shaders.
- [ ] **GPU Particle System:** Millones de partículas gestionadas 100% en la GPU.
- [ ] **Frustum Culling en GPU:** Reducción drástica de CPU overhead en escenas masivas.

---

## 🎯 Visión Final
Ferrous Web no es una librería más; es la transición hacia una web donde las aplicaciones no "intentan" emular gráficos de escritorio, sino que **son** aplicaciones nativas corriendo dentro de un estándar abierto. 

**Rust + WebGPU + ECS** es la fórmula que hará que las experiencias 3D en el navegador dejen de sentirse como juguetes y empiecen a sentirse como el futuro.

---
*Roadmap actualizado el 2026-04-11 tras completar la Fase 4.*
