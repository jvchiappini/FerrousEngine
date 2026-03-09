//! # `ToastManager` — Notificaciones Efímeras con Animación
//!
//! El sistema de toasts se compone de dos partes:
//!
//! - **[`Toast`]**: Descriptor de una notificación individual (texto, duración, nivel).
//! - **[`ToastManager`]**: Widget colocado una sola vez en el árbol raíz que gestiona
//!   la cola de toasts activos, sus animaciones y el pintado final.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{ToastManager, Toast, ToastLevel};
//!
//! // 1. Añadir el manager como último hijo del root (z-order correcto)
//! let manager_id = tree.add_node(Box::new(ToastManager::<MyApp>::new()), Some(root_id));
//!
//! // 2. Desde cualquier callback, enviar un toast a través de la App:
//! Button::new("Guardar").on_click(|ctx| {
//!     ctx.app.toasts.push(Toast::success("Proyecto guardado"));
//!     ctx.tree.mark_paint_dirty(ctx.app.toast_manager_id);
//! });
//! ```
//!
//! ## Arquitectura
//!
//! El `ToastManager` no depende de nodos hijos: emite todos sus `RenderCommand`
//! directamente en `draw()` y anima los toasts en `update()`. Esto garantiza que
//! los toasts siempre aparezcan **sobre todo el contenido** sin reordenar el árbol.
//!
//! ### Cola de toasts
//!
//! Cada `Toast` tiene un ciclo de vida:
//!
//! ```text
//! elapsed < enter_duration   → slide-in  (offset_y: height→0, alpha: 0→1)
//! enter_duration ≤ t < total → visible   (offset_y: 0, alpha: 1)
//! total - exit_duration ≤ t  → slide-out (offset_y: 0→height, alpha: 1→0)
//! t ≥ total                  → eliminado de la cola
//! ```
//!
//! La interpolación usa una curva `ease_out` suave para un movimiento natural.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, Color,
};

// ─── ToastLevel ──────────────────────────────────────────────────────────────

/// Nivel semántico de un toast. Controla el color del acento lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    /// Información general. Acento azul (`theme.primary`).
    Info,
    /// Operación completada con éxito. Acento verde.
    Success,
    /// Advertencia no crítica. Acento naranja/amarillo.
    Warning,
    /// Error o fallo. Acento rojo.
    Error,
}

impl ToastLevel {
    /// Devuelve el color RGBA del acento lateral del toast.
    pub fn accent_color(&self) -> [f32; 4] {
        match self {
            ToastLevel::Info    => [0.42, 0.39, 1.00, 1.0],  // azul-violeta
            ToastLevel::Success => [0.24, 0.80, 0.44, 1.0],  // verde
            ToastLevel::Warning => [1.00, 0.70, 0.15, 1.0],  // naranja
            ToastLevel::Error   => [0.95, 0.27, 0.36, 1.0],  // rojo
        }
    }

    /// Devuelve el emoji o símbolo asociado.
    pub fn icon(&self) -> &'static str {
        match self {
            ToastLevel::Info    => "ℹ",
            ToastLevel::Success => "✓",
            ToastLevel::Warning => "⚠",
            ToastLevel::Error   => "✕",
        }
    }
}

// ─── Toast (descriptor) ──────────────────────────────────────────────────────

/// Descriptor de una notificación efímera.
///
/// Crea nuevos toasts con los constructores semánticos:
/// [`Toast::info`], [`Toast::success`], [`Toast::warning`], [`Toast::error`].
#[derive(Clone)]
pub struct Toast {
    /// Texto principal del toast.
    pub message: String,
    /// Nivel semántico (controla el color del acento).
    pub level: ToastLevel,
    /// Duración total visible en segundos (por defecto `3.0`).
    pub duration_secs: f32,
    /// Duración de la animación de entrada en segundos (por defecto `0.25`).
    pub enter_secs: f32,
    /// Duración de la animación de salida en segundos (por defecto `0.30`).
    pub exit_secs: f32,

    // Estado interno de animación — gestionado por `ToastManager`
    pub(crate) elapsed: f32,
}

