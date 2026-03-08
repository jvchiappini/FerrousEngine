//! `Theme` — Sistema de temas globales para la UI de FerrousEngine.
//!
//! Define una paleta semántica de colores y valores visuales compartidos
//! por todos los widgets. Elimina los colores hardcodeados `[f32; 4]` y
//! permite cambiar el aspecto visual de toda la aplicación desde un solo lugar.
//!
//! # Ejemplo
//!
//! ```rust
//! use ferrous_ui_core::{Theme, Color};
//!
//! let theme = Theme::dark()
//!     .with_primary(Color::hex("#6C63FF"))
//!     .with_surface(Color::hex("#1E1E2E"))
//!     .with_on_surface(Color::hex("#CDD6F4"))
//!     .with_border_radius(8.0)
//!     .with_base_font_size(14.0);
//! ```

/// Color RGBA normalizado (0.0–1.0 por canal).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Crea un color a partir de valores RGBA normalizados (0.0–1.0).
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Crea un color a partir de valores RGBA en el rango 0–255.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Crea un color a partir de una cadena hexadecimal.
    ///
    /// Formatos soportados: `"#RRGGBB"`, `"#RRGGBBAA"`, `"RRGGBB"`, `"RRGGBBAA"`.
    ///
    /// Devuelve negro opaco si la cadena es inválida.
    pub fn hex(s: &str) -> Self {
        let s = s.trim_start_matches('#');
        let parse = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0);

        match s.len() {
            6 => Self::from_rgba8(parse(0), parse(2), parse(4), 255),
            8 => Self::from_rgba8(parse(0), parse(2), parse(4), parse(6)),
            _ => Self::BLACK,
        }
    }

    /// Convierte el color al array `[f32; 4]` usado en `RenderCommand`.
    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Devuelve el color con la opacidad modificada.
    pub fn with_alpha(mut self, a: f32) -> Self {
        self.a = a.clamp(0.0, 1.0);
        self
    }

    /// Interpola linealmente entre dos colores.
    pub fn lerp(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    // ─── Constantes ───────────────────────────────────────────────────────────

    pub const BLACK:       Color = Color::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE:       Color = Color::new(1.0, 1.0, 1.0, 1.0);
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);

    /// Color de acento por defecto del tema oscuro (violeta Ferrous).
    pub const FERROUS_ACCENT: Color = Color::new(0.424, 0.388, 1.0, 1.0); // #6C63FF
}

impl From<[f32; 4]> for Color {
    fn from(a: [f32; 4]) -> Self {
        Self::new(a[0], a[1], a[2], a[3])
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        c.to_array()
    }
}

// ────────────────────────────────────────────────────────────────────────────────
// Theme
// ────────────────────────────────────────────────────────────────────────────────

/// Paleta de colores y valores visuales compartidos por toda la aplicación.
///
/// Un `Theme` define los roles semánticos de color (primario, superficie, texto)
/// y otros valores globales como el radio de borde y el tamaño base de fuente.
/// Los widgets acceden al tema a través de su `DrawContext` en lugar de usar
/// colores `[f32; 4]` literales, lo que permite cambiar el aspecto visual
/// completo de la app desde un único punto.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // ─── Colores principales ──────────────────────────────────────────────────
    /// Color de acento principal (botones, indicadores, elementos activos).
    pub primary: Color,
    /// Variante más oscura del color primario (hover, bordes).
    pub primary_variant: Color,
    /// Color de texto/iconos sobre superficies primarias.
    pub on_primary: Color,

    // ─── Superficies ──────────────────────────────────────────────────────────
    /// Fondo de la aplicación.
    pub background: Color,
    /// Fondo de paneles y tarjetas (ligeramente más claro que background).
    pub surface: Color,
    /// Fondo elevado (popups, tooltips, dropdowns).
    pub surface_elevated: Color,
    /// Color de texto/iconos sobre superficies.
    pub on_surface: Color,
    /// Color de texto secundario / muted (etiquetas, hints).
    pub on_surface_muted: Color,

    // ─── Feedback ─────────────────────────────────────────────────────────────
    /// Color para estados de error o destructivo.
    pub error: Color,
    /// Color para estados de éxito o confirmación.
    pub success: Color,
    /// Color para advertencias.
    pub warning: Color,

    // ─── Valores de diseño ────────────────────────────────────────────────────
    /// Radio de borde por defecto para widgets con esquinas redondeadas.
    pub border_radius: f32,
    /// Tamaño de fuente base en píxeles (equivale a `font-size: 1rem`).
    pub font_size_base: f32,
    /// Tamaño de fuente para textos pequeños (captions, hints).
    pub font_size_small: f32,
    /// Tamaño de fuente para encabezados.
    pub font_size_heading: f32,
}

