# 🚀 Ferrous WebEngine Roadmap

Vision: To create a high-performance, WebGPU-first 3D engine that leverages the safety and speed of Rust/WASM to provide a superior alternative to JavaScript-only libraries.

---

## 🟢 Phase 1: Core Stability & Native Interop (Finalized ✅)
*Focus: Establishing a rock-solid foundation for browser deployment.*

- [x] **WebGPU Initialization:** Robust fallbacks for modern browsers.
- [x] **Asynchronous WASM Loading:** Professional preloader with progress tracking.
- [x] **Pointer Lock & Focus:** Seamless capture of input for 3D navigation.
- [x] **Camera System:** Fly and Orbit controllers with JS-exposed parameters.
- [x] **Viewport Sync:** Proactive auto-resizing every frame to eliminate glitches.
- [x] **Debug Overlay:** Integrated FPS counter and basic render stats.

## 🟡 Phase 2: JavaScript Harmony & Fluent API (Finalized ✅)
*Focus: Making the engine accessible to Every Web Developer.*

- [x] **Fluent JS API:** Chainable object-oriented API (`engine.createBox().set_position()`).
- [x] **Modular Dispatcher:** Command-based architecture that decouples UI and Logic.
- [x] **Error Branding:** Structured JSON error reporting from Rust/WASM to JS.
- [ ] **Typescript Definitions:** (Planned) Complete `@types` for all engine modules.

## 🟠 Phase 3: Visual Fidelity & Atmosphere (Current)
*Focus: Achieving visual parity with top-tier 3D tools.*

- [x] **Dynamic Camera Uniforms:** Real-time control of exposure, fog color, and density.
- [x] **Distance Fog:** Dynamic, uniform-driven fog integrated into the PBR pipeline.
- [x] **HDR Pipeline:** ACES tone-mapping with dynamic exposure control.
- [ ] **GLTF 2.0 Full Support:** Native Rust loader for animations and skins.
- [ ] **HDRI Environments:** Skybox and IBL importance sampling.

## 🔴 Phase 4: Advanced Graphics & VFX
*Focus: Pushing the limits of WebGPU.*

- [ ] **Compute-based Post-Processing:** SSAO and Bloom optimizations via compute shaders.
- [ ] **GPU Particle System:** Handling 1,000,000+ particles management in GPU.
- [ ] **Skeletal Animation:** Hardware-accelerated skinning in Rust.

---

### 💡 The Ferrous Edge
*   **Zero GC Pressure:** All heavy scene updates happen in Rust.
*   **Fluent DX:** A modern, chainable API that feels native to JS developers.
*   **WebGPU First:** No legacy overhead; designed for the modern hardware era.

---
*Roadmap updated on 2026-04-11 post-Phase 3 completion.*
