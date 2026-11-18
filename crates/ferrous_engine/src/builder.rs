use ferrous_core::Color;
use ferrous_core::RenderQuality;
use ferrous_renderer::RenderStyle;

use crate::redraw::RedrawPolicy;
use crate::traits::FerrousApp;

/// Selects which engine subsystems the runner initialises and which frame
/// callbacks it invokes.
///
/// | Mode | 3D renderer | ECS world sync | `draw_3d` called |
/// |------|-------------|----------------|------------------|
/// | `Flat2D` | ✓ (2-D only) | ✗ | ✗ |
/// | `Flat2D` | ✓ | ✗ | ✗ |
/// | `Game3D` | ✓ | ✓ | ✓ |
///
/// The renderer is always present so `draw_ui` / fonts / GUI work in every
/// mode.  Only `Game3D` runs the full ECS→renderer sync and invokes `draw_3d`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppMode {
    /// Full 3-D game: ECS world sync, camera input, `draw_3d` callback.
    #[default]
    Game3D,
    /// Flat 2-D mode: renderer active in 2D fast-path, no 3-D world sync,
    /// no camera input, no `draw_3d` callback.
    Flat2D,
}

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
    /// Whether the window should show OS-native decorations (title bar, minimize /
    /// maximise / close buttons).  Set to `false` to get a bare borderless
    /// window — useful when you want to draw a fully-custom title bar with
    /// `ferrous_gui`.  Defaults to `true`.
    pub decorations: bool,
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
    /// Frame rate kept while idle (see [`idle_timeout`](Self::idle_timeout)).
    ///
    /// `None` = stop redrawing completely until the next input event.
    /// `Some(n)` = keep drawing at `n` fps, so time-driven UI (caret blink,
    /// messages from other windows, async loads) still shows up — at a
    /// fraction of the full-rate cost.
    ///
    /// These two fields are the older, timer-based throttle for
    /// [`RedrawPolicy::Continuous`] apps. [`RedrawPolicy::OnDemand`] replaces
    /// them: the loop sleeps until an input, a `request_redraw`/`redraw_at`
    /// wake or a worker event, and the fields are ignored.
    pub idle_fps: Option<u32>,
    /// When the runner draws frames (see [`RedrawPolicy`]).
    ///
    /// Default: [`RedrawPolicy::Continuous`], which keeps the historical
    /// `target_fps` loop for games and demos. Editors and tools that only
    /// change in response to input should opt into
    /// [`RedrawPolicy::OnDemand`]: an idle app then presents zero frames and
    /// submits no GPU work.
    pub redraw_policy: RedrawPolicy,
    /// MSAA sample count: `1` = no anti-aliasing, `4` = 4x MSAA.
    ///
    /// Higher values improve edge quality but multiply GPU raster work.
    /// Default: `1` (no MSAA) — enable explicitly when needed.
    pub sample_count: u32,
    /// Which engine subsystems to activate.  See [`AppMode`] for details.
    ///
    /// Default: [`AppMode::Game3D`].  Set to [`AppMode::Flat2D`] for
    /// tools that only need GUI, or [`AppMode::Flat2D`] for 2-D games.
    pub mode: AppMode,
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
    /// Resolution scale applied to the physical size of the window/canvas.
    /// Defaults to 1.0. Set to 2.0 or higher for supersampling (improves clarity on Web).
    pub resolution_scale: f64,
    /// Record a startup trace and print it at the first presented frame.
    ///
    /// Default: `false`. The `FERROUS_TRACE_STARTUP=1` environment variable
    /// enables the same trace without a code change; see
    /// [`ferrous_core::startup`].
    pub startup_trace: bool,
    /// On-disk cache for baked font atlases.
    ///
    /// `None` (the default) bakes every font on every launch.  Applications opt
    /// in with [`ferrous_font::FontCacheConfig::for_app`] so the second launch
    /// reads the MSDF atlas from disk instead of regenerating it.
    pub font_cache: Option<ferrous_font::FontCacheConfig>,
    /// Backends to try at startup, in order.
    ///
    /// Each entry gets its own `wgpu::Instance`; the engine moves to the next
    /// one only when the current one yields no adapter (or an adapter that
    /// cannot present on the window).  Default: Vulkan, then DX12, then GL on
    /// Windows; Metal on macOS; Vulkan, then GL elsewhere.  `WGPU_BACKEND`, when
    /// set, replaces the list with exactly those backends.
    pub backends: Vec<wgpu::Backends>,
    /// Which GPU to prefer.  Default: [`wgpu::PowerPreference::HighPerformance`].
    ///
    /// `LowPower` selects the integrated GPU on a hybrid laptop; `WGPU_POWER_PREF`
    /// (`low`, `high`, `none`) overrides this at runtime.
    pub power_preference: wgpu::PowerPreference,
    /// Persist the wgpu pipeline cache between runs.
    ///
    /// `None` (the default) recompiles every pipeline on every launch.  Only
    /// backends that expose [`wgpu::Features::PIPELINE_CACHE`] (Vulkan in wgpu
    /// 23) use it; DX12, Metal and GL ignore it.
    pub pipeline_cache: Option<PipelineCacheConfig>,
    /// Whether the OS window is ever shown.
    ///
    /// Default `true`. `false` creates the window hidden and never maps it:
    /// frames still render and present to the hidden surface, but the window
    /// never receives input, focus or compositor work. Benchmarks and CI use
    /// it to measure an app's idle cost without a person or another window
    /// interfering.
    pub visible: bool,
    /// Show the window only when the first frame is ready (prompt 17 S3b).
    ///
    /// Default `true`: the window is created hidden, the GPU state and the
    /// first frame are prepared while it is hidden, and it is shown right after
    /// that frame is presented, so the first thing the user sees is the
    /// application, not a white or black rectangle. `false` restores the S3
    /// behaviour (show it with the background colour immediately).
    pub show_window_when_ready: bool,
    /// Safety timeout for [`show_window_when_ready`](Self::show_window_when_ready).
    ///
    /// If the first frame has not been presented after this long (a cold or
    /// software driver, a slow shader compile), the window is shown with
    /// [`background_color`](Self::background_color) so the user knows the app
    /// opened. Default: 800 ms from the start of the run. `FERROUS_SHOW_TIMEOUT_MS`
    /// overrides it at runtime.
    pub window_show_timeout: std::time::Duration,
}

