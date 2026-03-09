//! # `SvgWidget` — Renderizado de Gráficos Vectoriales Escalables
//!
//! `SvgWidget` muestra contenido SVG dentro de un widget de la UI. Soporta
//! dos modos de renderizado según las capacidades del backend:
//!
//! ## Modos de renderizado
//!
//! ### Modo Textura (recomendado para SVGs complejos)
//! El SVG se **rasteriza a una textura** en la GPU y se muestra como `ImageWidget`.
//! El backend de renderizado registra el SVG y devuelve un `texture_id` que el
//! widget usa para pintarse. Este modo soporta cualquier SVG válido.
//!
//! ```rust,ignore
//! // 1. El backend rasteriza el SVG y te da un texture_id
//! let icon_id = renderer.register_svg(include_str!("assets/icon.svg"), 64, 64);
//!
//! // 2. Crear el widget
//! let svg = SvgWidget::<MyApp>::from_texture(icon_id)
//!     .tint(theme.primary.to_array())
//!     .size(64.0, 64.0);
//! ```
//!
//! ### Modo Primitivas (para iconos vectoriales simples)
//! Para SVGs simples (iconos monoline, formas básicas), se puede usar el modo
//! de primitivas que convierte paths SVG en `RenderCommand::Quad` directamente,
//! sin textura. Ideal para iconos que deben reescalar sin pérdida de calidad
//! y recolorear en tiempo real.
//!
//! ```rust,ignore
//! // Icono vectorial definido con primitivas
//! let close_icon = SvgWidget::<MyApp>::from_primitives(vec![
//!     SvgPrimitive::Line { x1: 4.0, y1: 4.0, x2: 20.0, y2: 20.0, stroke: 2.0 },
//!     SvgPrimitive::Line { x1: 20.0, y1: 4.0, x2: 4.0, y2: 20.0, stroke: 2.0 },
//! ])
//! .color([1.0, 1.0, 1.0, 1.0])
//! .viewbox(0.0, 0.0, 24.0, 24.0);
//! ```
//!
//! ## Características comunes
//!
//! - `ImageFit::Contain` por defecto (sin distorsión).
//! - Tinte de color aplicable en cualquier modo.
//! - Tamaño intrínseco configurable para el sistema de layout.
//! - En modo textura: placeholder estético mientras carga.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, StyleBuilder, StyleExt,
    ImageFit,
};

// ─── SvgPrimitive ─────────────────────────────────────────────────────────────

/// Primitiva vectorial simple para `SvgWidget` en modo primitivas.
///
/// Coordenadas en el espacio del viewbox original (antes del escalado).
#[derive(Debug, Clone)]
pub enum SvgPrimitive {
    /// Rectángulo relleno con radio de esquina opcional.
    Rect {
        x: f32, y: f32, width: f32, height: f32,
        radius: f32,
        fill: bool,
        stroke_width: f32,
    },
    /// Círculo (elipse con `rx == ry`).
    Circle {
        cx: f32, cy: f32, r: f32,
        fill: bool,
        stroke_width: f32,
    },
    /// Línea de `(x1,y1)` a `(x2,y2)` simulada con un Quad rotado.
    /// Nota simplificación: el Quad del backend no soporta rotación arbitraria;
    /// las líneas perfectamente H o V se renderizan exactas, las oblicuas
    /// se aproximan con el Quad más próximo.
    Line {
        x1: f32, y1: f32, x2: f32, y2: f32,
        stroke_width: f32,
    },
    /// Rectángulo horizontal (útil para líneas de separación).
    HLine {
        x: f32, y: f32, length: f32,
        stroke_width: f32,
    },
    /// Rectángulo vertical.
    VLine {
        x: f32, y: f32, length: f32,
        stroke_width: f32,
    },
}

// ─── SvgSource ────────────────────────────────────────────────────────────────

