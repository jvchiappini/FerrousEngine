use crate::{
    Background, BuildContext, DrawContext, EventContext, EventResponse, FerrousWidget,
    LayoutContext, RenderCommand, TextAlign, UiEvent, Vec2, Widget,
};

// ─── Button ──────────────────────────────────────────────────────────────────

/// Botón interactivo con hit-testing automático y callbacks enriquecidos.
///
/// El sistema de eventos enruta los clicks automáticamente — el usuario NO necesita
/// escribir lógica de detección de mouse. Solo registra el callback:
///
/// ```rust,ignore
/// ui.button("Guardar")
///     .size(120.0, 36.0)
///     .on_click(|ctx| ctx.app.save())
///     .spawn(&mut ui);
/// ```
#[derive(FerrousWidget)]
pub struct Button<App> {
    #[prop(label = "Texto", category = "Contenido")]
    pub label: String,

    /// Radios de las 4 esquinas: [top-left, top-right, bottom-left, bottom-right].
    /// `None` en cada componente hereda `theme.border_radius`.
    pub radii: [Option<f32>; 4],

    /// Alineación del texto del label dentro del botón. Defecto: centrado.
    pub text_align: TextAlign,

    /// Estado de hover (cursor encima).
    pub is_hovered: bool,

    /// Estado de presionado (mouse down dentro del botón, aún no soltado).
    /// El click se dispara al soltar (`MouseUp`) si `is_pressed == true`.
    pub is_pressed: bool,

    /// Si `true`, el botón no responde a eventos y se renderiza atenuado.
    pub disabled: bool,

    /// Si `true`, dibuja sombra y borde hover del botón.
    pub chrome: bool,

    /// Fondo personalizado. `Background::None` usa el color primario del tema.
    pub background: Background,

    /// Callback invocado al hacer click (MouseUp dentro del área del botón).
    on_click_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,

    /// Callback invocado al hacer click secundario (MouseUp con botón derecho).
    on_right_click_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,

    /// Callback invocado cuando el cursor entra en el botón.
    on_hover_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,

    /// Callback invocado cuando el cursor sale del botón.
    on_hover_end_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,
}

impl<App> Button<App> {
    /// Crea un botón con el texto `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            radii: [None; 4],
            text_align: TextAlign::CENTER,
            is_hovered: false,
            is_pressed: false,
            disabled: false,
            chrome: true,
            background: Background::None,
            on_click_cb: None,
            on_right_click_cb: None,
            on_hover_cb: None,
            on_hover_end_cb: None,
        }
    }

    pub fn with_border_radius(mut self, r: f32) -> Self {
        self.radii = [Some(r); 4];
        self
    }

    pub fn with_radii(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.radii = [Some(tl), Some(tr), Some(br), Some(bl)];
        self
    }

    pub fn with_text_align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    pub fn with_h_align(mut self, h: crate::HAlign) -> Self {
        self.text_align.h = h;
        self
    }

    pub fn with_v_align(mut self, v: crate::VAlign) -> Self {
        self.text_align.v = v;
        self
    }

    pub fn with_background(mut self, bg: Background) -> Self {
        self.background = bg;
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_chrome(mut self, chrome: bool) -> Self {
        self.chrome = chrome;
        self
    }

    /// Registra el callback principal de click.
    pub fn on_click(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_click_cb = Some(Box::new(f));
        self
    }

    /// Registra un callback de click derecho.
    pub fn on_right_click(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_right_click_cb = Some(Box::new(f));
        self
    }

    /// Registra un callback cuando el cursor entra al botón.
    pub fn on_hover(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_hover_cb = Some(Box::new(f));
        self
    }

    /// Registra un callback cuando el cursor sale del botón.
    pub fn on_hover_end(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_hover_end_cb = Some(Box::new(f));
        self
    }

    /// Resolución de radios usando el tema como fallback.
    fn resolved_radii(&self, theme_radius: f32) -> [f32; 4] {
        [
            self.radii[0].unwrap_or(theme_radius),
            self.radii[1].unwrap_or(theme_radius),
            self.radii[2].unwrap_or(theme_radius),
            self.radii[3].unwrap_or(theme_radius),
        ]
    }
}

impl<App> Widget<App> for Button<App> {
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = self.resolved_radii(ctx.theme.border_radius);

        // ── Sombra (solo cuando no está presionado, para dar efecto de profundidad) ──
        if self.chrome && !self.is_pressed && !self.disabled {
            cmds.push(RenderCommand::shadow_sm(ctx.rect));
        }