impl Toast {
    /// Crea un toast informativo.
    pub fn info(message: impl Into<String>) -> Self {
        Self::with_level(message, ToastLevel::Info)
    }

    /// Crea un toast de éxito.
    pub fn success(message: impl Into<String>) -> Self {
        Self::with_level(message, ToastLevel::Success)
    }

    /// Crea un toast de advertencia.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::with_level(message, ToastLevel::Warning)
    }

    /// Crea un toast de error.
    pub fn error(message: impl Into<String>) -> Self {
        Self::with_level(message, ToastLevel::Error)
    }

    fn with_level(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            duration_secs: 3.0,
            enter_secs: 0.25,
            exit_secs: 0.30,
            elapsed: 0.0,
        }
    }

    /// Duración total personalizada en segundos.
    pub fn duration(mut self, secs: f32) -> Self {
        self.duration_secs = secs;
        self
    }

    /// ¿Ha expirado completamente este toast?
    pub fn is_expired(&self) -> bool {
        self.elapsed >= self.duration_secs
    }

    /// Devuelve el alpha del toast en el instante actual (0.0–1.0).
    fn alpha(&self) -> f32 {
        let t = self.elapsed;
        let total = self.duration_secs;
        let enter = self.enter_secs;
        let exit_start = total - self.exit_secs;

        if t < enter {
            ease_out(t / enter)
        } else if t < exit_start {
            1.0
        } else {
            ease_out(1.0 - (t - exit_start) / self.exit_secs)
        }
    }

    /// Devuelve el desplazamiento Y del toast (slide desde abajo).
    /// En entrada: desliza desde `height` hasta 0. En salida: al revés.
    fn slide_y(&self, height: f32) -> f32 {
        let t = self.elapsed;
        let total = self.duration_secs;
        let enter = self.enter_secs;
        let exit_start = total - self.exit_secs;

        if t < enter {
            height * (1.0 - ease_out(t / enter))
        } else if t < exit_start {
            0.0
        } else {
            height * ease_out((t - exit_start) / self.exit_secs)
        }
    }
}

/// Curva ease-out cúbica para animaciones suaves.
fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

// ─── ToastManager ────────────────────────────────────────────────────────────

const TOAST_W: f32 = 320.0;
const TOAST_H: f32 = 52.0;
const TOAST_GAP: f32 = 8.0;
const TOAST_MARGIN_X: f32 = 16.0;
const TOAST_MARGIN_Y: f32 = 16.0;
const ACCENT_W: f32 = 4.0;
const ICON_W: f32 = 32.0;

/// Widget manager de toasts. Debe ser el **último hijo del nodo raíz**.
///
/// Gestiona una cola interna de [`Toast`]s activos, sus animaciones y el pintado.
/// Los toasts se añaden desde fuera mediante el campo público `queue`.
pub struct ToastManager<App> {
    /// Cola de toasts pendientes/activos. Agrega aquí nuevos toasts desde callbacks.
    pub queue: Vec<Toast>,
    /// Si `true`, los toasts se apilan en la esquina inferior derecha (por defecto).
    /// Si `false`, se apilan en la esquina inferior izquierda.
    pub anchor_right: bool,

    _marker: std::marker::PhantomData<App>,
}

impl<App> ToastManager<App> {
    /// Crea un manager vacío anclado a la esquina inferior derecha.
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            anchor_right: true,
            _marker: std::marker::PhantomData,
        }
    }

    /// Ancla los toasts a la esquina inferior izquierda.
    pub fn anchor_left(mut self) -> Self {
        self.anchor_right = false;
        self
    }

    /// Añade un toast a la cola.
    pub fn push(&mut self, toast: Toast) {
        self.queue.push(toast);
    }
}

