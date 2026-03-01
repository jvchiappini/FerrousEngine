use ferrous_core::Color;

use crate::traits::FerrousApp;

/// Configuración inicial de la ventana y el motor.
#[derive(Clone)]
pub struct AppConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Optional path to a `.ttf`/`.otf` font loaded asynchronously at startup.
    pub font_path: Option<String>,
    /// Enable vertical sync (smooth but capped to monitor refresh rate).
    pub vsync: bool,
    /// Whether the user can resize the window. Defaults to `true`.
    pub resizable: bool,
    /// Color used to clear the 3-D viewport every frame.
    pub background_color: Color,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: "Ferrous Application".to_string(),
            width: 1280,
            height: 720,
            font_path: None,
            vsync: false,
            resizable: true,
            background_color: Color::rgb(0.1, 0.1, 0.1),
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
    pub fn with_font(mut self, path: &str) -> Self {
        self.config.font_path = Some(path.to_string());
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

    /// Start the event loop. This call blocks until the window is closed.
    pub fn run(self) {
        crate::runner::run_internal(self.config, self.app_state);
    }
}