        // ── Fondo ──────────────────────────────────────────────────────────────
        let base_color = if self.disabled {
            ctx.theme.on_surface_muted.with_alpha(0.4).to_array()
        } else if self.is_pressed {
            // Presionado: más oscuro y desplazado visualmente
            ctx.theme.primary_variant.to_array()
        } else if self.is_hovered {
            // Hover: ligeramente más claro que el primario
            ctx.theme.primary_variant.with_alpha(0.88).to_array()
        } else {
            ctx.theme.primary.to_array()
        };

        match &self.background {
            Background::None => {
                cmds.push(RenderCommand::Quad {
                    rect: ctx.rect,
                    color: base_color,
                    radii: r,
                    flags: 0,
                });
            }
            Background::Solid(color) => {
                cmds.push(RenderCommand::Quad {
                    rect: ctx.rect,
                    color: *color,
                    radii: r,
                    flags: 0,
                });
            }
            other => {
                // Fondo sólido base
                cmds.push(RenderCommand::Quad {
                    rect: ctx.rect,
                    color: base_color,
                    radii: r,
                    flags: 0,
                });
                // Degradado o fondo procedural encima
                cmds.push(RenderCommand::GradientQuad {
                    rect: ctx.rect,
                    background: other.clone(),
                    radii: r,
                    raster_resolution: (0, 0),
                });
            }
        }

        // ── Borde sutil en hover ────────────────────────────────────────────────
        if self.chrome && self.is_hovered && !self.disabled && !self.is_pressed {
            cmds.push(RenderCommand::Border {
                rect: ctx.rect,
                color: ctx.theme.primary.with_alpha(0.6).to_array(),
                radii: r,
                width: 1.0,
            });
        }

        // ── Texto ───────────────────────────────────────────────────────────────
        let text_color = if self.disabled {
            ctx.theme.on_primary.with_alpha(0.5).to_array()
        } else {
            ctx.theme.on_primary.to_array()
        };

        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.label.clone(),
            color: text_color,
            font_size: ctx.theme.font_size_base,
            align: self.text_align,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        // Estimación proporcional. El layout engine puede ajustar con Flex.
        let char_advance_avg = 8.0;
        let w = self.label.len() as f32 * char_advance_avg + 32.0;
        Vec2::new(w.max(64.0), 36.0)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        if self.disabled {
            return EventResponse::Ignored;
        }

        match event {
            // ── Hover ────────────────────────────────────────────────────────────
            UiEvent::MouseEnter => {
                self.is_hovered = true;
                if let Some(cb) = &self.on_hover_cb {
                    cb(ctx);
                }
                EventResponse::Redraw
            }
            UiEvent::MouseLeave => {
                self.is_hovered = false;
                self.is_pressed = false; // cancelar el press si el cursor sale
                if let Some(cb) = &self.on_hover_end_cb {
                    cb(ctx);
                }
                EventResponse::Redraw
            }

            // ── Click: Down → marcar presionado, Up → disparar callback ──────────
            UiEvent::MouseDown { button: crate::MouseButton::Left, .. } => {
                self.is_pressed = true;
                EventResponse::Redraw
            }
            UiEvent::MouseUp { button: crate::MouseButton::Left, pos } => {
                let was_pressed = self.is_pressed;
                self.is_pressed = false;
                // Solo disparar el click si el mouse sube dentro del área del botón
                let pos_inside = ctx.rect.contains([pos.x, pos.y]);
                if was_pressed && pos_inside {
                    if let Some(cb) = &self.on_click_cb {
                        cb(ctx);
                    }
                }
                EventResponse::Redraw
            }

            // ── Click derecho ────────────────────────────────────────────────────
            UiEvent::MouseDown { button: crate::MouseButton::Right, .. } => {
                if let Some(cb) = &self.on_right_click_cb {
                    cb(ctx);
                }
                EventResponse::Consumed
            }

            _ => EventResponse::Ignored,
        }
    }

    /// Hit-testing personalizado: si el botón es perfectamente redondo, usar SDF circular.
    fn hit_test(&self, local_pos: Vec2, size: Vec2) -> bool {
        let r = self.radii[0];
        // Si todos los radios son iguales y >= la mitad del lado menor → es un círculo/óvalo
        let is_circle = self.radii.iter().all(|rr| *rr == r)
            && r.map_or(false, |rv| rv * 2.0 >= size.x.min(size.y));
        if is_circle {
            let center = size * 0.5;
            let radius = size.x.min(size.y) * 0.5;
            local_pos.distance(center) <= radius
        } else {
            // AABB estándar — el shader hace el resto para esquinas redondeadas visuales
            local_pos.x >= 0.0 && local_pos.x <= size.x &&
            local_pos.y >= 0.0 && local_pos.y <= size.y
        }
    }

    fn reflect(&self) -> Option<&dyn crate::FerrousWidgetReflect> {
        Some(self)
    }

    fn reflect_mut(&mut self) -> Option<&mut dyn crate::FerrousWidgetReflect> {
        Some(self)
    }
}