impl Theme {
    /// Construye el tema oscuro por defecto de FerrousEngine.
    ///
    /// Inspirado en las paletas de Catppuccin Mocha y Material Design 3.
    pub fn dark() -> Self {
        Self {
            primary:          Color::hex("#6C63FF"),
            primary_variant:  Color::hex("#4A43CC"),
            on_primary:       Color::WHITE,

            background:       Color::hex("#11111B"),
            surface:          Color::hex("#1E1E2E"),
            surface_elevated: Color::hex("#313244"),
            on_surface:       Color::hex("#CDD6F4"),
            on_surface_muted: Color::hex("#6C7086"),

            error:   Color::hex("#F38BA8"),
            success: Color::hex("#A6E3A1"),
            warning: Color::hex("#F9E2AF"),

            border_radius:   6.0,
            font_size_base:  14.0,
            font_size_small: 11.0,
            font_size_heading: 20.0,
        }
    }

    /// Construye el tema claro por defecto de FerrousEngine.
    pub fn light() -> Self {
        Self {
            primary:          Color::hex("#5048E5"),
            primary_variant:  Color::hex("#3730A3"),
            on_primary:       Color::WHITE,

            background:       Color::hex("#F8F9FA"),
            surface:          Color::hex("#FFFFFF"),
            surface_elevated: Color::hex("#F1F1F1"),
            on_surface:       Color::hex("#1C1C2E"),
            on_surface_muted: Color::hex("#6B7280"),

            error:   Color::hex("#DC2626"),
            success: Color::hex("#16A34A"),
            warning: Color::hex("#D97706"),

            border_radius:   6.0,
            font_size_base:  14.0,
            font_size_small: 11.0,
            font_size_heading: 20.0,
        }
    }

    // ─── Builder fluent ───────────────────────────────────────────────────────

    /// Reemplaza el color primario.
    pub fn with_primary(mut self, c: Color) -> Self { self.primary = c; self }
    /// Reemplaza la variante del color primario.
    pub fn with_primary_variant(mut self, c: Color) -> Self { self.primary_variant = c; self }
    /// Reemplaza el color de superficie.
    pub fn with_surface(mut self, c: Color) -> Self { self.surface = c; self }
    /// Reemplaza el color de fondo de la aplicación.
    pub fn with_background(mut self, c: Color) -> Self { self.background = c; self }
    /// Reemplaza el color de texto sobre superficies.
    pub fn with_on_surface(mut self, c: Color) -> Self { self.on_surface = c; self }
    /// Reemplaza el color de texto muted.
    pub fn with_on_surface_muted(mut self, c: Color) -> Self { self.on_surface_muted = c; self }
    /// Reemplaza el color de error.
    pub fn with_error(mut self, c: Color) -> Self { self.error = c; self }
    /// Establece el radio de borde global (en píxeles).
    pub fn with_border_radius(mut self, r: f32) -> Self { self.border_radius = r; self }
    /// Establece el tamaño de fuente base (en píxeles).
    pub fn with_base_font_size(mut self, s: f32) -> Self { self.font_size_base = s; self }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
