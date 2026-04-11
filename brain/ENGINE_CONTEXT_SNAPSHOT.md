# 🧠 Ferrous Web: Context Snapshot (Para Reset de Sesión)

Este archivo contiene el estado técnico comprimido del motor para que, si mi memoria se reinicia, pueda leerlo y volver al 100% de productividad instantáneamente.

## 🛠️ Estado Técnico Actual

### 1. API Fluente y DX (Fase 2 ✅)
- **JsEntity:** Se ha implementado un sistema de entidades fluente que permite encadenar comandos desde JavaScript.
- **Transformaciones:** Comandos separados para `set_position`, `set_rotation` y `set_scale` para evitar sobreescrituras accidentales.
- **Ejemplo:** `engine.createBox(...).set_position(0,2,0).set_color(1,0,0)`.

### 2. Atmósfera y Fidelidad Visual (Fase 3 ✅)
- **Dynamic Camera Uniforms:** El buffer de uniformes de la cámara ahora soporta `exposure`, `fog_color` y `fog_density` en una estructura alineada de 256 bytes.
- **Shaders PBR:** El shader `pbr.wgsl` utiliza neblina por distancia configurada dinámicamente.
- **Post-Process:** El pipeline de post-procesado ahora acepta el bind group de la cámara para aplicar exposición HDR dinámica en el paso de tone-mapping (ACES).

### 3. Sistema de Materiales Avanzado (Fase 4 ✅)
- **Carga de Assets (Pro):** Sistema asíncrono basado en promesas de JS (`load_texture`, `load_model`). Los assets se cargan en hilos de Rust y resuelven la promesa en la UI una vez subidos a la GPU.
- **Materiales 2.0:** Soporte para texturas de albedo, clearcoat, clearcoat_roughness y opacidad. API fluente completa: `set_color`, `set_material`, `set_texture`, `set_clearcoat`, `set_opacity`.
- **Plugin System v1:** Interfaz de extensión con hooks `on_update` y `on_sync_world`. Puente JS funcional vía `engine.register_plugin`.
- **Pipeline:** PBR con IBL e iluminación dinámica, SSAO integrado.
- **Arquitectura:** Desacoplo vía `JsCommand`. Global state `ASSET_RESOLVERS` y `JsWebPlugin` bridge.
- **API Fluente:** Nuevos métodos `set_clearcoat(factor, roughness)` y `set_opacity(v)` añadidos a `JsEntity`.

### 4. Sincronización de Pantalla y UI
- **Auto-Sync:** Comprobación proactiva cada frame de `window.inner_size()` contra el estado interno de WGPU. Corrige glitches visuales de inicio.

## 📂 Archivos Críticos
1. `crates/ferrous_web/src/engine.rs`: API Fluente y entrada WASM.
2. `crates/ferrous_renderer/src/resources/material.rs`: Definición de uniformes y lógica de materiales.
3. `assets/shaders/pbr.wgsl`: Shader PBR con soporte para atmósfera y materiales avanzados.
4. `scripts/check_ferrous_web_exports.py`: Validador de consistencia de la API (soporta múltiples clases).

## 🚀 Próxima Tarea (Fase 5: GPU-Driven Features)
Implementar **Compute Skinning** y **GPU Particle Systems** para maximizar el uso de la GPU en escenas masivas, moviendo la lógica de animación y simulación fuera del hilo principal de la CPU.

---
*Snapshot actualizado el 2026-04-11 tras completar la Fase 4 satisfactoriamente.*
