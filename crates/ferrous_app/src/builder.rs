use crate::traits::FerrousApp;

/// Configuración inicial de la ventana y el motor.
#[derive(Clone)]
pub struct AppConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub font_path: Option<String>,
    pub vsync: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: "Ferrous Application".to_string(),
            width: 1280,
            height: 720,
            font_path: None, // Si es None, no carga fuente por defecto
            vsync: true,
        }
    }
}

/// El punto de entrada principal. Usa el patrón Builder para configurar la app.
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

    pub fn with_title(mut self, title: &str) -> Self {
        self.config.title = title.to_string();
        self
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    pub fn with_font(mut self, path: &str) -> Self {
        self.config.font_path = Some(path.to_string());
        self
    }

    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.config.vsync = vsync;
        self
    }

    /// Ejecuta el bucle principal de la aplicación.
    pub fn run(self) {
        crate::runner::run_internal(self.config, self.app_state);
    }
}