/// How long the window stays hidden waiting for the first frame by default.
///
/// See [`AppConfig::window_show_timeout`].
pub const DEFAULT_SHOW_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(800);

/// Which backends a launch tries, in order.
///
/// The list is per platform; an application can replace it with
/// [`App::with_backends`].  The engine creates one instance per entry and only
/// advances when the previous entry has no usable adapter, so the common case
/// (Vulkan on Windows) never pays for enumerating DX12 and GL.
#[must_use]
pub fn default_backends() -> Vec<wgpu::Backends> {
    #[cfg(target_os = "windows")]
    {
        vec![
            wgpu::Backends::VULKAN,
            wgpu::Backends::DX12,
            wgpu::Backends::GL,
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![wgpu::Backends::METAL]
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "macos"),
        not(target_arch = "wasm32")
    ))]
    {
        vec![wgpu::Backends::VULKAN, wgpu::Backends::GL]
    }
    #[cfg(target_arch = "wasm32")]
    {
        vec![wgpu::Backends::BROWSER_WEBGPU]
    }
}

/// Where the persisted wgpu pipeline cache lives.
///
/// The file is keyed by backend, vendor, device, driver and driver version, so
/// a driver update or a GPU change starts from an empty cache.  A corrupt or
/// stale file is ignored (wgpu compiles the pipelines again).
#[derive(Clone, Debug)]
pub struct PipelineCacheConfig {
    /// Whether the cache is used at all.
    pub enabled: bool,
    /// Directory holding the cache files; `None` means the per-OS default for
    /// [`Self::app_name`].
    pub directory: Option<std::path::PathBuf>,
    /// Application name used by the default directory.
    pub app_name: String,
}

