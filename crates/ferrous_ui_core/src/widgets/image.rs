//! # `ImageWidget` — Renderizado de Imágenes y Texturas
//!
//! `ImageWidget` muestra una textura GPU dentro de un rectángulo, con soporte
//! para **modos de ajuste** (`Fit`, `Fill`, `Stretch`, `None`), **recorte UV**,
//! **color tint** y **radio de borde** para miniaturas redondeadas.
//!
//! Funciona en dos modos según el feature `assets` del crate:
//!
//! - **Con `assets`**: acepta un `Arc<ferrous_assets::Texture2d>` directamente.
//! - **Sin `assets`**: acepta un `texture_id: u64` opaco que el backend resolverá.
//!
//! ## Ejemplo de uso (sin feature `assets`)
//!
//! ```rust,ignore
//! use ferrous_ui_core::{ImageWidget, ImageFit};
//!
//! // El texture_id lo gestiona el backend de renderizado (ej. índice en el atlas)
//! let preview = ImageWidget::<MyApp>::from_id(my_texture_id)
//!     .fit(ImageFit::Contain)          // escala sin distorsión
//!     .tint([1.0, 0.8, 0.8, 1.0])     // tono rojizo
//!     .border_radius(8.0);             // esquinas redondeadas
//!
//! let img_id = tree.add_node(Box::new(preview), Some(panel_id));
//! tree.set_node_style(img_id, StyleBuilder::new().width_px(200.0).height_px(150.0).build());
//! ```
//!
//! ## Modo de ajuste (`ImageFit`)
//!
//! | Variante | Descripción |
//! |----------|-------------|
//! | `Contain` | Escala uniformemente hasta que quepa (letterbox/pillarbox). |
//! | `Cover` | Escala hasta llenar, recortando los bordes que sobresalen. |
//! | `Stretch` | Estira a las dimensiones exactas del widget (puede distorsionar). |
//! | `None` | Sin escalado; la imagen se muestra a tamaño original centrada. |
//!
//! ## Coordenadas UV
//!
//! Las UV van de `[0.0, 0.0]` (esquina superior izquierda) a `[1.0, 1.0]`
//! (esquina inferior derecha). Usar UV parciales permite subregiones (atlas):
//!
//! ```rust,ignore
//! // Solo el cuadrante superior izquierdo de la textura
//! ImageWidget::<MyApp>::from_id(atlas_id)
//!     .uv([ 0.0, 0.0 ], [ 0.5, 0.5 ]);
//! ```

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, StyleBuilder, StyleExt,
};

// ─── ImageFit ─────────────────────────────────────────────────────────────────

/// Define cómo la imagen se adapta a las dimensiones del widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFit {
    /// Escala uniformemente para que la imagen quepa entera (sin recorte).
    /// Puede dejar bandas letterbox/pillarbox si la proporción difiere.
    #[default]
    Contain,
    /// Escala uniformemente para llenar todo el widget, recortando si es necesario.
    Cover,
    /// Estira la imagen a las dimensiones exactas del widget.
    Stretch,
    /// Sin escalado: la imagen se muestra a su tamaño original y se centra.
    None,
}

// ─── ImageWidget ──────────────────────────────────────────────────────────────

/// Widget que renderiza una textura GPU con modos de ajuste configurables.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct ImageWidget<App = ()> {
    // ── Fuente de textura ─────────────────────────────────────────────
    /// ID opaco de textura (resuelto por el backend de renderizado).
    /// Usado cuando el feature `assets` no está habilitado.
    pub texture_id: u64,

    /// Textura real gestionada por el sistema de assets.
    #[cfg(feature = "assets")]
    pub texture: Option<std::sync::Arc<ferrous_assets::Texture2d>>,

    // ── Coordenadas UV (subregión de la textura) ──────────────────────
    /// Esquina superior izquierda de la región UV (defecto: `[0.0, 0.0]`).
    pub uv0: [f32; 2],
    /// Esquina inferior derecha de la región UV (defecto: `[1.0, 1.0]`).
    pub uv1: [f32; 2],

    // ... (campos restandes truncados para brevedad) ...
    pub fit: ImageFit,
    pub tint: [f32; 4],
    pub border_radius: f32,
    pub show_placeholder: bool,
    pub intrinsic_width: f32,
    pub intrinsic_height: f32,

    _marker: std::marker::PhantomData<App>,
}