impl<App> Default for ToastManager<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: Send + Sync + 'static> Widget<App> for ToastManager<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        // Posición absoluta, fill total (para poder dibujar en cualquier esquina)
        let style = crate::StyleBuilder::new()
            .absolute()
            .top(0.0).left(0.0)
            .fill_width().fill_height()
            .build();
        ctx.tree.set_node_style(ctx.node_id, style);
    }

    fn update(&mut self, ctx: &mut crate::UpdateContext) {
        let dt = ctx.delta_time;
        let mut needs_redraw = false;

        // Avanzar todos los toasts
        for toast in &mut self.queue {
            toast.elapsed += dt;
            needs_redraw = true;
        }

        // Eliminar los expirados
        let before = self.queue.len();
        self.queue.retain(|t| !t.is_expired());
        if self.queue.len() != before {
            needs_redraw = true;
        }
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        if self.queue.is_empty() {
            return;
        }

        let theme = &ctx.theme;
        let viewport = &ctx.rect;

        // Empezamos desde la esquina inferior y apilamos hacia arriba
        let base_x = if self.anchor_right {
            viewport.x + viewport.width - TOAST_W - TOAST_MARGIN_X
        } else {
            viewport.x + TOAST_MARGIN_X
        };

        let mut base_y = viewport.y + viewport.height - TOAST_MARGIN_Y - TOAST_H;

        for toast in self.queue.iter().rev() {
            let alpha = toast.alpha();
            if alpha <= 0.01 {
                base_y -= TOAST_H + TOAST_GAP;
                continue;
            }

            let slide = toast.slide_y(TOAST_H + 16.0);
            let actual_y = base_y + slide;
            let tr = Rect::new(base_x, actual_y, TOAST_W, TOAST_H);

            // ── Sombra ───────────────────────────────────────────────────
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(tr.x + 3.0, tr.y + 4.0, tr.width, tr.height),
                color: Color::BLACK.with_alpha(0.22 * alpha).to_array(),
                radii: [6.0; 4],
                flags: 0,
            });

            // ── Panel de fondo ────────────────────────────────────────────
            cmds.push(RenderCommand::Quad {
                rect: tr,
                color: theme.surface_elevated.with_alpha(alpha).to_array(),
                radii: [6.0; 4],
                flags: 0,
            });

            // ── Acento lateral ────────────────────────────────────────────
            let accent_col = toast.level.accent_color();
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(tr.x, tr.y + 2.0, ACCENT_W, tr.height - 4.0),
                color: [accent_col[0], accent_col[1], accent_col[2], accent_col[3] * alpha],
                radii: [3.0, 0.0, 0.0, 3.0],
                flags: 0,
            });

            // ── Icono ─────────────────────────────────────────────────────
            cmds.push(RenderCommand::Text {
                rect: Rect::new(tr.x + ACCENT_W + 8.0, tr.y, ICON_W, tr.height),
                text: toast.level.icon().to_string(),
                color: [accent_col[0], accent_col[1], accent_col[2], accent_col[3] * alpha],
                font_size: 14.0,
            });

            // ── Mensaje ───────────────────────────────────────────────────
            cmds.push(RenderCommand::Text {
                rect: Rect::new(
                    tr.x + ACCENT_W + ICON_W + 4.0,
                    tr.y,
                    tr.width - ACCENT_W - ICON_W - 12.0,
                    tr.height,
                ),
                text: toast.message.clone(),
                color: theme.on_surface.with_alpha(alpha).to_array(),
                font_size: theme.font_size_base,
            });

            // ── Barra de progreso de tiempo restante ─────────────────────
            let progress = 1.0 - (toast.elapsed / toast.duration_secs).clamp(0.0, 1.0);
            let progress_y = tr.y + tr.height - 2.0;
            let progress_w = (tr.width - ACCENT_W) * progress;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(tr.x + ACCENT_W, progress_y, tr.width - ACCENT_W, 2.0),
                color: theme.on_surface_muted.with_alpha(0.1 * alpha).to_array(),
                radii: [0.0; 4],
                flags: 0,
            });
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(tr.x + ACCENT_W, progress_y, progress_w, 2.0),
                color: [accent_col[0], accent_col[1], accent_col[2], 0.6 * alpha],
                radii: [0.0; 4],
                flags: 0,
            });

            base_y -= TOAST_H + TOAST_GAP;
        }
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO // Absolute, no participa en el flujo de layout
    }

    fn on_event(
        &mut self,
        _ctx: &mut EventContext<App>,
        _event: &UiEvent,
    ) -> EventResponse {
        // Los toasts no consumen eventos — son informativos, no bloqueantes
        EventResponse::Ignored
    }
}
