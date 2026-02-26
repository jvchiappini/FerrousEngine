use crate::{layout::Rect, RenderCommand, Widget};

/// Simple rectangular button widget used for interactive UIs.
///
/// Tracks hover/press state and produces a coloured quad when collected.
#[derive(Debug, Clone)]
pub struct Button {
    pub rect: [f32; 4], // x, y, width, height
    pub hovered: bool,
    pub pressed: bool,
    /// base colour (will be tinted when hovered/pressed)
    pub color: [f32; 4],
}

impl Button {
    /// Create a new button at given position/size.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            hovered: false,
            pressed: false,
            color: [0.2, 0.2, 0.8, 1.0],
        }
    }

    /// Hit test against mouse coordinates (window space).
    pub fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    /// Convenience drawing method that pushes directly into a `GuiBatch`.
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch) {
        let color = if self.pressed {
            [0.8, 0.2, 0.2, 1.0]
        } else if self.hovered {
            [0.2, 0.8, 0.2, 1.0]
        } else {
            self.color
        };
        batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color,
        });
    }
}

impl Widget for Button {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        let color = if self.pressed {
            [0.8, 0.2, 0.2, 1.0]
        } else if self.hovered {
            [0.2, 0.8, 0.2, 1.0]
        } else {
            self.color
        };
        let rect = Rect {
            x: self.rect[0],
            y: self.rect[1],
            width: self.rect[2],
            height: self.rect[3],
        };
        cmds.push(RenderCommand::Quad { rect, color });
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        self.hit(mx, my)
    }

    fn mouse_move(&mut self, mx: f64, my: f64) {
        self.hovered = self.hit(mx, my);
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            // press only when the cursor is over us
            self.pressed = self.hit(mx, my);
        } else {
            // release on any mouse-up
            self.pressed = false;
        }
    }
}
