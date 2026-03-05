use ferrous_core::Color;
use ferrous_core::RenderQuality;
use ferrous_renderer::RenderStyle;

use crate::traits::FerrousApp;

/// Configuración inicial de la ventana y el motor.
#[derive(Clone)]
pub struct AppConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Optional path to a `.ttf`/`.otf` font loaded asynchronously at startup.
    /// On wasm32 this field is ignored; use `font_bytes` instead.
    pub font_path: Option<String>,
    /// Optional raw font bytes (e.g. from `include_bytes!`).  Takes priority
    /// over `font_path` and works on every platform including wasm32.
    pub font_bytes: Option<&'static [u8]>,
    /// Enable vertical sync — locks GPU present to monitor refresh rate,
    /// eliminating tearing and capping GPU usage to the refresh rate.
    /// Default: `true`.
    pub vsync: bool,
    /// Whether the user can resize the window. Defaults to `true`.
    pub resizable: bool,
    /// Color used to clear the 3-D viewport every frame.
    pub background_color: Color,
    /// Maximum frames per second.  The runner sleeps the remainder of each
    /// frame budget on the CPU, keeping usage low during idle scenes.
    ///
    /// `None` = unlimited (busy-loop, maximum throughput at the cost of CPU).
    /// Default: `Some(60)`.
    pub target_fps: Option<u32>,
    /// How many seconds of no input before the application drops to idle
    /// (stops continuous rendering) to save CPU/GPU usage.
    /// Useful for editors or UI-heavy apps.
    /// `None` = never goes idle (always continuously redraws).
    pub idle_timeout: Option<f32>,
    /// MSAA sample count: `1` = no anti-aliasing, `4` = 4x MSAA.
    ///
    /// Higher values improve edge quality but multiply GPU raster work.
    /// Default: `1` (no MSAA) — enable explicitly when needed.
    pub sample_count: u32,
    /// Optional path to an HDR environment map.  If provided the renderer
    /// will initialise its IBL resources from this file.
    pub hdri_path: Option<String>,
    /// Default render style applied after the renderer is created.
    ///
    /// `RenderStyle::Pbr` — full PBR shading (default).
    /// `RenderStyle::CelShaded` — toon-ramp shading + optional outline.
    /// `RenderStyle::FlatShaded` — faceted / low-poly flat shading.
    pub render_style: RenderStyle,
    /// Render quality preset — controls which passes are active and at what
    /// resolution they run.  Defaults to [`RenderQuality::High`].
    ///
    /// When loaded from `ferrous.toml` via [`AppBuilder::with_config_file`],
    /// the quality preset also drives the default MSAA sample count unless
    /// `msaa` is explicitly set in the TOML.
    pub render_quality: RenderQuality,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: "Ferrous Application".to_string(),
            width: 1280,
            height: 720,
            font_path: None,
            font_bytes: None,
            vsync: true,
            resizable: true,
            background_color: Color::rgb(0.1, 0.1, 0.1),
            target_fps: Some(60),
            idle_timeout: None,
            sample_count: 1,
            hdri_path: None,
            render_style: RenderStyle::Pbr,
            render_quality: RenderQuality::High,
        }
    }
}

/// The main entry-point builder.  Follows the fluent builder pattern so
/// configuration reads naturally:
///
/// ```rust,ignore
/// App::new(MyGame::default())
///     .with_title("My Game")
///     .with_size(1920, 1080)
///     .with_background_color(Color::SKY_BLUE)
///     .run();
/// ```
pub struct App<A: FerrousApp> {
    config: AppConfig,
    app_state: A,
}

