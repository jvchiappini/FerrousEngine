# 🚀 Ferrous Web: Roadmap towards Next-Gen 3D Excellence

Currently at **Phase 5: Scene Persistence Pro**.
Atmospheric controls (fog, exposure) are fully dynamic.
A fluent, chainable API has been introduced to the WASM bridge.
Full scene save/load is now possible via JSON serialization.

### Key Milestones
- [x] Phase 1: Robust initialization & alignment.
- [x] Phase 2: Modular rendering architecture.
- [x] Phase 3: Dynamic Visuals & Fluent API.
- [x] Phase 4: Advanced Material System & Asset Pro.
- [x] Phase 5: Scene Persistence Pro.

---

## 🏗️ Fase 1: Cimentación y Estabilidad ✅
*Objetivo: Lograr un entorno de ejecución WebGPU/WASM robusto y sin fricción.*

- [x] **Runtime WebGPU/WASM:** Ejecución estable en navegadores modernos.
- [x] **Auto-Resizing Dinámico:** Sincronización perfecta entre el layout del DOM y el viewport.
- [x] **Sistema de Input Unificado:** Soporte WASD, Mouse Orbit y Pointer Lock.
- [x] **Depuración Integrada:** Overlay de FPS y métricas de ECS visibles en tiempo real.

---

## 🔌 Fase 2: Experiencia de Desarrollador (DX) y Bridge ✅
*Objetivo: Hacer que usar Ferrous desde JavaScript sea tan fácil como Three.js.*

- [x] **Fluent JS API:** Refactorizar el bridge para permitir llamadas encadenadas.
- [x] **Asset Management Pro:** Carga asíncrona de texturas y modelos con promesas nativas.
- [x] **Hot-Reloading de Props:** Cambiar parámetros de motores (velocidad, colores, luces) desde React.
- [x] **Typescript Definitions:** Definiciones @types completas para todos los módulos (`crates/ferrous_web/types`).

---

## ✨ Fase 3: Visual Fidelity & API Expansion ✅
*Objetivo: Alcanzar la paridad de características visuales con motores triple-A.*

- [x] **Dynamic Camera Uniforms**: Expanded uniform buffer to support exposure and fog.
- [x] **Post-Process Integration**: Tone-mapping pass uses global exposure.
- [x] **Atmospheric Controls**: Implemented real-time fog and exposure settings.

---

## ⚡ Fase 4: Advanced Material System & Asset Pro ✅
*Objetivo: Sistema de materiales profesional y carga asíncrona robusta.*

- [x] **Material Descriptor Expansion**: Support emissive, clearcoat, and opacity.
- [x] **Asset Management Pro**: Unified promise-based loading for textures/models.
- [x] **Dynamic Updates**: Real-time material modification via JS API.

---

## 💾 Fase 5: Scene Persistence Pro ✅
*Objetivo: Guardado y carga de mundos complejos de forma modular y persistente.*

- [x] **Serialization Infrastructure**: Integrated `serde` across all core engine types (Transform, Material, Light).
- [x] **SceneBlueprint**: Robust data container for serializing complex scenes to JSON.
- [x] **persistence API**: New `export_scene()` and `import_scene()` methods in `FerrousWebEngine`.
- [x] **ECS Persistence**: Bridge between ECS state and serializable blueprint for full world reconstruction.

---

## 🎯 Visión Final
Ferrous Web no es una librería más; es la transición hacia una web donde las aplicaciones no "intentan" emular gráficos de escritorio, sino que **son** aplicaciones nativas corriendo dentro de un estándar abierto. 

**Rust + WebGPU + ECS** es la fórmula que hará que las experiencias 3D en el navegador dejen de sentirse como juguetes y empiecen a sentirse como el futuro.
