use crate::{layout::Rect, RenderCommand, Widget};
use winit::keyboard::KeyCode;

/// Simple single-line text input widget.
///
/// Maintains a string buffer and focus state. Rendering shows a colored
/// background and the current text; no cursor or editing logic beyond
/// appending characters via `insert_char` and deleting with `backspace`.
#[derive(Debug, Clone)]
pub struct TextInput {
    pub rect: [f32; 4],
    pub text: String,
    pub focused: bool,
    pub placeholder: String,
    pub bg_color: [f32; 4],
    pub text_color: [f32; 4],
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
        }
    }

    pub fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    pub fn insert_char(&mut self, c: char) {
        if self.focused {
            self.text.push(c);
        }
    }

    pub fn backspace(&mut self) {
        if self.focused {
            self.text.pop();
        }
    }

    /// Draw into both a quad batch and a text batch. If `font` is `None`, only
    /// the quad background will be emitted and text will be skipped.
    pub fn draw(
        &self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
        font: Option<&ferrous_assets::font::Font>,
    ) {
        // background
        quad_batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color: self.bg_color,
            radii: [0.0; 4],
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
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        self.hit(mx, my)
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            // focus on press inside the rect, unfocus otherwise
            self.focused = self.hit(mx, my);
        }
    }

    fn keyboard_input(&mut self, text: Option<&str>, key: Option<KeyCode>, pressed: bool) {
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
            if let Some(k) = key {
                if k == KeyCode::Backspace {
                    self.backspace();
                }
            }
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
    }
}
