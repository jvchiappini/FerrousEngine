//! [`TextFieldState`] — estado reutilizable para cualquier campo de texto de una línea.
//!
//! Encapsula de forma **independiente del backend**:
//! - Parpadeo del cursor (configurable).
//! - Backspace mantenido con retardo inicial y cadencia de repeat.
//! - Ctrl+A para seleccionar todo.
//! - Ctrl+←/→ para saltar palabras.
//! - Shift+←/→, Shift+Home/End, Ctrl+Shift+←/→ para seleccionar con teclado.
//! - Ctrl+C/X para copiar/cortar (devuelve el texto en el resultado).
//! - Ctrl+V para pegar (el llamador provee el texto del portapapeles).
//! - Ctrl+Z/Y para deshacer/rehacer.
//! - Posición del cursor y rango de selección en el texto.
//!
//! ## Uso rápido
//!
//! ```rust,ignore
//! // En tu estado de aplicación o widget:
//! let mut field = TextFieldState::new();
//!
//! // Cada frame (delta en segundos):
//! field.tick(dt);
//!
//! // Al recibir eventos de teclado:
//! field.on_key(key, &mut buffer, ctrl_held);
//!
//! // Al recibir caracteres tipados:
//! field.on_char(c, &mut buffer);
//!
//! // Al enfocar / desenfocar:
//! field.focus();
//! field.blur();
//!
//! // En el render, consulta:
//! field.cursor_visible   // ¿mostrar cursor?
//! field.selection()      // Option<(usize, usize)> rango normalizado de selección
//! field.cursor_pos       // índice de char en el buffer
//! ```

// ─── Constantes ───────────────────────────────────────────────────────────────

/// Duración de la fase "visible" del cursor parpadeante (segundos).
pub const BLINK_ON: f32 = 0.53;
/// Duración de la fase "oculto" del cursor parpadeante (segundos).
pub const BLINK_OFF: f32 = 0.53;
/// Retardo inicial antes de que el backspace mantenido comience a repetirse.
pub const BACKSPACE_DELAY: f32 = 0.40;
/// Intervalo entre borrados consecutivos una vez en modo repeat.
pub const BACKSPACE_REPEAT: f32 = 0.05;
/// Máximo de entradas en el historial de undo.
pub const UNDO_LIMIT: usize = 100;

// ─── Tipos ────────────────────────────────────────────────────────────────────

/// Teclas especiales que `TextFieldState` entiende.
/// Son independientes de cualquier backend de ventana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ShiftArrowLeft,
    ShiftArrowRight,
    ShiftHome,
    ShiftEnd,
    CtrlArrowLeft,
    CtrlArrowRight,
    CtrlShiftArrowLeft,
    CtrlShiftArrowRight,
    Home,
    End,
    SelectAll, // Ctrl+A
    Copy,      // Ctrl+C
    Cut,       // Ctrl+X
    Paste,     // Ctrl+V — el llamador provee el texto mediante `clipboard_text`
    Undo,      // Ctrl+Z
    Redo,      // Ctrl+Y
    Enter,
    Escape,
    Tab,
}

/// Resultado de procesar una tecla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKeyResult {
    /// La tecla no fue reconocida o no tuvo efecto.
    Ignored,
    /// La tecla fue procesada (puede necesitar re-render).
    Handled,
    /// El usuario confirmó la entrada (Enter).
    Submit,
    /// El usuario canceló la entrada (Escape).
    Cancel,
    /// Ctrl+C/X — el llamador debe enviar este texto al portapapeles.
    CopyToClipboard(String),
}

// ─── Snapshot de undo ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Snapshot {
    buf: String,
    cursor_pos: usize,
    sel_anchor: Option<usize>,
}

// ─── TextFieldState ───────────────────────────────────────────────────────────

