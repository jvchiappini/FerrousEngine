//! Plugin system for FerrousEngine.
//!
//! A **plugin** is any type that implements [`Plugin`].  Plugins are registered
//! on an [`AppBuilder`] before the event loop starts; each plugin's
//! [`Plugin::build`] method is called in registration order.
//!
//! # Example
//!
//! ```rust,ignore
//! use ferrous_app::{App, AppBuilder, Plugin, DefaultPlugins};
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn name(&self) -> &'static str { "MyPlugin" }
//!
//!     fn build(&self, app: &mut AppBuilder) {
//!         app.add_system(ferrous_ecs::prelude::Stage::Update, my_system);
//!     }
//! }
//!
//! fn main() {
//!     AppBuilder::new()
//!         .add_plugin(DefaultPlugins)
//!         .add_plugin(MyPlugin)
//!         .run();
//! }
//! ```

use ferrous_ecs::prelude::Stage;
use ferrous_ecs::system::System;

use crate::builder::AppConfig;

// ── Plugin trait ─────────────────────────────────────────────────────────────

/// The core extensibility mechanism for FerrousEngine.
///
/// Implement this trait to bundle a group of systems, resources, or render
/// passes under a single name.  Plugins are registered on [`AppBuilder`] via
/// [`AppBuilder::add_plugin`].
pub trait Plugin: 'static {
    /// A unique, human-readable name used in debug output.
    fn name(&self) -> &'static str;

    /// Called once during [`AppBuilder::run`] (before the event loop starts).
    ///
    /// Register systems, modify config, and add render passes here.
    fn build(&self, app: &mut AppBuilder);

    /// Called when the application is shutting down.  Optional cleanup.
    fn cleanup(&self, _app: &mut AppBuilder) {}
}

// ── AppBuilder ────────────────────────────────────────────────────────────────

/// Composable application builder.
///
/// Unlike [`App`][crate::builder::App] (which requires a concrete
/// [`FerrousApp`][crate::traits::FerrousApp] type), `AppBuilder` builds an
/// application from a collection of plugins.
///
/// ```rust,ignore
/// AppBuilder::new()
///     .add_plugin(DefaultPlugins)
///     .add_plugin(MyGamePlugin)
///     .run();
/// ```
pub struct AppBuilder {
    /// Accumulated configuration — plugins can mutate this.
    pub config: AppConfig,

    /// Staged systems to add.  Each entry is `(stage, boxed_system)`.
    pub(crate) staged_systems: Vec<(Stage, Box<dyn System>)>,

    /// Names of registered plugins (for duplicate detection / debug).
    registered_names: Vec<&'static str>,
}

impl AppBuilder {
    /// Create an empty builder with default [`AppConfig`].
    pub fn new() -> Self {
        Self {
            config: AppConfig::default(),
            staged_systems: Vec::new(),
            registered_names: Vec::new(),
        }
    }

    // ── Config helpers ────────────────────────────────────────────────────

    /// Override the window title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.config.title = title.to_string();
        self
    }

    /// Override window dimensions.
    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    /// Apply a full [`AppConfig`] (replaces the default).
    pub fn with_config(mut self, config: AppConfig) -> Self {
        self.config = config;
        self
    }

    // ── Plugin registration ────────────────────────────────────────────────

    /// Register a plugin.  Calls [`Plugin::build`] immediately so that
    /// inner `add_plugin` / `add_system` calls take effect in order.
    ///
    /// # Panics
    /// Panics if the same plugin name is registered twice (prevents
    /// accidental double-registration of `DefaultPlugins` etc.).
    pub fn add_plugin(mut self, plugin: impl Plugin) -> Self {
        let name = plugin.name();
        assert!(
            !self.registered_names.contains(&name),
            "Plugin '{name}' registered twice — each plugin may only be added once"
        );
        self.registered_names.push(name);
        plugin.build(&mut self);
        self
    }

    /// Add a system function at a specific stage.
    ///
    /// The system is stored as a boxed `FnMut` closure and registered into
    /// the [`StagedScheduler`][ferrous_ecs::prelude::StagedScheduler] before
    /// the first frame.
    ///
    /// ```rust,ignore
    /// app.add_system(Stage::Update, |world: &mut ferrous_ecs::World, res: &mut ResourceMap| {
    ///     // ...
    /// });
    /// ```
    pub fn add_system<S: System>(mut self, stage: Stage, system: S) -> Self {
        self.staged_systems.push((stage, Box::new(system)));
        self
    }

    /// Add a pre-boxed system (builder-pattern helper used by plugin impls
    /// that call `app.add_system_boxed(...)` from `&mut self` context).
    pub(crate) fn add_system_boxed(&mut self, stage: Stage, system: Box<dyn System>) {
        self.staged_systems.push((stage, system));
    }

    // ── Execution ─────────────────────────────────────────────────────────

    /// Start the event loop.  This call blocks until the window is closed.
    ///
    /// Internally this constructs a [`PluginRunner`][crate::runner::PluginRunner]
    /// and runs it through winit's event loop.
    pub fn run(self) {
        crate::runner::run_plugin_app(self);
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Built-in plugins ──────────────────────────────────────────────────────────

/// Registers core ECS systems: `TimeSystem`, `VelocitySystem`,
/// `AnimationSystem`, `BehaviorSystem`, `TransformSystem`.
///
/// These correspond to the stages `PreUpdate → Update → PostUpdate`.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn name(&self) -> &'static str {
        "CorePlugin"
    }

    fn build(&self, app: &mut AppBuilder) {
        use ferrous_core::{AnimationSystem, BehaviorSystem, TimeSystem, TransformSystem, VelocitySystem};

        app.add_system_boxed(Stage::PreUpdate, Box::new(TimeSystem));
        app.add_system_boxed(Stage::Update, Box::new(VelocitySystem));
        app.add_system_boxed(Stage::Update, Box::new(AnimationSystem));
        app.add_system_boxed(Stage::Update, Box::new(BehaviorSystem));
        app.add_system_boxed(Stage::PostUpdate, Box::new(TransformSystem));
    }
}

