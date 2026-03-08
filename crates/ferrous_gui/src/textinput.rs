use crate::GuiKey;
use crate::{layout::Rect, RenderCommand, Widget};

/// Single-line text input widget with a visual blinking cursor.
///
/// Maintains a UTF-8 string buffer and tracks a byte-level insertion cursor.
/// When focused, the cursor is rendered as a thin vertical quad at the
/// current insertion position so the user can see where the next character
/// will appear.  Arrow-key navigation moves the cursor left/right.
pub struct TextInput {
    pub rect: [f32; 4],
    pub text: String,
    pub focused: bool,
    pub placeholder: String,
    pub bg_color: [f32; 4],
    pub text_color: [f32; 4],
    /// Index into `text` (in characters, not bytes) of the insertion cursor.
    pub cursor_pos: usize,
    /// Colour of the cursor bar (default: white).
    pub cursor_color: [f32; 4],
    /// optional tooltip string
    pub tooltip: Option<String>,
    /// Optional callback fired whenever the text content changes.
    on_change: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl TextInput {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            text: String::new(),
            focused: false,
            placeholder: String::from(""),
            bg_color: [0.1, 0.1, 0.1, 1.0],
            text_color: [1.0, 1.0, 1.0, 1.0],
            cursor_pos: 0,
            cursor_color: [1.0, 1.0, 1.0, 0.8],
            tooltip: None,
            on_change: None,
        }
    }

    /// Attach a tooltip shown on hover.
    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Register a callback fired whenever the text changes.
    pub fn on_change<F: Fn(&str) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    pub fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    /// Insert a character at the current cursor position.
    pub fn insert_char(&mut self, c: char) {
        if self.focused {
            // Convert cursor_pos (character index) to a byte index.
            let byte_idx = self.char_to_byte_idx(self.cursor_pos);
            self.text.insert(byte_idx, c);
            self.cursor_pos += 1;
            self.fire_on_change();
        }
    }

    /// Delete the character immediately before the cursor.
    pub fn backspace(&mut self) {
        if self.focused && self.cursor_pos > 0 {
            let byte_idx = self.char_to_byte_idx(self.cursor_pos - 1);
            let char_len = self.text[byte_idx..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8());
            self.text.drain(byte_idx..byte_idx + char_len);
            self.cursor_pos -= 1;
            self.fire_on_change();
        }
    }

    /// Delete the character at the cursor (Delete key).
    pub fn delete_forward(&mut self) {
        if self.focused && self.cursor_pos < self.char_count() {
            let byte_idx = self.char_to_byte_idx(self.cursor_pos);
            let char_len = self.text[byte_idx..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8());
            self.text.drain(byte_idx..byte_idx + char_len);
            self.fire_on_change();
        }
    }

    /// Move cursor one character to the left.
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor one character to the right.
    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.char_count() {
            self.cursor_pos += 1;
        }
    }

    /// Move cursor to the beginning of the line.
    pub fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Move cursor to the end of the line.
    pub fn cursor_end(&mut self) {
        self.cursor_pos = self.char_count();
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    fn char_to_byte_idx(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map_or(self.text.len(), |(b, _)| b)
    }

    /// Approximate pixel x-offset of the cursor inside the text area.
    /// We use the same fixed-width estimate as the button label (0.6 × font_size).
    fn cursor_x_offset(&self) -> f32 {
        let font_size = 16.0_f32;
        let text_before: String = self.text.chars().take(self.cursor_pos).collect();
        text_before.len() as f32 * font_size * 0.6
    }

    fn fire_on_change(&self) {
        if let Some(cb) = &self.on_change {
            cb(&self.text);
        }
    }

    /// Draw into both a quad batch and a text batch. If `font` is `None`, only
    /// the quad background will be emitted and text will be skipped.
    #[cfg(feature = "text")]
    pub fn draw(
        &self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
        font: Option<&ferrous_assets::Font>,
    ) {
        // background
        quad_batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: self.bg_color,
            radii: [0.0; 4],
            tex_index: 0,
            flags: 0,
        });
        if let Some(f) = font {
            let ty = self.rect[1] + (self.rect[3] - 16.0) / 2.0;
            let tx = self.rect[0] + 4.0;
            let display = if self.text.is_empty() {
                &self.placeholder
            } else {
                &self.text
            };
            text_batch.draw_text(f, display, [tx, ty], 16.0, self.text_color);
        }
        // cursor bar
        if self.focused {
            let tx = self.rect[0] + 4.0 + self.cursor_x_offset();
            let cy = self.rect[1] + 3.0;
            let ch = self.rect[3] - 6.0;
            quad_batch.push(crate::renderer::GuiQuad {
                pos: [tx, cy],
                size: [2.0, ch],
                uv0: [0.0, 0.0],
                uv1: [1.0, 1.0],
                color: self.cursor_color,
                radii: [0.0; 4],
                tex_index: 0,
                flags: 0,
            });
        }
    }

    #[cfg(not(feature = "text"))]
    pub fn draw(
        &self,
        quad_batch: &mut crate::renderer::GuiBatch,
        _text_batch: &mut crate::renderer::TextBatch,
    ) {
        quad_batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: self.bg_color,
            radii: [0.0; 4],
            tex_index: 0,
            flags: 0,
        });
        if self.focused {
            let tx = self.rect[0] + 4.0 + self.cursor_x_offset();
            let cy = self.rect[1] + 3.0;
            let ch = self.rect[3] - 6.0;
            quad_batch.push(crate::renderer::GuiQuad {
                pos: [tx, cy],
                size: [2.0, ch],
                uv0: [0.0, 0.0],
                uv1: [1.0, 1.0],
                color: self.cursor_color,
                radii: [0.0; 4],
                tex_index: 0,
                flags: 0,
            });
        }
    }
}