/// Estado completo de un campo de texto de una sola línea.
///
/// No contiene el buffer de texto — ese lo gestiona el llamador para que
/// sea fácil integrarlo en cualquier estructura de datos existente.
#[derive(Debug, Clone)]
pub struct TextFieldState {
    // ── Cursor ────────────────────────────────────────────────────────────
    /// Posición del cursor en bytes dentro del buffer.
    pub cursor_pos: usize,
    /// Si el cursor está actualmente visible (alterna con el timer de parpadeo).
    pub cursor_visible: bool,
    /// Ancla de selección. Cuando hay selección activa, el rango es
    /// `min(cursor_pos, sel_anchor)..max(cursor_pos, sel_anchor)`.
    /// `None` = sin selección.
    pub sel_anchor: Option<usize>,
    /// Si todo el texto está seleccionado (Ctrl+A). Atajo de conveniencia
    /// — también se refleja en `sel_anchor`.
    pub all_selected: bool,
    /// Si el campo tiene el foco.
    pub focused: bool,

    // ── Timers internos ───────────────────────────────────────────────────
    blink_timer: f32,
    backspace_held: bool,
    backspace_timer: f32,
    backspace_repeating: bool,

    // ── Historial de undo/redo ─────────────────────────────────────────────
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl TextFieldState {
    /// Crea un nuevo estado con valores predeterminados.
    pub fn new() -> Self {
        Self {
            cursor_pos: 0,
            cursor_visible: true,
            sel_anchor: None,
            all_selected: false,
            focused: false,
            blink_timer: 0.0,
            backspace_held: false,
            backspace_timer: 0.0,
            backspace_repeating: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    // ── Selección ─────────────────────────────────────────────────────────

    /// Devuelve el rango de selección normalizado `(start, end)` si hay texto seleccionado.
    pub fn selection(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.sel_anchor {
            if anchor != self.cursor_pos {
                let start = anchor.min(self.cursor_pos);
                let end = anchor.max(self.cursor_pos);
                return Some((start, end));
            }
        }
        None
    }

    /// Devuelve `true` si hay texto seleccionado (ya sea por Ctrl+A o por Shift+flecha).
    pub fn has_selection(&self) -> bool {
        self.all_selected || self.selection().is_some()
    }

    /// Borra la selección sin modificar el buffer.
    fn clear_selection(&mut self) {
        self.sel_anchor = None;
        self.all_selected = false;
    }

    /// Inicia o extiende una selección: fija el ancla si no hay ninguna,
    /// y mueve el cursor al nuevo `new_cursor`.
    fn extend_selection(&mut self, new_cursor: usize) {
        if self.sel_anchor.is_none() {
            self.sel_anchor = Some(self.cursor_pos);
        }
        self.all_selected = false;
        self.cursor_pos = new_cursor;
    }

    /// Borra el texto seleccionado del buffer y deja el cursor en el inicio de la selección.
    /// Devuelve `true` si se borró algo.
    fn delete_selection(&mut self, buf: &mut String) -> bool {
        if self.all_selected {
            if buf.is_empty() {
                return false;
            }
            buf.clear();
            self.cursor_pos = 0;
            self.all_selected = false;
            self.sel_anchor = None;
            return true;
        }
        if let Some((start, end)) = self.selection() {
            buf.drain(start..end);
            self.cursor_pos = start;
            self.sel_anchor = None;
            self.all_selected = false;
            return true;
        }
        false
    }

    /// Devuelve el texto actualmente seleccionado.
    pub fn selected_text<'a>(&self, buf: &'a str) -> &'a str {
        if self.all_selected {
            return buf;
        }
        if let Some((start, end)) = self.selection() {
            return &buf[start..end];
        }
        ""
    }

    // ── Foco ──────────────────────────────────────────────────────────────

    /// Marca el campo como enfocado y reinicia la animación del cursor.
    pub fn focus(&mut self) {
        self.focused = true;
        self.reset_blink();
    }

    /// Marca el campo como desenfocado y limpia el estado de animación.
    pub fn blur(&mut self) {
        self.focused = false;
        self.blink_timer = 0.0;
        self.cursor_visible = true;
        self.backspace_held = false;
        self.backspace_timer = 0.0;
        self.backspace_repeating = false;
        self.all_selected = false;
        self.sel_anchor = None;
    }

    // ── Tick (cada frame) ─────────────────────────────────────────────────

    /// Avanza los timers internos. Debe llamarse **una vez por frame**.
    ///
    /// Devuelve `true` si la fase del cursor cambió (para pedir re-render).
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.focused {
            return false;
        }

        // Parpadeo
        self.blink_timer += dt;
        let phase = if self.cursor_visible {
            BLINK_ON
        } else {
            BLINK_OFF
        };
        if self.blink_timer >= phase {
            self.blink_timer -= phase;
            self.cursor_visible = !self.cursor_visible;
            return true;
        }
        false
    }

