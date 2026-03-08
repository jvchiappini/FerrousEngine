use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2, GuiKey};

// ─── TextInput ───────────────────────────────────────────────────────────────

/// Widget de entrada de texto de una sola línea (Fase 6.1).
pub struct TextInput<App> {
    pub text: String,
    pub placeholder: String,
    pub cursor_pos: usize,
    pub is_focused: bool,
    on_submit_cb: Option<Box<dyn Fn(&mut EventContext<App>, &str) + Send + Sync>>,
}

impl<App> TextInput<App> {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            placeholder: placeholder.into(),
            cursor_pos: 0,
            is_focused: false,
            on_submit_cb: None,
        }
    }

    pub fn on_submit(mut self, f: impl Fn(&mut EventContext<App>, &str) + Send + Sync + 'static) -> Self {
        self.on_submit_cb = Some(Box::new(f));
        self
    }
}

impl<App> Widget<App> for TextInput<App> {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        
        // Fondo
        let bg_color = if self.is_focused { theme.surface_variant } else { theme.surface };
        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: bg_color.to_array(),
            radii: [theme.border_radius; 4],
            flags: 0,
        });

        // Borde
        let border_color = if self.is_focused { theme.primary } else { theme.on_surface_muted.with_alpha(0.3) };
        // Dibujamos el borde como un quad ligeramente más grande o 4 líneas
        // Simplificado: un quad hueco no existe, usamos el truco de 1px
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y + r.height - 1.0, r.width, 1.0),
            color: border_color.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        // Texto o Placeholder
        let display_text = if self.text.is_empty() { &self.placeholder } else { &self.text };
        let text_color = if self.text.is_empty() { theme.on_surface_muted } else { theme.on_surface };
        
        cmds.push(RenderCommand::Text {
            rect: Rect::new(r.x + 8.0, r.y, r.width - 16.0, r.height),
            text: display_text.to_string(),
            color: text_color.to_array(),
            font_size: theme.font_size_base,
        });

        // Cursor
        if self.is_focused {
            let char_width = theme.font_size_base * 0.55;
            let cursor_x = r.x + 8.0 + (self.cursor_pos as f32 * char_width);
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(cursor_x, r.y + 4.0, 2.0, r.height - 8.0),
                color: theme.primary.to_array(),
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(200.0, 32.0)
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseDown { .. } => {
                self.is_focused = true;
                EventResponse::Redraw
            }
            UiEvent::Char { c } if self.is_focused => {
                if !c.is_control() {
                    self.text.insert(self.cursor_pos, *c);
                    self.cursor_pos += 1;
                    EventResponse::Redraw
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::KeyDown { key } if self.is_focused => {
                match key {
                    GuiKey::Backspace => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            self.text.remove(self.cursor_pos);
                            EventResponse::Redraw
                        } else {
                            EventResponse::Ignored
                        }
                    }
                    GuiKey::ArrowLeft => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            EventResponse::Redraw
                        } else {
                            EventResponse::Ignored
                        }
                    }
                    GuiKey::ArrowRight => {
                        if self.cursor_pos < self.text.len() {
                            self.cursor_pos += 1;
                            EventResponse::Redraw
                        } else {
                            EventResponse::Ignored
                        }
                    }
                    GuiKey::Enter => {
                        if let Some(cb) = &self.on_submit_cb {
                            cb(ctx, &self.text);
                        }
                        self.is_focused = false;
                        EventResponse::Redraw
                    }
                    GuiKey::Escape => {
                        self.is_focused = false;
                        EventResponse::Redraw
                    }
                    _ => EventResponse::Ignored,
                }
            }
            _ => EventResponse::Ignored,
        }
    }
}