enum SvgSource {
    /// Textura pre-rasterizada por el backend.
    Texture {
        texture_id: u64,
        #[cfg(feature = "assets")]
        texture: Option<std::sync::Arc<ferrous_assets::Texture2d>>,
    },
    /// Primitivas vectoriales dibujadas en `draw()`.
    Primitives { primitives: Vec<SvgPrimitive> },
    /// SVG source en texto; el backend lo rasterizará de forma diferida.
    Source {
        content: String,
        texture_id: u64,
        #[cfg(feature = "assets")]
        texture: Option<std::sync::Arc<ferrous_assets::Texture2d>>,
    },
}

// ─── SvgWidget ────────────────────────────────────────────────────────────────

/// Widget que muestra gráficos vectoriales escalables.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct SvgWidget<App = ()> {
    source: SvgSource,

    // ── Viewbox del SVG original (para escalado en modo primitivas) ───
    /// Coordenada X de origen del viewbox SVG.
    pub viewbox_x: f32,
    /// Coordenada Y de origen del viewbox SVG.
    pub viewbox_y: f32,
    /// Ancho del viewbox SVG (espacio de coordenadas de las primitivas).
    pub viewbox_w: f32,
    /// Alto del viewbox SVG (espacio de coordenadas de las primitivas).
    pub viewbox_h: f32,

    // ── Opciones visuales ─────────────────────────────────────────────
    /// Modo de ajuste (solo en modo textura).
    pub fit: ImageFit,
    /// Color principal para tinte o fill de primitivas.
    pub color: [f32; 4],
    /// Tamaño intrínseco del widget (ancho × alto en px).
    pub intrinsic_width: f32,
    pub intrinsic_height: f32,

    _marker: std::marker::PhantomData<App>,
}

impl<App> SvgWidget<App> {
    // ── Constructores ─────────────────────────────────────────────────

    /// Crea un `SvgWidget` a partir de un `texture_id` de textura pre-rasterizada.
    ///
    /// El backend de renderizado debe haber rasterizado el SVG previamente y
    /// registrado la textura resultante, devolviendo un `texture_id`.
    pub fn from_texture(texture_id: u64) -> Self {
        Self::new_inner(SvgSource::Texture {
            texture_id,
            #[cfg(feature = "assets")]
            texture: None,
        })
    }

    /// Crea un `SvgWidget` a partir de una textura del sistema de assets.
    #[cfg(feature = "assets")]
    pub fn from_asset_texture(texture: std::sync::Arc<ferrous_assets::Texture2d>) -> Self {
        Self::new_inner(SvgSource::Texture {
            texture_id: 0,
            texture: Some(texture),
        })
    }

    /// Crea un `SvgWidget` a partir de contenido SVG en texto.
    ///
    /// El `texture_id` inicial es `0` (placeholder). El backend debe llamar a
    /// `widget.set_texture_id(id)` cuando la rasterización esté lista.
    pub fn from_source(svg_content: impl Into<String>) -> Self {
        Self::new_inner(SvgSource::Source {
            content: svg_content.into(),
            texture_id: 0,
            #[cfg(feature = "assets")]
            texture: None,
        })
    }

    /// Crea un `SvgWidget` en modo primitivas vectoriales.
    ///
    /// Las primitivas se escalan automáticamente al `viewbox` configurado.
    pub fn from_primitives(primitives: Vec<SvgPrimitive>) -> Self {
        Self::new_inner(SvgSource::Primitives { primitives })
    }

    fn new_inner(source: SvgSource) -> Self {
        Self {
            source,
            viewbox_x: 0.0,
            viewbox_y: 0.0,
            viewbox_w: 24.0,
            viewbox_h: 24.0,
            fit: ImageFit::Contain,
            color: [1.0, 1.0, 1.0, 1.0],
            intrinsic_width: 0.0,
            intrinsic_height: 0.0,
            _marker: std::marker::PhantomData,
        }
    }

    // ── Configuración ─────────────────────────────────────────────────

    /// Define el viewbox del SVG (sistema de coordenadas de las primitivas).
    ///
    /// Por defecto: `(0, 0, 24, 24)` — compatible con la mayoría de icon sets.
    pub fn viewbox(mut self, x: f32, y: f32, w: f32, h: f32) -> Self {
        self.viewbox_x = x;
        self.viewbox_y = y;
        self.viewbox_w = w;
        self.viewbox_h = h;
        self
    }