/// Configures the window parameters from [`AppConfig`].
///
/// This plugin is essentially a marker — window creation is handled by the
/// runner's `resumed` callback.  Use it to distinguish between headless and
/// windowed configurations in the future.
pub struct WindowPlugin {
    /// Override the window title.  `None` uses [`AppConfig::title`].
    pub title: Option<String>,
    /// Override window width.  `None` uses [`AppConfig::width`].
    pub width: Option<u32>,
    /// Override window height.  `None` uses [`AppConfig::height`].
    pub height: Option<u32>,
}

impl Default for WindowPlugin {
    fn default() -> Self {
        Self {
            title: None,
            width: None,
            height: None,
        }
    }
}

impl Plugin for WindowPlugin {
    fn name(&self) -> &'static str {
        "WindowPlugin"
    }

    fn build(&self, app: &mut AppBuilder) {
        if let Some(title) = &self.title {
            app.config.title = title.clone();
        }
        if let Some(w) = self.width {
            app.config.width = w;
        }
        if let Some(h) = self.height {
            app.config.height = h;
        }
    }
}

/// Enables the keyboard and mouse input subsystem.
///
/// Currently a marker plugin — input is always active in the runner.
/// In the future this could be made optional (e.g. for headless servers).
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn name(&self) -> &'static str {
        "InputPlugin"
    }

    fn build(&self, _app: &mut AppBuilder) {
        // Input subsystem is always active via InputState in the runner.
        // This plugin exists for composability and future optionality.
    }
}

/// Enables the frame-time clock (`TimeSystem`).
///
/// Currently a marker plugin — `TimeSystem` is also registered by [`CorePlugin`].
/// Use `TimePlugin` directly when you want *only* timing without the full
/// core suite.
pub struct TimePlugin;

impl Plugin for TimePlugin {
    fn name(&self) -> &'static str {
        "TimePlugin"
    }

    fn build(&self, app: &mut AppBuilder) {
        use ferrous_core::TimeSystem;
        app.add_system_boxed(Stage::PreUpdate, Box::new(TimeSystem));
    }
}

/// Enables the renderer (PBR pipeline, SSAO, bloom, etc.).
///
/// Renderer initialisation still happens in the runner's `resumed` callback;
/// this plugin configures the quality/style settings via [`AppConfig`].
pub struct RendererPlugin {
    /// Render style to apply after renderer creation.
    pub render_style: ferrous_renderer::RenderStyle,
    /// Optional HDRI path for IBL.
    pub hdri_path: Option<String>,
    /// MSAA sample count.
    pub sample_count: u32,
}

impl Default for RendererPlugin {
    fn default() -> Self {
        Self {
            render_style: ferrous_renderer::RenderStyle::Pbr,
            hdri_path: None,
            sample_count: 1,
        }
    }
}

impl Plugin for RendererPlugin {
    fn name(&self) -> &'static str {
        "RendererPlugin"
    }

    fn build(&self, app: &mut AppBuilder) {
        app.config.render_style = self.render_style.clone();
        app.config.hdri_path = self.hdri_path.clone();
        app.config.sample_count = self.sample_count;
    }
}