impl<A: FerrousApp + 'static> App<A> {
    pub fn new(app_state: A) -> Self {
        Self {
            config: AppConfig::default(),
            app_state,
        }
    }

    /// Set the window title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.config.title = title.to_string();
        self
    }

    /// Set the initial window size in logical pixels.
    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    /// Path to a font file to load at startup (optional).
    /// On wasm32 this is ignored; use [`with_font_bytes`] instead.
    pub fn with_font(mut self, path: &str) -> Self {
        self.config.font_path = Some(path.to_string());
        self
    }

    /// Embed a font directly as bytes (e.g. via `include_bytes!`).
    /// Works on every platform and takes priority over `with_font`.
    ///
    /// # Example
    /// ```rust,ignore
    /// App::new(MyGame)
    ///     .with_font_bytes(include_bytes!("../../assets/fonts/Roboto-Regular.ttf"))
    ///     .run();
    /// ```
    pub fn with_font_bytes(mut self, bytes: &'static [u8]) -> Self {
        self.config.font_bytes = Some(bytes);
        self
    }

    /// Enable or disable vertical sync (default: enabled).
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.config.vsync = vsync;
        self
    }

    /// Allow or prevent the user from resizing the window (default: allowed).
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.config.resizable = resizable;
        self
    }

    /// Colour used to clear the 3-D viewport before each frame (default: dark gray).
    pub fn with_background_color(mut self, color: Color) -> Self {
        self.config.background_color = color;
        self
    }

    /// Set the HDRI file path used for image-based lighting.
    /// Path is resolved relative to the working directory at runtime.
    pub fn with_hdri(mut self, path: &str) -> Self {
        self.config.hdri_path = Some(path.to_string());
        self
    }

    /// Set the initial render style.
    ///
    /// Defaults to [`RenderStyle::Pbr`] (full PBR).
    ///
    /// # Example
    /// ```rust,ignore
    /// App::new(MyGame)
    ///     .with_render_style(RenderStyle::CelShaded { toon_levels: 4, outline_width: 0.02 })
    ///     .run();
    /// ```
    pub fn with_render_style(mut self, style: RenderStyle) -> Self {
        self.config.render_style = style;
        self
    }

    /// Set the render quality preset.
    ///
    /// Defaults to [`RenderQuality::High`].  When set programmatically (rather
    /// than via `ferrous.toml`) the MSAA sample count is *not* automatically
    /// updated — call [`with_msaa`][Self::with_msaa] explicitly if needed.
    pub fn with_render_quality(mut self, quality: RenderQuality) -> Self {
        self.config.render_quality = quality;
        self
    }

    /// Load `ferrous.toml` from `path` and apply all recognised settings to
    /// this app's configuration.
    ///
    /// Missing files are silently ignored (returns `self` unchanged).  Parse
    /// errors are logged via `log::warn!` and also ignored so that a broken
    /// config file does not crash the application at startup.
    ///
    /// This method should be called **before** any explicit `with_*` overrides
    /// so that code-level settings take priority over the file.
    ///
    /// # Example
    /// ```rust,ignore
    /// App::new(MyGame)
    ///     .with_config_file("ferrous.toml")   // loaded first
    ///     .with_title("Override Title")        // code wins
    ///     .run();
    /// ```
    pub fn with_config_file(mut self, path: &str) -> Self {
        match crate::config::load_config(path) {
            Ok(engine_cfg) => engine_cfg.apply_to(&mut self.config),
            Err(e) => log::warn!("ferrous.toml: {e}"),
        }
        self
    }

    /// Cap the frame rate.  The runner sleeps unused CPU time each frame.
    ///
    /// `None` disables the cap (maximum throughput, higher CPU usage).
    pub fn with_target_fps(mut self, fps: Option<u32>) -> Self {
        self.config.target_fps = fps;
        self
    }

    /// Sets how long (in seconds) the application expects no input before
    /// stopping continuous redraws. `None` disables idling.
    pub fn with_idle_timeout(mut self, timeout: Option<f32>) -> Self {
        self.config.idle_timeout = timeout;
        self
    }

    /// Set MSAA sample count (`1` = off, `4` = 4x).
    ///
    /// 4x MSAA significantly increases GPU raster cost; only enable it when
    /// edge quality is critical.
    pub fn with_msaa(mut self, sample_count: u32) -> Self {
        self.config.sample_count = sample_count;
        self
    }

    /// Start the event loop. This call blocks until the window is closed.
    pub fn run(self) {
        crate::runner::run_internal(self.config, self.app_state);
    }
}
