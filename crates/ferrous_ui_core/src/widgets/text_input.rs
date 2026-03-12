use crate::{
    text_field_state::{FieldKey, FieldKeyResult, TextFieldState},
    DrawContext, EventContext, EventResponse, GuiKey, LayoutContext, Rect, RenderCommand, UiEvent,
    Vec2, Widget,
};

/// Widget de entrada de texto de una sola línea.
///
/// Soporta:
/// - Cursor posicionable con flechas izquierda/derecha, Home, End.
/// - Salto de palabra con Ctrl+←/→.
/// - Selección con Shift+flechas, Shift+Home/End, Ctrl+Shift+←/→ y Ctrl+A.
/// - Click para posicionar el cursor, Shift+click para extender la selección.
/// - Portapapeles: Ctrl+C (copiar), Ctrl+X (cortar), Ctrl+V (pegar).
/// - Undo/Redo: Ctrl+Z / Ctrl+Y.
/// - Backspace mantenido con repeat, Delete, inserción de caracteres.
/// - Scroll horizontal cuando el texto supera el ancho del campo.
pub struct TextInput<App> {
    pub text: String,
    pub placeholder: String,
    pub is_focused: bool,
    pub binding: Option<std::sync::Arc<crate::Observable<String>>>,
    on_submit_cb: Option<Box<dyn Fn(&mut EventContext<App>, &str) + Send + Sync>>,
    /// Toda la lógica de animación/cursor/selección delegada aquí.
    pub field_state: TextFieldState,
    /// Desplazamiento horizontal en píxeles para el scroll del texto.
    scroll_offset: f32,
    /// Callback para leer el portapapeles (debe ser seteado por el host).
    clipboard_read_cb: Option<Box<dyn Fn() -> String + Send + Sync>>,
    /// Callback para escribir al portapapeles.
    clipboard_write_cb: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl<App> TextInput<App> {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            placeholder: placeholder.into(),
            is_focused: false,
            binding: None,
            on_submit_cb: None,
            field_state: TextFieldState::new(),
            scroll_offset: 0.0,
            clipboard_read_cb: None,
            clipboard_write_cb: None,
        }
    }

    pub fn with_binding(
        mut self,
        observable: std::sync::Arc<crate::Observable<String>>,
        node_id: crate::NodeId,
    ) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }

    pub fn on_submit(
        mut self,
        f: impl Fn(&mut EventContext<App>, &str) + Send + Sync + 'static,
    ) -> Self {
        self.on_submit_cb = Some(Box::new(f));
        self
    }

    /// Provee callbacks de portapapeles para que Ctrl+C/X/V funcionen.
    pub fn with_clipboard(
        mut self,
        read: impl Fn() -> String + Send + Sync + 'static,
        write: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        self.clipboard_read_cb = Some(Box::new(read));
        self.clipboard_write_cb = Some(Box::new(write));
        self
    }

    // Convenience accessors kept for backward compat
    pub fn cursor_pos(&self) -> usize {
        self.field_state.cursor_pos
    }
    pub fn all_selected(&self) -> bool {
        self.field_state.all_selected
    }

    fn current_text(&self) -> String {
        self.binding
            .as_ref()
            .map(|o| o.get())
            .unwrap_or_else(|| self.text.clone())
    }

    fn commit_text(&mut self, ctx: &mut EventContext<App>, new_text: String) {
        if let Some(o) = &self.binding {
            let dirty = o.set(new_text);
            ctx.tree.reactivity.notify_change(dirty);
        } else {
            self.text = new_text;
        }
    }

    /// Actualiza `scroll_offset` para que el cursor siempre sea visible.
    fn update_scroll(&mut self, visible_width: f32, char_width: f32) {
        let text = self.current_text();
        let cursor_char = text[..self.field_state.cursor_pos].chars().count();
        let cursor_x = cursor_char as f32 * char_width;

        if cursor_x < self.scroll_offset {
            self.scroll_offset = (cursor_x - 8.0).max(0.0);
        } else if cursor_x > self.scroll_offset + visible_width {
            self.scroll_offset = cursor_x - visible_width + 8.0;
        }
    }
}