impl PipelineCacheConfig {
    /// A cache for `app_name` under the per-OS default directory
    /// (`%LOCALAPPDATA%\<app>\cache\pipelines` on Windows).
    #[must_use]
    pub fn for_app(app_name: impl Into<String>) -> Self {
        Self {
            enabled: true,
            directory: None,
            app_name: app_name.into(),
        }
    }

    /// Uses `directory` instead of the per-OS default.
    #[must_use]
    pub fn with_directory(mut self, directory: impl Into<std::path::PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    /// A disabled cache.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            directory: None,
            app_name: String::new(),
        }
    }

    /// The directory this config resolves to, when the platform has one.
    #[must_use]
    pub fn resolve_directory(&self) -> Option<std::path::PathBuf> {
        if !self.enabled {
            return None;
        }
        if let Some(directory) = &self.directory {
            return Some(directory.clone());
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("LOCALAPPDATA").map(|base| {
                std::path::PathBuf::from(base)
                    .join(&self.app_name)
                    .join("cache")
                    .join("pipelines")
            })
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var_os("HOME").map(|home| {
                std::path::PathBuf::from(home)
                    .join("Library")
                    .join("Caches")
                    .join(&self.app_name)
                    .join("pipelines")
            })
        }
        #[cfg(all(
            not(target_os = "windows"),
            not(target_os = "macos"),
            not(target_arch = "wasm32")
        ))]
        {
            std::env::var_os("XDG_CACHE_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".cache"))
                })
                .map(|base| base.join(&self.app_name).join("pipelines"))
        }
    }
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
            decorations: true,
            background_color: Color::rgb(0.1, 0.1, 0.1),
            target_fps: Some(60),
            idle_timeout: None,
            idle_fps: None,
            redraw_policy: RedrawPolicy::Continuous,
            sample_count: 1,
            hdri_path: None,
            render_style: RenderStyle::Pbr,
            render_quality: RenderQuality::High,
            mode: AppMode::Game3D,
            resolution_scale: 1.0,
            startup_trace: false,
            font_cache: None,
            backends: default_backends(),
            power_preference: wgpu::PowerPreference::HighPerformance,
            pipeline_cache: None,
            visible: true,
            show_window_when_ready: true,
            window_show_timeout: DEFAULT_SHOW_TIMEOUT,
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

    /// Show or hide the OS-native window decorations (title bar, minimize /
    /// maximise / close buttons).  Defaults to `true` (decorated).
    ///
    /// Set to `false` when you want a fully-borderless window and intend to
    /// implement a custom title bar with `ferrous_gui`.
    ///
    /// # Example
    /// ```rust,ignore
    /// App::new(MyApp)
    ///     .with_decorations(false)
    ///     .run();
    /// ```
    pub fn with_decorations(mut self, decorations: bool) -> Self {
        self.config.decorations = decorations;
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

    /// Frame rate kept while idle. `None` stops redrawing until input
    /// arrives; `Some(n)` throttles to `n` fps. Only used with
    /// [`with_idle_timeout`](Self::with_idle_timeout).
    pub fn with_idle_fps(mut self, fps: Option<u32>) -> Self {
        self.config.idle_fps = fps;
        self
    }

    /// Chooses the frame scheduling policy (see [`RedrawPolicy`]).
    ///
    /// [`RedrawPolicy::OnDemand`] draws a frame only when input arrives, the
    /// app calls `ctx.request_redraw()` / `ctx.redraw_at(instant)`, a worker
    /// wakes the loop through the `Waker` obtained in `setup`, or the window
    /// changes. An idle app presents no frames.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::new(MyEditor)
    ///     .with_mode(AppMode::Flat2D)
    ///     .with_redraw_policy(RedrawPolicy::OnDemand)
    ///     .run();
    /// ```
    pub fn with_redraw_policy(mut self, policy: RedrawPolicy) -> Self {
        self.config.redraw_policy = policy;
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

    /// Set the application mode — controls which engine subsystems are active.
    ///
    /// | Mode | Use when |
    /// |------|----------|
    /// | [`AppMode::Game3D`] | 3-D game or simulation (default) |
    /// | [`AppMode::Flat2D`] | 2-D game, top-down, side-scroller |
    /// | [`AppMode::Flat2D`] | GUI tool, editor, desktop utility |
    ///
    /// # Example
    /// ```rust,ignore
    /// App::new(MyTool)
    ///     .with_mode(AppMode::Flat2D)
    ///     .run();
    /// ```
    pub fn with_mode(mut self, mode: crate::builder::AppMode) -> Self {
        self.config.mode = mode;
        self
    }

    /// Set a resolution multiplier for supersampling (e.g. 2.0 for 2x resolution).
    /// Highly recommended for web and high-DPI displays if rendering is pixelated.
    pub fn with_resolution_scale(mut self, scale: f64) -> Self {
        self.config.resolution_scale = scale;
        self
    }

    /// Turn the startup trace on or off (default: off).
    ///
    /// When on, the engine records marks for the window, GPU initialisation,
    /// font upload, app setup and the first presented frame, and prints a
    /// table to stderr at the first present. Applications can add their own
    /// marks with [`ferrous_core::startup::mark`].
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::new(MyApp).with_startup_trace(true).run();
    /// ```
    pub fn with_startup_trace(mut self, enable: bool) -> Self {
        self.config.startup_trace = enable;
        self
    }

    /// Cache baked font atlases on disk (default: off).
    ///
    /// The cache is keyed by the font bytes and the character set, so a launch
    /// after the first one uploads the stored MSDF atlas instead of baking it.
    /// A corrupt or outdated file is rebuilt; see [`ferrous_font::AtlasCache`].
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::new(MyApp)
    ///     .with_font_cache(ferrous_font::FontCacheConfig::for_app("MyApp"))
    ///     .run();
    /// ```
    pub fn with_font_cache(mut self, cache: ferrous_font::FontCacheConfig) -> Self {
        self.config.font_cache = Some(cache);
        self
    }

    /// Replaces the ordered backend list (see [`AppConfig::backends`]).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::new(MyApp)
    ///     .with_backends([wgpu::Backends::VULKAN, wgpu::Backends::DX12])
    ///     .run();
    /// ```
    pub fn with_backends(mut self, backends: impl IntoIterator<Item = wgpu::Backends>) -> Self {
        self.config.backends = backends.into_iter().collect();
        self
    }

    /// Prefers the discrete GPU ([`wgpu::PowerPreference::HighPerformance`],
    /// the default) or the integrated one ([`wgpu::PowerPreference::LowPower`]).
    ///
    /// `WGPU_POWER_PREF` (`high`, `low`, `none`) overrides this at runtime.
    pub fn with_power_preference(mut self, preference: wgpu::PowerPreference) -> Self {
        self.config.power_preference = preference;
        self
    }

    /// Persists the wgpu pipeline cache between runs
    /// ([`PipelineCacheConfig::for_app`]).
    ///
    /// Only backends that expose `PIPELINE_CACHE` (Vulkan in wgpu 23) benefit;
    /// the others ignore it.
    pub fn with_pipeline_cache(mut self, cache: PipelineCacheConfig) -> Self {
        self.config.pipeline_cache = Some(cache);
        self
    }

    /// Never maps the OS window (see [`AppConfig::visible`]).
    ///
    /// The app still initialises the GPU, renders and presents to the hidden
    /// surface, so its frame code runs exactly as in a normal session.
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.config.visible = visible;
        self
    }

    /// Shows the window only when the first frame is ready
    /// ([`AppConfig::show_window_when_ready`], default on).
    pub fn with_show_window_when_ready(mut self, show: bool) -> Self {
        self.config.show_window_when_ready = show;
        self
    }

    /// Sets the safety timeout for showing the window
    /// ([`AppConfig::window_show_timeout`], default 800 ms).
    pub fn with_window_show_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.window_show_timeout = timeout;
        self
    }

    /// Start the event loop. This call blocks until the window is closed.
    pub fn run(self) {
        crate::runner::run_internal(self.config, self.app_state);
    }
}
