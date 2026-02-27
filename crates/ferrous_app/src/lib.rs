//! Framework modular para crear aplicaciones y juegos con FerrousEngine.

pub mod builder;
pub mod context;
mod graphics; // <- NUEVO: Oculta la complejidad de WGPU
mod runner;
pub mod traits;

pub use builder::{App, AppConfig};
pub use context::AppContext;
pub use traits::FerrousApp;