impl<App> ImageWidget<App> {
    /// Crea un `ImageWidget` a partir de un `texture_id` opaco.
    ///
    /// El `texture_id` es el identificador que el backend de renderizado
    /// usa para localizar la textura en su tabla interna (atlas, bind group, etc.).
    pub fn from_id(texture_id: u64) -> Self {
        Self {
            texture_id,
            #[cfg(feature = "assets")]
            texture: None,
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            fit: ImageFit::Contain,
            tint: [1.0, 1.0, 1.0, 1.0],
            border_radius: 0.0,
            show_placeholder: true,
            intrinsic_width: 0.0,
            intrinsic_height: 0.0,
            _marker: std::marker::PhantomData,
        }
    }

    /// Crea un `ImageWidget` a partir de una textura del sistema de assets.
    #[cfg(feature = "assets")]
    pub fn from_texture(texture: std::sync::Arc<ferrous_assets::Texture2d>) -> Self {
        let (w, h) = (texture.texture.width() as f32, texture.texture.height() as f32);
        Self {
            texture_id: 0,
            texture: Some(texture),
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            fit: ImageFit::Contain,
            tint: [1.0, 1.0, 1.0, 1.0],
            border_radius: 0.0,
            show_placeholder: true,
            intrinsic_width: w,
            intrinsic_height: h,
            _marker: std::marker::PhantomData,
        }
    }

    /// Establece el modo de ajuste de la imagen.
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Coordenadas UV para mostrar una subregión de la textura (atlas).
    ///
    /// `uv0` = esquina superior izquierda, `uv1` = esquina inferior derecha.
    pub fn uv(mut self, uv0: [f32; 2], uv1: [f32; 2]) -> Self {
        self.uv0 = uv0;
        self.uv1 = uv1;
        self
    }

    /// Color tint RGBA multiplicativo (defecto: sin tinte = `[1,1,1,1]`).
    pub fn tint(mut self, color: [f32; 4]) -> Self {
        self.tint = color;
        self
    }

    /// Radio de borde en píxeles para esquinas redondeadas.
    pub fn border_radius(mut self, r: f32) -> Self {
        self.border_radius = r;
        self
    }

    /// Tamaño intrínseco de la textura (mejora el cálculo de `calculate_size`).
    pub fn intrinsic_size(mut self, w: f32, h: f32) -> Self {
        self.intrinsic_width = w;
        self.intrinsic_height = h;
        self
    }