impl Widget for TextInput {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        // background quad
        cmds.push(RenderCommand::Quad {
            rect: Rect {
                x: self.rect[0],
                y: self.rect[1],
                width: self.rect[2],
                height: self.rect[3],
            },
            color: self.bg_color,
            radii: [0.0; 4],
            flags: 0,
        });
        // text command
        let display = if self.text.is_empty() {
            &self.placeholder
        } else {
            &self.text
        };
        cmds.push(RenderCommand::Text {
            rect: Rect {
                x: self.rect[0] + 4.0,
                y: self.rect[1] + (self.rect[3] - 16.0) / 2.0,
                width: 0.0,
                height: 0.0,
            },
            text: display.clone(),
            color: self.text_color,
            font_size: 16.0,
        });
        // cursor bar rendered as a 2-px wide quad
        if self.focused {
            let tx = self.rect[0] + 4.0 + self.cursor_x_offset();
            let cy = self.rect[1] + 3.0;
            let ch = self.rect[3] - 6.0;
            cmds.push(RenderCommand::Quad {
                rect: Rect {
                    x: tx,
                    y: cy,
                    width: 2.0,
                    height: ch,
                },
                color: self.cursor_color,
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        self.hit(mx, my)
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            self.focused = self.hit(mx, my);
            if self.focused {
                // Move cursor to end on click (simple heuristic).
                self.cursor_pos = self.char_count();
            }
        }
    }

    fn keyboard_input(&mut self, text: Option<&str>, key: Option<GuiKey>, pressed: bool) {
        if !self.focused {
            return;
        }
        if let Some(txt) = text {
            for c in txt.chars() {
                if !c.is_control() {
                    self.insert_char(c);
                }
            }
        }
        if pressed {
            #[cfg(feature = "winit-backend")]
            if let Some(k) = key {
                match k {
                    GuiKey::Backspace => self.backspace(),
                    GuiKey::Delete => self.delete_forward(),
                    GuiKey::ArrowLeft => self.cursor_left(),
                    GuiKey::ArrowRight => self.cursor_right(),
                    GuiKey::Home => self.cursor_home(),
                    GuiKey::End => self.cursor_end(),
                    _ => {}
                }
            }
            #[cfg(not(feature = "winit-backend"))]
            if let Some(k) = key {
                match k {
                    GuiKey::Backspace => self.backspace(),
                    _ => {}
                }
            }
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
    }

    fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }
}