    /// Color de fill/tinte de las primitivas o de la textura.
    pub fn color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    /// Modo de ajuste (solo aplica en modo textura).
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Tamaño intrínseco en píxeles para el sistema de layout.
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.intrinsic_width = w;
        self.intrinsic_height = h;
        self
    }

    /// Registra el `texture_id` cuando la rasterización asíncrona esté lista.
    /// Solo útil en modo `from_source`.
    pub fn set_texture_id(&mut self, id: u64) {
        match &mut self.source {
            SvgSource::Source { texture_id, .. } => *texture_id = id,
            SvgSource::Texture { texture_id, .. }    => *texture_id = id,
            _ => {}
        }
    }

    /// Devuelve el `texture_id` actual (0 si aún no rasterizado).
    pub fn texture_id(&self) -> u64 {
        match &self.source {
            SvgSource::Texture { texture_id, .. } => *texture_id,
            SvgSource::Source { texture_id, .. } => *texture_id,
            SvgSource::Primitives { .. } => 0,
        }
    }

    /// Devuelve el contenido SVG en texto (solo modo `from_source`).
    pub fn svg_source(&self) -> Option<&str> {
        match &self.source {
            SvgSource::Source { content, .. } => Some(content.as_str()),
            _ => None,
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Calcula la transformación lineal [escala_x, escala_y, offset_x, offset_y]
    /// para mapear coordenadas del viewbox al rect del widget.
    fn viewbox_transform(&self, dest: Rect) -> (f32, f32, f32, f32) {
        let scale_x = dest.width / self.viewbox_w;
        let scale_y = dest.height / self.viewbox_h;

        // Contain: misma escala en ambas dimensiones
        let scale = scale_x.min(scale_y);
        let scaled_w = self.viewbox_w * scale;
        let scaled_h = self.viewbox_h * scale;
        let offset_x = dest.x + (dest.width - scaled_w) * 0.5;
        let offset_y = dest.y + (dest.height - scaled_h) * 0.5;

        (scale, scale, offset_x, offset_y)
    }

    /// Transforma coordenadas de viewbox a coordenadas de pantalla.
    fn transform_x(&self, vx: f32, scale: f32, offset_x: f32) -> f32 {
        (vx - self.viewbox_x) * scale + offset_x
    }

    fn transform_y(&self, vy: f32, scale: f32, offset_y: f32) -> f32 {
        (vy - self.viewbox_y) * scale + offset_y
    }

    /// Renderiza las primitivas del modo vectorial.
    fn draw_primitives(
        &self,
        primitives: &[SvgPrimitive],
        dest: Rect,
        cmds: &mut Vec<RenderCommand>,
    ) {
        let (scale_x, scale_y, offset_x, offset_y) = self.viewbox_transform(dest);
        let color = self.color;

        for prim in primitives {
            match prim {
                SvgPrimitive::Rect { x, y, width, height, radius, fill: _, stroke_width } => {
                    let px = self.transform_x(*x, scale_x, offset_x);
                    let py = self.transform_y(*y, scale_y, offset_y);
                    let pw = width * scale_x;
                    let ph = height * scale_y;
                    let pr = radius * scale_x.min(scale_y);
                    cmds.push(RenderCommand::Quad {
                        rect: Rect::new(px, py, pw, ph),
                        color,
                        radii: [pr; 4],
                        flags: 0,
                    });
                    let _ = stroke_width;
                }

                SvgPrimitive::Circle { cx, cy, r, fill: _, stroke_width } => {
                    let px = self.transform_x(cx - r, scale_x, offset_x);
                    let py = self.transform_y(cy - r, scale_y, offset_y);
                    let pr = r * scale_x.min(scale_y);
                    let pw = r * 2.0 * scale_x;
                    let ph = r * 2.0 * scale_y;
                    cmds.push(RenderCommand::Quad {
                        rect: Rect::new(px, py, pw, ph),
                        color,
                        radii: [pr; 4],
                        flags: 0,
                    });
                    let _ = stroke_width;
                }

                SvgPrimitive::HLine { x, y, length, stroke_width } => {
                    let px = self.transform_x(*x, scale_x, offset_x);
                    let py = self.transform_y(*y, scale_y, offset_y);
                    let pw = length * scale_x;
                    let ph = (stroke_width * scale_y).max(1.0);
                    cmds.push(RenderCommand::Quad {
                        rect: Rect::new(px, py - ph * 0.5, pw, ph),
                        color,
                        radii: [ph * 0.5; 4],
                        flags: 0,
                    });
                }

                SvgPrimitive::VLine { x, y, length, stroke_width } => {
                    let px = self.transform_x(*x, scale_x, offset_x);
                    let py = self.transform_y(*y, scale_y, offset_y);
                    let pw = (stroke_width * scale_x).max(1.0);
                    let ph = length * scale_y;
                    cmds.push(RenderCommand::Quad {
                        rect: Rect::new(px - pw * 0.5, py, pw, ph),
                        color,
                        radii: [pw * 0.5; 4],
                        flags: 0,
                    });
                }

                SvgPrimitive::Line { x1, y1, x2, y2, stroke_width } => {
                    // Líneas oblicuas: aproximación con Quad horizontal/vertical
                    // Para renderizado exacto de líneas diagonales se requiere
                    // soporte de rotación en el backend.
                    let px1 = self.transform_x(*x1, scale_x, offset_x);
                    let py1 = self.transform_y(*y1, scale_y, offset_y);
                    let px2 = self.transform_x(*x2, scale_x, offset_x);
                    let py2 = self.transform_y(*y2, scale_y, offset_y);

                    let dx = px2 - px1;
                    let dy = py2 - py1;
                    let len = (dx * dx + dy * dy).sqrt();
                    let thickness = (stroke_width * scale_x.min(scale_y)).max(1.0);

                    if dx.abs() >= dy.abs() {
                        // Predominantemente horizontal
                        let mn_x = px1.min(px2);
                        let mid_y = (py1 + py2) * 0.5;
                        cmds.push(RenderCommand::Quad {
                            rect: Rect::new(mn_x, mid_y - thickness * 0.5, len, thickness),
                            color,
                            radii: [thickness * 0.5; 4],
                            flags: 0,
                        });
                    } else {
                        // Predominantemente vertical
                        let mn_y = py1.min(py2);
                        let mid_x = (px1 + px2) * 0.5;
                        cmds.push(RenderCommand::Quad {
                            rect: Rect::new(mid_x - thickness * 0.5, mn_y, thickness, len),
                            color,
                            radii: [thickness * 0.5; 4],
                            flags: 0,
                        });
                    }
                }
            }
        }
    }

    /// Calcula el rect de destino para el modo textura (equivalente al de ImageWidget).
    fn texture_dest_rect(&self, r: Rect) -> Rect {
        let iw = if self.intrinsic_width > 0.0 { self.intrinsic_width } else { self.viewbox_w };
        let ih = if self.intrinsic_height > 0.0 { self.intrinsic_height } else { self.viewbox_h };

        if iw <= 0.0 || ih <= 0.0 {
            return r;
        }

        let aspect = iw / ih;
        let cont_aspect = r.width / r.height;

        let (w, h) = match self.fit {
            ImageFit::Stretch => return r,
            ImageFit::None => (iw, ih),
            ImageFit::Contain => {
                if aspect > cont_aspect {
                    let w = r.width;
                    (w, w / aspect)
                } else {
                    let h = r.height;
                    (h * aspect, h)
                }
            }
            ImageFit::Cover => {
                if aspect < cont_aspect {
                    let w = r.width;
                    (w, w / aspect)
                } else {
                    let h = r.height;
                    (h * aspect, h)
                }
            }
        };

        let x = r.x + (r.width - w) * 0.5;
        let y = r.y + (r.height - h) * 0.5;
        Rect::new(x, y, w, h)
    }
}

impl<App> Default for SvgWidget<App> {
    fn default() -> Self {
        Self::from_primitives(Vec::new())
    }
}

impl<App: Send + Sync + 'static> Widget<App> for SvgWidget<App> {
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = ctx.rect;
        let theme = &ctx.theme;

        match &self.source {
            // ── Modo textura ─────────────────────────────────────────────────
            SvgSource::Texture { texture_id, .. } | SvgSource::Source { texture_id, .. }
                if *texture_id != 0 || {
                    #[cfg(feature = "assets")]
                    {
                        match &self.source {
                            SvgSource::Texture { texture, .. } | SvgSource::Source { texture, .. } => texture.is_some(),
                            _ => false,
                        }
                    }
                    #[cfg(not(feature = "assets"))]
                    false
                }
            =>
            {
                let dest = self.texture_dest_rect(r);
                
                #[cfg(feature = "assets")]
                {
                    let tex = match &self.source {
                        SvgSource::Texture { texture, .. } | SvgSource::Source { texture, .. } => texture.clone(),
                        _ => None,
                    };

                    if let Some(t) = tex {
                        cmds.push(RenderCommand::Image {
                            rect: dest,
                            texture: t,
                            uv0: [0.0, 0.0],
                            uv1: [1.0, 1.0],
                            color: self.color,
                        });
                    } else {
                        // Fallback a ID si el backend lo soporta (no en asset mode)
                    }
                }
                #[cfg(not(feature = "assets"))]
                {
                    cmds.push(RenderCommand::Image {
                        rect: dest,
                        texture_id: *texture_id,
                        uv0: [0.0, 0.0],
                        uv1: [1.0, 1.0],
                        color: self.color,
                    });
                }
            }

            // ── Modo textura sin ID (placeholder de carga) ───────────────────
            SvgSource::Texture { texture_id: 0, .. } | SvgSource::Source { texture_id: 0, .. } => {
                // Placeholder: spinning indicator / rect muted
                cmds.push(RenderCommand::Quad {
                    rect: r,
                    color: theme.surface_elevated.to_array(),
                    radii: [4.0; 4],
                    flags: 0,
                });
                // Icono SVG (placeholder)
                let cx = r.x + r.width * 0.5;
                let cy = r.y + r.height * 0.5;
                let sz = r.width.min(r.height) * 0.25;
                cmds.push(RenderCommand::Quad {
                    rect: Rect::new(cx - sz, cy - 1.0, sz * 2.0, 2.0),
                    color: theme.primary.with_alpha(0.4).to_array(),
                    radii: [1.0; 4],
                    flags: 0,
                });
                cmds.push(RenderCommand::Quad {
                    rect: Rect::new(cx - 1.0, cy - sz, 2.0, sz * 2.0),
                    color: theme.primary.with_alpha(0.4).to_array(),
                    radii: [1.0; 4],
                    flags: 0,
                });
            }

            // ── Modo primitivas ──────────────────────────────────────────────
            SvgSource::Primitives { primitives } => {
                let dest_rect = {
                    let aspect = self.viewbox_w / self.viewbox_h.max(0.001);
                    let cont_aspect = r.width / r.height.max(0.001);
                    let (w, h) = if aspect > cont_aspect {
                        let w = r.width;
                        (w, w / aspect)
                    } else {
                        let h = r.height;
                        (h * aspect, h)
                    };
                    let x = r.x + (r.width - w) * 0.5;
                    let y = r.y + (r.height - h) * 0.5;
                    Rect::new(x, y, w, h)
                };
                self.draw_primitives(primitives, dest_rect, cmds);
            }

            _ => {}
        }
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        let w = if self.intrinsic_width > 0.0 { self.intrinsic_width } else { self.viewbox_w };
        let h = if self.intrinsic_height > 0.0 { self.intrinsic_height } else { self.viewbox_h };
        if w > 0.0 && h > 0.0 {
            Vec2::new(w, h)
        } else {
            Vec2::new(ctx.available_space.x, ctx.available_space.y)
        }
    }

    fn on_event(
        &mut self,
        _ctx: &mut EventContext<App>,
        _event: &UiEvent,
    ) -> EventResponse {
        EventResponse::Ignored
    }
}