    /// Desactiva el placeholder de fondo cuando no hay textura.
    pub fn no_placeholder(mut self) -> Self {
        self.show_placeholder = false;
        self
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Calcula el rectángulo de destino donde pintar la imagen, aplicando
    /// el `fit` seleccionado dentro del contenedor `container`.
    fn dest_rect(&self, container: Rect) -> Rect {
        if self.intrinsic_width <= 0.0 || self.intrinsic_height <= 0.0 {
            // Sin dimensiones intrínsecas: usar Stretch siempre
            return container;
        }

        let aspect = self.intrinsic_width / self.intrinsic_height;
        let cont_aspect = container.width / container.height;

        match self.fit {
            ImageFit::Stretch => container,

            ImageFit::None => {
                // Tamaño original, centrado
                let x = container.x + (container.width - self.intrinsic_width) * 0.5;
                let y = container.y + (container.height - self.intrinsic_height) * 0.5;
                Rect::new(x, y, self.intrinsic_width, self.intrinsic_height)
            }

            ImageFit::Contain => {
                // Escalar para caber sin recorte
                let (w, h) = if aspect > cont_aspect {
                    // Limitado por el ancho
                    let w = container.width;
                    (w, w / aspect)
                } else {
                    // Limitado por el alto
                    let h = container.height;
                    (h * aspect, h)
                };
                let x = container.x + (container.width - w) * 0.5;
                let y = container.y + (container.height - h) * 0.5;
                Rect::new(x, y, w, h)
            }

            ImageFit::Cover => {
                // Escalar para cubrir sin bandas (puede haber recorte en UV)
                let (w, h) = if aspect < cont_aspect {
                    let w = container.width;
                    (w, w / aspect)
                } else {
                    let h = container.height;
                    (h * aspect, h)
                };
                let x = container.x + (container.width - w) * 0.5;
                let y = container.y + (container.height - h) * 0.5;
                Rect::new(x, y, w, h)
            }
        }
    }
}

impl<App> Default for ImageWidget<App> {
    fn default() -> Self {
        Self::from_id(0)
    }
}

impl<App: Send + Sync + 'static> Widget<App> for ImageWidget<App> {
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = ctx.rect;
        let theme = &ctx.theme;

        // ── Placeholder cuando no hay textura ────────────────────────────
        if self.texture_id == 0 && self.show_placeholder {
            // Fondo con patrón de tablero (emulado con dos quads)
            cmds.push(RenderCommand::Quad {
                rect: r,
                color: theme.surface_elevated.to_array(),
                radii: [self.border_radius; 4],
                flags: 0,
            });
            // Cruz diagonal — placeholder visual estándar
            let cx = r.x + r.width * 0.5;
            let cy = r.y + r.height * 0.5;
            let size = r.width.min(r.height) * 0.3;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(cx - size * 0.5, cy - 1.0, size, 2.0),
                color: theme.on_surface_muted.with_alpha(0.3).to_array(),
                radii: [1.0; 4],
                flags: 0,
            });
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(cx - 1.0, cy - size * 0.5, 2.0, size),
                color: theme.on_surface_muted.with_alpha(0.3).to_array(),
                radii: [1.0; 4],
                flags: 0,
            });
            // Borde del placeholder
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x, r.y, r.width, 1.0),
                color: theme.on_surface_muted.with_alpha(0.15).to_array(),
                radii: [0.0; 4],
                flags: 0,
            });
            return;
        }

        if self.texture_id == 0 {
            return; // Sin placeholder y sin textura → nada que pintar
        }

        // ── Borde redondeado: clip antes de la imagen si border_radius > 0 ──
        if self.border_radius > 0.0 {
            cmds.push(RenderCommand::PushClip { rect: r });
        }

        let dest = self.dest_rect(r);

        // ── RenderCommand::Image ─────────────────────────────────────────
        #[cfg(feature = "assets")]
        {
            if let Some(tex) = self.texture.as_ref() {
                cmds.push(RenderCommand::Image {
                    rect: dest,
                    texture: tex.clone(),
                    uv0: self.uv0,
                    uv1: self.uv1,
                    color: self.tint,
                });
            } else if self.texture_id != 0 {
                // Si tenemos un ID pero no una textura Arc, esto es un error de configuración
                // pero intentamos un fallback silencioso si los tipos coincidieran (no es el caso aquí).
            }
        }
        #[cfg(not(feature = "assets"))]
        {
            cmds.push(RenderCommand::Image {
                rect: dest,
                texture_id: self.texture_id,
                uv0: self.uv0,
                uv1: self.uv1,
                color: self.tint,
            });
        }

        if self.border_radius > 0.0 {
            cmds.push(RenderCommand::PopClip);
        }
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        if self.intrinsic_width > 0.0 && self.intrinsic_height > 0.0 {
            Vec2::new(self.intrinsic_width, self.intrinsic_height)
        } else {
            // Sin dimensiones conocidas, reportamos el espacio disponible
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