    /// Consulta si el backspace mantenido necesita disparar un borrado este frame.
    ///
    /// Llama a este método **después de `tick()`** y antes de procesar eventos
    /// para el caso de tener un polling de teclas (modo inmediato). Si el
    /// `is_backspace_held` es `true` y el timer expiró, devuelve `true`.
    pub fn poll_backspace_repeat(&mut self, dt: f32, is_held: bool) -> bool {
        if !is_held {
            self.backspace_held = false;
            self.backspace_timer = 0.0;
            self.backspace_repeating = false;
            return false;
        }
        if !self.backspace_held {
            return false;
        }
        self.backspace_timer += dt;
        if !self.backspace_repeating {
            if self.backspace_timer >= BACKSPACE_DELAY {
                self.backspace_repeating = true;
                self.backspace_timer = 0.0;
                true
            } else {
                false
            }
        } else if self.backspace_timer >= BACKSPACE_REPEAT {
            self.backspace_timer -= BACKSPACE_REPEAT;
            true
        } else {
            false
        }
    }

    // ── Posicionamiento por click ─────────────────────────────────────────

    /// Posiciona el cursor en el carácter más cercano a la coordenada `click_x`
    /// dentro del campo. `field_x` es el borde izquierdo del área de texto (sin padding),
    /// `char_width` es el ancho monoespaciado de cada carácter.
    ///
    /// Si `extend_sel` es `true` (Shift+click), extiende la selección en lugar de limpiarla.
    pub fn click_at(
        &mut self,
        buf: &str,
        click_x: f32,
        field_x: f32,
        char_width: f32,
        scroll_offset: f32,
        extend_sel: bool,
    ) {
        let relative_x = click_x - field_x + scroll_offset;
        let char_index = ((relative_x / char_width).round() as usize).min(buf.chars().count());
        // Convertir índice de carácter a byte-index
        let byte_pos = char_to_byte_index(buf, char_index);

        if extend_sel {
            self.extend_selection(byte_pos);
        } else {
            self.clear_selection();
            self.cursor_pos = byte_pos;
        }
        self.reset_blink();
    }

    // ── Entrada de carácter ───────────────────────────────────────────────

    /// Procesa un carácter Unicode tipado.
    ///
    /// Si hay selección, la reemplaza con el carácter.
    pub fn on_char(&mut self, c: char, buf: &mut String) {
        if c.is_control() {
            return;
        }
        self.push_undo(buf);
        if self.has_selection() {
            self.delete_selection(buf);
        }
        buf.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
        self.reset_blink();
        self.redo_stack.clear();
    }

    // ── Entrada de tecla especial ─────────────────────────────────────────

    /// Procesa una [`FieldKey`].
    ///
    /// Para `FieldKey::Paste`, pasa el texto del portapapeles en `clipboard_text`.
    /// Devuelve un [`FieldKeyResult`] que indica qué ocurrió.
    pub fn on_key(&mut self, key: FieldKey, buf: &mut String) -> FieldKeyResult {
        self.on_key_with_clipboard(key, buf, None)
    }

