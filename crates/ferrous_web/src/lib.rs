//! `ferrous_web` — WASM/WebGPU bridge for Ferrous Engine.
//!
//! # Architecture
//!
//! ```text
//! ferrous_web  (this crate)
//! ├── engine/          — FerrousWebEngine struct + lifecycle
//! │   ├── mod.rs       — struct definition, constructor, mountAndRun
//! │   ├── api_primitives.rs  — createBox, createSphere, createCylinder …
//! │   ├── api_scene.rs       — createScene, exportScene, importScene …
//! │   ├── api_camera.rs      — setCamera, setCameraFov, configureControls …
//! │   ├── api_lighting.rs    — addPointLight, setDirectionalLight …
//! │   ├── api_materials.rs   — updateMaterial, loadTexture, setTransform …
//! │   └── api_environment.rs — setEnvironment, setExposure, setBackground …
//! ├── entity.rs       — JsEntity handle (chainable API)
//! ├── commands.rs     — JsCommand enum (all variants)
//! ├── dispatcher.rs   — executes JsCommands against AppContext
//! ├── runtime.rs      — FerrousApp impl, asset polling, plugin hooks
//! ├── camera.rs       — CameraController (fly / orbit / none)
//! ├── config.rs       — EngineConfig, EngineMetrics, CameraControlMode
//! └── plugin.rs       — WebPlugin trait + JsWebPlugin
//! ```

mod camera;
mod commands;
mod config;
mod dispatcher;
mod engine;
mod entity;
mod plugin;
mod runtime;

pub use engine::FerrousWebEngine;
pub use entity::JsEntity;