impl<App> Widget<App> for TextInput<App> {
    fn update(&mut self, ctx: &mut crate::UpdateContext) {
        if !self.is_focused {
            self.field_state.blur();
            return;
        }
        if self.field_state.tick(ctx.delta_time) {
            ctx.needs_redraw = true;
        }
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        let text = self.current_text();
        let char_width = theme.font_size_base * 0.55;
        let text_area_x = r.x + 8.0;
        let text_area_w = r.width - 16.0;

        // Fondo
        let bg_color = if self.is_focused {
            theme.surface_elevated
        } else {
            theme.surface
        };
        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: bg_color.to_array(),
            radii: [theme.border_radius; 4],
            flags: 0,
        });

        // Clip del área de texto
        cmds.push(RenderCommand::PushClip {
            rect: Rect::new(text_area_x, r.y, text_area_w, r.height),
        });

        // ── Resaltado de selección ──────────────────────────────────────────
        if self.is_focused {
            if self.field_state.all_selected && !text.is_empty() {
                // Toda la selección
                let sel_width = (text.chars().count() as f32 * char_width - self.scroll_offset)
                    .max(0.0)
                    .min(text_area_w);
                cmds.push(RenderCommand::Quad {
                    rect: Rect::new(text_area_x, r.y + 4.0, sel_width, r.height - 8.0),
                    color: theme.primary.with_alpha(0.25).to_array(),
                    radii: [2.0; 4],
                    flags: 0,
                });
            } else if let Some((sel_start, sel_end)) = self.field_state.selection() {
                // Selección parcial
                let start_chars = text[..sel_start].chars().count();
                let end_chars = text[..sel_end].chars().count();
                let sel_x = text_area_x + (start_chars as f32 * char_width) - self.scroll_offset;
                let sel_w = (end_chars - start_chars) as f32 * char_width;
                // Solo dibujar la parte visible
                let clipped_x = sel_x.max(text_area_x);
                let clipped_w = (sel_x + sel_w).min(text_area_x + text_area_w) - clipped_x;
                if clipped_w > 0.0 {
                    cmds.push(RenderCommand::Quad {
                        rect: Rect::new(clipped_x, r.y + 4.0, clipped_w, r.height - 8.0),
                        color: theme.primary.with_alpha(0.25).to_array(),
                        radii: [2.0; 4],
                        flags: 0,
                    });
                }
            }
        }

        // Texto o placeholder (con scroll)
        let display_text = if text.is_empty() {
            &self.placeholder
        } else {
            &text
        };
        let text_color = if text.is_empty() {
            theme.on_surface_muted
        } else {
            theme.on_surface
        };
        cmds.push(RenderCommand::Text {
            rect: Rect::new(
                text_area_x - self.scroll_offset,
                r.y,
                text_area_w + self.scroll_offset,
                r.height,
            ),
            text: display_text.to_string(),
            color: text_color.to_array(),
            font_size: theme.font_size_base,
            align: crate::TextAlign::TOP_LEFT,
        });

        // Cursor parpadeante
        if self.is_focused && self.field_state.cursor_visible && !self.field_state.has_selection() {
            let cursor_chars = if text.is_empty() {
                0
            } else {
                text[..self.field_state.cursor_pos].chars().count()
            };
            let cursor_x = text_area_x + (cursor_chars as f32 * char_width) - self.scroll_offset;
            if cursor_x >= text_area_x && cursor_x <= text_area_x + text_area_w {
                cmds.push(RenderCommand::Quad {
                    rect: Rect::new(cursor_x, r.y + 4.0, 2.0, r.height - 8.0),
                    color: theme.primary.to_array(),
                    radii: [0.0; 4],
                    flags: 0,
                });
            }
        }

        cmds.push(RenderCommand::PopClip);

        // Borde inferior (indicador de foco)
        let border_color = if self.is_focused {
            theme.primary
        } else {
            theme.on_surface_muted.with_alpha(0.3)
        };
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y + r.height - 1.0, r.width, 1.0),
            color: border_color.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(200.0, 32.0)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        let mut text = self.current_text();

        match event {
            UiEvent::MouseDown { pos, .. } => {
                let r = ctx.rect;
                let was_focused = self.is_focused;
                self.is_focused = true;
                self.field_state.focus();

                // Calcular posición del cursor según el click
                let char_width = ctx.theme.font_size_base * 0.55;
                let text_area_x = r.x + 8.0;
                self.field_state.click_at(
                    &text,
                    pos.x,
                    text_area_x,
                    char_width,
                    self.scroll_offset,
                    false, // Shift+click se maneja por separado si se necesita
                );

                // Si antes no había foco, seleccionar todo al hacer doble-click se podría agregar aquí
                let _ = was_focused;
                EventResponse::Redraw
            }

            UiEvent::Char { c } if self.is_focused => {
                self.field_state.on_char(*c, &mut text);
                let char_width = ctx.theme.font_size_base * 0.55;
                let visible_w = ctx.rect.width - 16.0;
                self.update_scroll(visible_w, char_width);
                self.commit_text(ctx, text);
                EventResponse::Redraw
            }

            UiEvent::KeyDown { key } if self.is_focused => {
                // Map GuiKey → FieldKey
                let fkey = match key {
                    GuiKey::CtrlA => FieldKey::SelectAll,
                    GuiKey::CtrlC => FieldKey::Copy,
                    GuiKey::CtrlX => FieldKey::Cut,
                    GuiKey::CtrlV => FieldKey::Paste,
                    GuiKey::CtrlZ => FieldKey::Undo,
                    GuiKey::CtrlY => FieldKey::Redo,
                    GuiKey::CtrlArrowLeft => FieldKey::CtrlArrowLeft,
                    GuiKey::CtrlArrowRight => FieldKey::CtrlArrowRight,
                    GuiKey::CtrlShiftArrowLeft => FieldKey::CtrlShiftArrowLeft,
                    GuiKey::CtrlShiftArrowRight => FieldKey::CtrlShiftArrowRight,
                    GuiKey::ShiftArrowLeft => FieldKey::ShiftArrowLeft,
                    GuiKey::ShiftArrowRight => FieldKey::ShiftArrowRight,
                    GuiKey::ShiftHome => FieldKey::ShiftHome,
                    GuiKey::ShiftEnd => FieldKey::ShiftEnd,
                    GuiKey::Backspace => FieldKey::Backspace,
                    GuiKey::Delete => FieldKey::Delete,
                    GuiKey::ArrowLeft => FieldKey::ArrowLeft,
                    GuiKey::ArrowRight => FieldKey::ArrowRight,
                    GuiKey::Home => FieldKey::Home,
                    GuiKey::End => FieldKey::End,
                    GuiKey::Enter => FieldKey::Enter,
                    GuiKey::Escape => FieldKey::Escape,
                    GuiKey::Tab => FieldKey::Tab,
                    _ => return EventResponse::Ignored,
                };

                // Obtener texto del portapapeles para Paste
                let clipboard_text_owned: Option<String> = if fkey == FieldKey::Paste {
                    self.clipboard_read_cb.as_ref().map(|f| f())
                } else {
                    None
                };
                let clipboard_str = clipboard_text_owned.as_deref();

                let result = self
                    .field_state
                    .on_key_with_clipboard(fkey, &mut text, clipboard_str);

                // Actualizar scroll después de cualquier cambio
                let char_width = ctx.theme.font_size_base * 0.55;
                let visible_w = ctx.rect.width - 16.0;

                match result {
                    FieldKeyResult::Ignored => return EventResponse::Ignored,
                    FieldKeyResult::Handled => {
                        self.update_scroll(visible_w, char_width);
                        self.commit_text(ctx, text);
                        EventResponse::Redraw
                    }
                    FieldKeyResult::Submit => {
                        if let Some(cb) = &self.on_submit_cb {
                            cb(ctx, &text);
                        }
                        self.is_focused = false;
                        self.field_state.blur();
                        self.scroll_offset = 0.0;
                        EventResponse::Redraw
                    }
                    FieldKeyResult::Cancel => {
                        self.is_focused = false;
                        self.field_state.blur();
                        self.scroll_offset = 0.0;
                        EventResponse::Redraw
                    }
                    FieldKeyResult::CopyToClipboard(copied) => {
                        if let Some(write_cb) = &self.clipboard_write_cb {
                            write_cb(&copied);
                        }
                        // Para Cut también se modificó el buffer
                        self.update_scroll(visible_w, char_width);
                        self.commit_text(ctx, text);
                        EventResponse::Redraw
                    }
                }
            }

            UiEvent::KeyUp {
                key: GuiKey::Backspace,
            } if self.is_focused => {
                self.field_state.on_backspace_released();
                EventResponse::Consumed
            }

            _ => EventResponse::Ignored,
        }
    }
}
