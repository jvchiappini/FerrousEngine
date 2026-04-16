//! Background system for UI widgets.
//!
//! This module contains types and implementations for background styling,
//! including gradients (linear, radial, conic) and procedural backgrounds.

use serde::{Deserialize, Serialize};

/// Un stop de color en un degradado (posición 0.0–1.0 y color RGBA lineal).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Posición normalizada del stop (0.0 = inicio, 1.0 = fin).
    pub position: f32,
    /// Color RGBA lineal del stop.
    pub color: [f32; 4],
}

impl GradientStop {
    pub fn new(position: f32, color: [f32; 4]) -> Self {
        Self { position, color }
    }

    /// Crea un stop a partir de un hex `"#RRGGBBAA"` o `"#RRGGBB"`.
    pub fn from_hex(position: f32, hex: &str) -> Self {
        let s = hex.trim().trim_start_matches('#');
        let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
        let color = if s.len() >= 6 {
            match (p(0), p(2), p(4)) {
                (Some(r), Some(g), Some(b)) => {
                    let a = if s.len() >= 8 {
                        p(6).unwrap_or(255)
                    } else {
                        255
                    };
                    let to_lin = |v: u8| (v as f32 / 255.0).powf(2.2);
                    [to_lin(r), to_lin(g), to_lin(b), a as f32 / 255.0]
                }
                _ => [1.0, 1.0, 1.0, 1.0],
            }
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        Self { position, color }
    }
}

/// Dirección de un degradado lineal expresada en grados (0° = arriba→abajo, 90° = izq→der).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientAngle(pub f32);

impl GradientAngle {
    pub fn top_to_bottom() -> Self {
        Self(0.0)
    }
    pub fn left_to_right() -> Self {
        Self(90.0)
    }
    pub fn diagonal() -> Self {
        Self(45.0)
    }
    /// Calcula el vector de dirección normalizado para el ángulo.
    pub fn direction(self) -> [f32; 2] {
        let rad = (self.0 - 90.0).to_radians();
        [rad.cos(), rad.sin()]
    }
}

/// Fondo configurable de un widget — desde sólido hasta degradados y procedurales.
#[derive(Clone)]
#[derive(Default)]
pub enum Background {
    /// Sin fondo extra; el widget usa el color base del tema.
    #[default]
    None,

    /// Color sólido, sobreescribe el color del tema.
    Solid([f32; 4]),

    /// Degradado lineal con N stops.
    ///
    /// El ángulo sigue la convención CSS: 0° = top → bottom, 90° = left → right.
    LinearGradient {
        stops: Vec<GradientStop>,
        angle: GradientAngle,
    },

    /// Degradado radial con N stops, desde el centro hacia el borde.
    ///
    /// `center` es la posición relativa del centro (0.0–1.0 en cada eje).
    /// `radius` es el radio como fracción del lado menor del rect (0.5 = toca los bordes).
    RadialGradient {
        stops: Vec<GradientStop>,
        center: [f32; 2],
        radius: f32,
    },

    /// Degradado cónico (barrido angular) con N stops.
    ///
    /// `center` es la posición relativa (0.5, 0.5 = centro).
    /// `start_angle` en grados (0° = derecha, sentido horario).
    ConicGradient {
        stops: Vec<GradientStop>,
        center: [f32; 2],
        start_angle: f32,
    },

    /// Función procedural: recibe la posición UV normalizada `(u, v)` ∈ [0,1]² y
    /// devuelve el color RGBA lineal del píxel.
    ///
    /// Permite aplicar noise, patrones, animaciones, etc.
    /// Se evalúa **en CPU** generando una textura temporal de la resolución del widget.
    ///
    /// ```rust,ignore
    /// Background::Procedural(Arc::new(|u, v| {
    ///     let n = perlin(u * 4.0, v * 4.0);
    ///     [n, n, n, 1.0]
    /// }))
    /// ```
    Procedural(std::sync::Arc<dyn Fn(f32, f32) -> [f32; 4] + Send + Sync>),

    /// Textura de imagen.  El path/id permite identificarla para cacheo.
    ///
    /// La función `sampler` recibe `(u, v)` ∈ [0,1]² y devuelve RGBA lineal;
    /// puede leer desde un buffer precargado o simplemente ser un sólido fallback.
    Texture {
        /// ID de textura opaco para cacheo y comparación.
        texture_id: u64,
        /// Sampler de la textura: `fn(u: f32, v: f32) -> [f32; 4]`.
        sampler: std::sync::Arc<dyn Fn(f32, f32) -> [f32; 4] + Send + Sync>,
        /// Modo de UV (repeat, clamp, etc.) — codificado como flags.
        uv_mode: UvMode,
    },
}

/// Modo de muestreo UV para texturas de fondo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum UvMode {
    /// La textura se estira para cubrir el rect completo.
    #[default]
    Stretch,
    /// La textura se repite en tile.
    Repeat,
    /// La textura se recorta si es más pequeña que el rect.
    Clamp,
}