/// Enables the GUI subsystem (`ferrous_gui`).
///
/// Currently a marker plugin — the GUI is always initialised in the runner.
/// In the future this could be optional for headless or server builds.
pub struct GuiPlugin;

impl Plugin for GuiPlugin {
    fn name(&self) -> &'static str {
        "GuiPlugin"
    }

    fn build(&self, _app: &mut AppBuilder) {
        // GUI is always active via Ui in the runner.
    }
}

/// Enables the asset server with optional hot-reload.
///
/// Currently a marker plugin — the `AssetServer` is always active in the runner.
pub struct AssetPlugin;

impl Plugin for AssetPlugin {
    fn name(&self) -> &'static str {
        "AssetPlugin"
    }

    fn build(&self, _app: &mut AppBuilder) {
        // AssetServer is always active in the runner.
    }
}

/// Convenience bundle that registers all standard engine plugins:
///
/// - [`CorePlugin`] — ECS systems (time, velocity, animation, transform)
/// - [`WindowPlugin`] — window configuration
/// - [`InputPlugin`] — keyboard + mouse input
/// - [`AssetPlugin`] — async asset server
/// - [`RendererPlugin`] — PBR renderer
/// - [`GuiPlugin`] — GUI subsystem
///
/// # Example
///
/// ```rust,ignore
/// AppBuilder::new()
///     .add_plugin(DefaultPlugins)
///     .run();
/// ```
pub struct DefaultPlugins;

impl Plugin for DefaultPlugins {
    fn name(&self) -> &'static str {
        "DefaultPlugins"
    }

    fn build(&self, app: &mut AppBuilder) {
        // Register sub-plugins without going through add_plugin (which checks
        // for duplicates by name), since DefaultPlugins is already registered.
        CorePlugin.build(app);
        // WindowPlugin, InputPlugin, AssetPlugin, RendererPlugin, GuiPlugin
        // are marker plugins — calling build() directly is fine.
        WindowPlugin::default().build(app);
        InputPlugin.build(app);
        AssetPlugin.build(app);
        RendererPlugin::default().build(app);
        GuiPlugin.build(app);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_builder_default_config() {
        let app = AppBuilder::new();
        assert_eq!(app.config.title, "Ferrous Application");
        assert_eq!(app.config.width, 1280);
        assert_eq!(app.config.height, 720);
    }

    #[test]
    fn app_builder_with_title() {
        let app = AppBuilder::new().with_title("My Game");
        assert_eq!(app.config.title, "My Game");
    }

    #[test]
    fn app_builder_with_size() {
        let app = AppBuilder::new().with_size(1920, 1080);
        assert_eq!(app.config.width, 1920);
        assert_eq!(app.config.height, 1080);
    }

    #[test]
    fn plugin_name_dedup() {
        // Registering the same plugin twice should panic.
        let result = std::panic::catch_unwind(|| {
            AppBuilder::new()
                .add_plugin(InputPlugin)
                .add_plugin(InputPlugin);
        });
        assert!(result.is_err(), "Expected panic on duplicate plugin");
    }

    #[test]
    fn custom_plugin_build_called() {
        use std::sync::{Arc, Mutex};

        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();

        struct MyPlugin {
            called: Arc<Mutex<bool>>,
        }
        impl Plugin for MyPlugin {
            fn name(&self) -> &'static str {
                "MyPlugin"
            }
            fn build(&self, _app: &mut AppBuilder) {
                *self.called.lock().unwrap() = true;
            }
        }

        AppBuilder::new().add_plugin(MyPlugin { called: called_clone });
        assert!(*called.lock().unwrap(), "build() was not called");
    }

    #[test]
    fn window_plugin_overrides_config() {
        let app = AppBuilder::new().add_plugin(WindowPlugin {
            title: Some("Custom".to_string()),
            width: Some(800),
            height: Some(600),
        });
        assert_eq!(app.config.title, "Custom");
        assert_eq!(app.config.width, 800);
        assert_eq!(app.config.height, 600);
    }

    #[test]
    fn default_plugins_registers_systems() {
        let app = AppBuilder::new().add_plugin(DefaultPlugins);
        // CorePlugin registers 5 systems (Time + Velocity + Animation + Behavior + Transform)
        assert_eq!(app.staged_systems.len(), 5);
    }

    #[test]
    fn renderer_plugin_sets_render_style() {
        use ferrous_renderer::RenderStyle;
        let app = AppBuilder::new().add_plugin(RendererPlugin {
            render_style: RenderStyle::FlatShaded,
            hdri_path: None,
            sample_count: 4,
        });
        assert_eq!(app.config.render_style, RenderStyle::FlatShaded);
        assert_eq!(app.config.sample_count, 4);
    }
}