// ─── Iconos predefinidos ──────────────────────────────────────────────────────

/// Colección de iconos SVG predefinidos en modo primitivas.
///
/// Todos usan viewbox `(0, 0, 24, 24)` — estándar Material Design / Feather Icons.
pub struct Icons;

impl Icons {
    /// Icono `×` para cerrar / eliminar.
    pub fn close<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::Line { x1: 4.0, y1: 4.0, x2: 20.0, y2: 20.0, stroke_width: 2.0 },
            SvgPrimitive::Line { x1: 20.0, y1: 4.0, x2: 4.0, y2: 20.0, stroke_width: 2.0 },
        ])
    }

    /// Icono `+` para añadir.
    pub fn plus<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::HLine { x: 4.0, y: 12.0, length: 16.0, stroke_width: 2.0 },
            SvgPrimitive::VLine { x: 12.0, y: 4.0, length: 16.0, stroke_width: 2.0 },
        ])
    }

    /// Icono de búsqueda (lupa circular).
    pub fn search<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::Circle { cx: 10.0, cy: 10.0, r: 6.5, fill: false, stroke_width: 2.0 },
            SvgPrimitive::Line { x1: 15.0, y1: 15.0, x2: 20.0, y2: 20.0, stroke_width: 2.0 },
        ])
    }

    /// Icono de ajustes (engranaje aproximado con un círculo).
    pub fn settings<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::Circle { cx: 12.0, cy: 12.0, r: 9.0, fill: false, stroke_width: 2.0 },
            SvgPrimitive::Circle { cx: 12.0, cy: 12.0, r: 3.5, fill: true, stroke_width: 0.0 },
        ])
    }

    /// Icono de menú (tres líneas horizontales — hamburguesa).
    pub fn menu<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::HLine { x: 3.0, y: 6.0,  length: 18.0, stroke_width: 2.0 },
            SvgPrimitive::HLine { x: 3.0, y: 12.0, length: 18.0, stroke_width: 2.0 },
            SvgPrimitive::HLine { x: 3.0, y: 18.0, length: 18.0, stroke_width: 2.0 },
        ])
    }

    /// Icono de flecha hacia la derecha.
    pub fn arrow_right<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::HLine { x: 4.0,  y: 12.0, length: 16.0, stroke_width: 2.0 },
            SvgPrimitive::Line  { x1: 14.0, y1: 6.0, x2: 20.0, y2: 12.0, stroke_width: 2.0 },
            SvgPrimitive::Line  { x1: 14.0, y1: 18.0, x2: 20.0, y2: 12.0, stroke_width: 2.0 },
        ])
    }

    /// Icono de check (✓).
    pub fn check<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::Line { x1: 4.0, y1: 12.0, x2: 9.0, y2: 17.0, stroke_width: 2.5 },
            SvgPrimitive::Line { x1: 9.0, y1: 17.0, x2: 20.0, y2: 6.0, stroke_width: 2.5 },
        ])
    }

    /// Icono de advertencia (triángulo aproximado con líneas).
    pub fn warning<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::VLine { x: 12.0, y: 7.0,  length: 7.0, stroke_width: 2.0 },
            SvgPrimitive::Circle { cx: 12.0, cy: 17.5, r: 1.5, fill: true, stroke_width: 0.0 },
        ])
    }

    /// Icono de información (i).
    pub fn info<App>() -> SvgWidget<App> {
        SvgWidget::from_primitives(vec![
            SvgPrimitive::Circle { cx: 12.0, cy: 7.0, r: 1.5, fill: true, stroke_width: 0.0 },
            SvgPrimitive::VLine  { x: 12.0, y: 10.0, length: 7.0, stroke_width: 2.0 },
        ])
    }
}