impl Background {
    /// Crea un degradado lineal de 2 stops (conveniencia).
    pub fn linear(from: [f32; 4], to: [f32; 4], angle_deg: f32) -> Self {
        Self::LinearGradient {
            stops: vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            angle: GradientAngle(angle_deg),
        }
    }

    /// Crea un degradado radial de 2 stops (conveniencia).
    pub fn radial(inner: [f32; 4], outer: [f32; 4]) -> Self {
        Self::RadialGradient {
            stops: vec![GradientStop::new(0.0, inner), GradientStop::new(1.0, outer)],
            center: [0.5, 0.5],
            radius: 0.5,
        }
    }

    /// Crea un fondo procedural a partir de cualquier función `Fn(u, v) -> [f32; 4]`.
    pub fn procedural(f: impl Fn(f32, f32) -> [f32; 4] + Send + Sync + 'static) -> Self {
        Self::Procedural(std::sync::Arc::new(f))
    }

    /// Interpola linealmente entre dos colores RGBA.
    pub fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]
    }

    /// Evalúa el color del fondo en un punto UV `(u, v)` ∈ [0,1]².
    ///
    /// Util para CPU rasterization o previews en el editor.
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        match self {
            Background::None => [0.0; 4],
            Background::Solid(c) => *c,
            Background::LinearGradient { stops, angle } => {
                if stops.is_empty() {
                    return [0.0; 4];
                }
                let [dx, dy] = angle.direction();
                // Project UV (in [0,1]²) onto the gradient axis.
                // direction() gives a unit vector; we offset u/v from 0.5 so
                // the centre of the rect is the midpoint of the gradient, then
                // scale+bias back to [0,1].
                let t = ((u - 0.5) * dx + (v - 0.5) * dy) * 0.5 + 0.5;
                Self::sample_stops(stops, t.clamp(0.0, 1.0))
            }
            Background::RadialGradient {
                stops,
                center,
                radius,
            } => {
                if stops.is_empty() {
                    return [0.0; 4];
                }
                let du = u - center[0];
                let dv = v - center[1];
                let t = (du * du + dv * dv).sqrt() / radius.max(0.001);
                Self::sample_stops(stops, t.clamp(0.0, 1.0))
            }
            Background::ConicGradient {
                stops,
                center,
                start_angle,
            } => {
                if stops.is_empty() {
                    return [0.0; 4];
                }
                let du = u - center[0];
                let dv = v - center[1];
                let angle = (dv.atan2(du).to_degrees() - start_angle + 360.0) % 360.0;
                let t = angle / 360.0;
                Self::sample_stops(stops, t)
            }
            Background::Procedural(f) => f(u, v),
            Background::Texture { sampler, .. } => sampler(u, v),
        }
    }

    /// Interpola entre los stops para un `t` dado.
    fn sample_stops(stops: &[GradientStop], t: f32) -> [f32; 4] {
        if stops.len() == 1 {
            return stops[0].color;
        }
        let mut a = &stops[0];
        let mut b = &stops[stops.len() - 1];
        for w in stops.windows(2) {
            if t >= w[0].position && t <= w[1].position {
                a = &w[0];
                b = &w[1];
                break;
            }
        }
        let span = b.position - a.position;
        let local_t = if span < 1e-6 {
            0.0
        } else {
            (t - a.position) / span
        };
        Self::lerp_color(a.color, b.color, local_t.clamp(0.0, 1.0))
    }
}


impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Background::None => write!(f, "Background::None"),
            Background::Solid(c) => write!(f, "Background::Solid({:?})", c),
            Background::LinearGradient { stops, angle } => {
                write!(
                    f,
                    "Background::LinearGradient {{ stops: {:?}, angle: {:?} }}",
                    stops, angle
                )
            }
            Background::RadialGradient {
                stops,
                center,
                radius,
            } => {
                write!(
                    f,
                    "Background::RadialGradient {{ stops: {:?}, center: {:?}, radius: {} }}",
                    stops, center, radius
                )
            }
            Background::ConicGradient {
                stops,
                center,
                start_angle,
            } => {
                write!(
                    f,
                    "Background::ConicGradient {{ stops: {:?}, center: {:?}, start_angle: {} }}",
                    stops, center, start_angle
                )
            }
            Background::Procedural(_) => write!(f, "Background::Procedural(<fn>)"),
            Background::Texture {
                texture_id,
                uv_mode,
                ..
            } => {
                write!(
                    f,
                    "Background::Texture {{ texture_id: {}, uv_mode: {:?} }}",
                    texture_id, uv_mode
                )
            }
        }
    }
}