    /// Igual que [`on_key`] pero permite pasar el texto del portapapeles para `FieldKey::Paste`.
    pub fn on_key_with_clipboard(
        &mut self,
        key: FieldKey,
        buf: &mut String,
        clipboard: Option<&str>,
    ) -> FieldKeyResult {
        match key {
            // ── Selección completa ────────────────────────────────────────
            FieldKey::SelectAll => {
                if !buf.is_empty() {
                    self.all_selected = true;
                    self.sel_anchor = Some(0);
                    self.cursor_pos = buf.len();
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    FieldKeyResult::Ignored
                }
            }

            // ── Backspace ─────────────────────────────────────────────────
            FieldKey::Backspace => {
                self.push_undo(buf);
                let changed = if self.has_selection() {
                    self.delete_selection(buf)
                } else {
                    self.do_backspace(buf)
                };
                if !self.backspace_held {
                    self.backspace_held = true;
                    self.backspace_timer = 0.0;
                    self.backspace_repeating = false;
                }
                if changed {
                    self.redo_stack.clear();
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    self.undo_stack.pop(); // no hubo cambio, revertir push
                    FieldKeyResult::Ignored
                }
            }

            // ── Delete ────────────────────────────────────────────────────
            FieldKey::Delete => {
                self.push_undo(buf);
                let changed = if self.has_selection() {
                    self.delete_selection(buf)
                } else if self.cursor_pos < buf.len() {
                    buf.remove(self.cursor_pos);
                    true
                } else {
                    false
                };
                if changed {
                    self.redo_stack.clear();
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    self.undo_stack.pop();
                    FieldKeyResult::Ignored
                }
            }

            // ── Navegación sin selección ──────────────────────────────────
            FieldKey::ArrowLeft => {
                if self.has_selection() {
                    // Colapsa la selección al inicio
                    let start = self
                        .selection()
                        .map(|(s, _)| s)
                        .unwrap_or(if self.all_selected {
                            0
                        } else {
                            self.cursor_pos
                        });
                    self.cursor_pos = start;
                    self.clear_selection();
                } else if self.cursor_pos > 0 {
                    self.cursor_pos = prev_char_boundary(buf, self.cursor_pos);
                } else {
                    return FieldKeyResult::Ignored;
                }
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::ArrowRight => {
                if self.has_selection() {
                    // Colapsa la selección al final
                    let end = self
                        .selection()
                        .map(|(_, e)| e)
                        .unwrap_or(if self.all_selected {
                            buf.len()
                        } else {
                            self.cursor_pos
                        });
                    self.cursor_pos = end;
                    self.clear_selection();
                } else if self.cursor_pos < buf.len() {
                    self.cursor_pos = next_char_boundary(buf, self.cursor_pos);
                } else {
                    return FieldKeyResult::Ignored;
                }
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::CtrlArrowLeft => {
                self.clear_selection();
                self.cursor_pos = word_start_before(buf, self.cursor_pos);
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::CtrlArrowRight => {
                self.clear_selection();
                self.cursor_pos = word_end_after(buf, self.cursor_pos);
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::Home => {
                self.cursor_pos = 0;
                self.clear_selection();
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::End => {
                self.cursor_pos = buf.len();
                self.clear_selection();
                self.reset_blink();
                FieldKeyResult::Handled
            }

            // ── Navegación con selección (Shift) ──────────────────────────
            FieldKey::ShiftArrowLeft => {
                if self.cursor_pos > 0 {
                    let new_pos = prev_char_boundary(buf, self.cursor_pos);
                    self.extend_selection(new_pos);
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    FieldKeyResult::Ignored
                }
            }

            FieldKey::ShiftArrowRight => {
                if self.cursor_pos < buf.len() {
                    let new_pos = next_char_boundary(buf, self.cursor_pos);
                    self.extend_selection(new_pos);
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    FieldKeyResult::Ignored
                }
            }

            FieldKey::ShiftHome => {
                self.extend_selection(0);
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::ShiftEnd => {
                self.extend_selection(buf.len());
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::CtrlShiftArrowLeft => {
                let new_pos = word_start_before(buf, self.cursor_pos);
                self.extend_selection(new_pos);
                self.reset_blink();
                FieldKeyResult::Handled
            }

            FieldKey::CtrlShiftArrowRight => {
                let new_pos = word_end_after(buf, self.cursor_pos);
                self.extend_selection(new_pos);
                self.reset_blink();
                FieldKeyResult::Handled
            }

            // ── Portapapeles ──────────────────────────────────────────────
            FieldKey::Copy => {
                let text = self.selected_text(buf).to_string();
                if text.is_empty() {
                    FieldKeyResult::Ignored
                } else {
                    FieldKeyResult::CopyToClipboard(text)
                }
            }

            FieldKey::Cut => {
                let text = self.selected_text(buf).to_string();
                if text.is_empty() {
                    return FieldKeyResult::Ignored;
                }
                self.push_undo(buf);
                self.delete_selection(buf);
                self.redo_stack.clear();
                self.reset_blink();
                FieldKeyResult::CopyToClipboard(text)
            }

            FieldKey::Paste => {
                if let Some(text) = clipboard {
                    if text.is_empty() {
                        return FieldKeyResult::Ignored;
                    }
                    self.push_undo(buf);
                    if self.has_selection() {
                        self.delete_selection(buf);
                    }
                    buf.insert_str(self.cursor_pos, text);
                    self.cursor_pos += text.len();
                    self.redo_stack.clear();
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    FieldKeyResult::Ignored
                }
            }

            // ── Undo / Redo ───────────────────────────────────────────────
            FieldKey::Undo => {
                if let Some(snap) = self.undo_stack.pop() {
                    let cur_snap = Snapshot {
                        buf: buf.clone(),
                        cursor_pos: self.cursor_pos,
                        sel_anchor: self.sel_anchor,
                    };
                    self.redo_stack.push(cur_snap);
                    *buf = snap.buf;
                    self.cursor_pos = snap.cursor_pos;
                    self.sel_anchor = snap.sel_anchor;
                    self.all_selected = false;
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    FieldKeyResult::Ignored
                }
            }

            FieldKey::Redo => {
                if let Some(snap) = self.redo_stack.pop() {
                    let cur_snap = Snapshot {
                        buf: buf.clone(),
                        cursor_pos: self.cursor_pos,
                        sel_anchor: self.sel_anchor,
                    };
                    self.undo_stack.push(cur_snap);
                    *buf = snap.buf;
                    self.cursor_pos = snap.cursor_pos;
                    self.sel_anchor = snap.sel_anchor;
                    self.all_selected = false;
                    self.reset_blink();
                    FieldKeyResult::Handled
                } else {
                    FieldKeyResult::Ignored
                }
            }

            FieldKey::Enter => FieldKeyResult::Submit,
            FieldKey::Escape => FieldKeyResult::Cancel,

            FieldKey::Tab => FieldKeyResult::Ignored,
        }
    }

    /// Notifica que la tecla Backspace fue **soltada** (para modo de eventos).
    pub fn on_backspace_released(&mut self) {
        self.backspace_held = false;
        self.backspace_timer = 0.0;
        self.backspace_repeating = false;
    }

    // ── Undo helpers ─────────────────────────────────────────────────────

    fn push_undo(&mut self, buf: &str) {
        if self.undo_stack.len() >= UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(Snapshot {
            buf: buf.to_string(),
            cursor_pos: self.cursor_pos,
            sel_anchor: self.sel_anchor,
        });
    }

    // ── Helpers internos ──────────────────────────────────────────────────

    fn reset_blink(&mut self) {
        self.blink_timer = 0.0;
        self.cursor_visible = true;
    }

    fn do_backspace(&mut self, buf: &mut String) -> bool {
        if self.cursor_pos > 0 {
            let prev = prev_char_boundary(buf, self.cursor_pos);
            buf.drain(prev..self.cursor_pos);
            self.cursor_pos = prev;
            true
        } else {
            false
        }
    }
}

impl Default for TextFieldState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers de navegación Unicode ───────────────────────────────────────────

/// Convierte un índice de carácter (no byte) a byte-index.
pub fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Devuelve el byte-index del límite de carácter previo antes de `pos`.
fn prev_char_boundary(s: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut p = pos - 1;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Devuelve el byte-index del límite de carácter siguiente después de `pos`.
fn next_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos + 1;
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}

/// Salta hacia atrás hasta el inicio de la palabra anterior.
/// Salta espacios, luego el bloque no-espacio.
fn word_start_before(s: &str, pos: usize) -> usize {
    let bytes = s.as_bytes();
    let mut p = pos;
    // Skip trailing spaces
    while p > 0 && bytes[p - 1] == b' ' {
        p -= 1;
    }
    // Skip word chars
    while p > 0 && bytes[p - 1] != b' ' {
        p -= 1;
    }
    p
}

/// Salta hacia adelante hasta el final de la siguiente palabra.
fn word_end_after(s: &str, pos: usize) -> usize {
    let bytes = s.as_bytes();
    let len = s.len();
    let mut p = pos;
    // Skip leading spaces
    while p < len && bytes[p] == b' ' {
        p += 1;
    }
    // Skip word chars
    while p < len && bytes[p] != b' ' {
        p += 1;
    }
    p
}
