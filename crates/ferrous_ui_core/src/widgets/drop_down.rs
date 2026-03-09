use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Vec2};

// ─── DropDown ────────────────────────────────────────────────────────────────

/// Selector desplegable con lista de opciones (Fase 6.1).
pub struct DropDown<App> {
    pub options: Vec<String>,
    pub selected_index: usize,
    pub is_open: bool,
    pub binding: Option<std::sync::Arc<crate::Observable<usize>>>,
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, usize) + Send + Sync>>,
}

impl<App> DropDown<App> {
    /// Crea un nuevo selector con las opciones dadas.
    pub fn new(options: Vec<impl Into<String>>) -> Self {
        Self {
            options: options.into_iter().map(|s| s.into()).collect(),
            selected_index: 0,
            is_open: false,
            binding: None,
            on_change_cb: None,
        }
    }

    /// Vincula el selector a un `Observable<usize>`.
    pub fn with_binding(mut self, observable: std::sync::Arc<crate::Observable<usize>>, node_id: crate::NodeId) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }

    /// Registra un callback que se invoca al cambiar la selección.
    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, usize) + Send + Sync + 'static) -> Self {
        self.on_change_cb = Some(Box::new(f));
        self
    }

    fn update_selection(&mut self, ctx: &mut EventContext<App>, index: usize) {
        if let Some(o) = &self.binding {
            let dirty = o.set(index);
            ctx.tree.reactivity.notify_change(dirty);
        } else {
            self.selected_index = index;
        }

        if let Some(cb) = &self.on_change_cb {
            cb(ctx, index);
        }
    }
}

impl<App> Widget<App> for DropDown<App> {
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let selected_index = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.selected_index);
        let text = self.options.get(selected_index).cloned().unwrap_or_else(|| "Select...".to_string());
        
        // Dibujar el botón base (Trigger)
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: ctx.theme.surface_elevated.to_array(),
            radii: [ctx.theme.border_radius; 4],
            flags: 0,
        });

        cmds.push(RenderCommand::Text {
            rect: crate::Rect {
                x: ctx.rect.x + 8.0,
                y: ctx.rect.y,
                width: ctx.rect.width - 24.0,
                height: ctx.rect.height,
            },
            text: format!("{} ▼", text),
            color: ctx.theme.on_surface.to_array(),
            font_size: ctx.theme.font_size_base,
        });

        // Si está abierto, dibujar la lista de opciones
        if self.is_open {
            let item_h = 30.0;
            let list_h = self.options.len() as f32 * item_h;
            let list_rect = crate::Rect {
                x: ctx.rect.x,
                y: ctx.rect.y + ctx.rect.height + 2.0, // Pequeño gap
                width: ctx.rect.width,
                height: list_h,
            };

            // Fondo de la lista
            cmds.push(RenderCommand::Quad {
                rect: list_rect,
                color: ctx.theme.surface.to_array(),
                radii: [ctx.theme.border_radius; 4],
                flags: 0,
            });

            // Items
            for (i, opt) in self.options.iter().enumerate() {
                let item_rect = crate::Rect {
                    x: list_rect.x,
                    y: list_rect.y + i as f32 * item_h,
                    width: list_rect.width,
                    height: item_h,
                };

                // Highlight de selección
                if i == selected_index {
                    cmds.push(RenderCommand::Quad {
                        rect: item_rect,
                        color: ctx.theme.primary.with_alpha(0.2).to_array(),
                        radii: [0.0; 4],
                        flags: 0,
                    });
                }

                cmds.push(RenderCommand::Text {
                    rect: crate::Rect {
                        x: item_rect.x + 8.0,
                        y: item_rect.y,
                        width: item_rect.width - 16.0,
                        height: item_rect.height,
                    },
                    text: opt.clone(),
                    color: ctx.theme.on_surface.to_array(),
                    font_size: ctx.theme.font_size_base,
                });
            }
        }
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(150.0, 36.0)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown { pos, .. } => {
                let p = [pos.x, pos.y];
                
                // Click en el trigger
                if ctx.rect.contains(p) {
                    self.is_open = !self.is_open;
                    return EventResponse::Redraw;
                }

                // Click en las opciones si está abierto
                if self.is_open {
                    let item_h = 30.0;
                    let list_y = ctx.rect.y + ctx.rect.height + 2.0;
                    for i in 0..self.options.len() {
                        let item_rect = crate::Rect {
                            x: ctx.rect.x,
                            y: list_y + i as f32 * item_h,
                            width: ctx.rect.width,
                            height: item_h,
                        };
                        if item_rect.contains(p) {
                            self.update_selection(ctx, i);
                            self.is_open = false;
                            return EventResponse::Redraw;
                        }
                    }
                    // Click fuera (dentro de la lógica del drop down)
                    self.is_open = false;
                    return EventResponse::Redraw;
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }
}
