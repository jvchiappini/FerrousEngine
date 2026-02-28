use crate::{layout::Rect, RenderCommand, Widget};

/// Slider widget for selecting a value in [0.0, 1.0].
///
/// Produces a background quad and a filled thumb. Keeps internal state
/// (current value, dragging) and handles hit testing on the thumb.
#[derive(Debug, Clone)]
pub struct Slider {
    pub rect: [f32; 4], // x, y, width, height
    pub value: f32,
    pub dragging: bool,
    pub thumb_color: [f32; 4],
    pub track_color: [f32; 4],
}

impl Slider {
    /// Create a new slider positioned at `(x,y)` with given width/height and
    /// initial normalized value.
    pub fn new(x: f32, y: f32, w: f32, h: f32, value: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            value: value.clamp(0.0, 1.0),
            dragging: false,
            thumb_color: [0.8, 0.8, 0.8, 1.0],
            track_color: [0.2, 0.2, 0.2, 1.0],
        }
    }

    /// Hit test the thumb (not whole track).
    pub fn thumb_hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        let thumb_w = self.rect[2] * 0.1;
        let tx = self.rect[0] + (self.rect[2] - thumb_w) * self.value;
        x >= tx && x <= tx + thumb_w && y >= self.rect[1] && y <= self.rect[1] + self.rect[3]
    }

    /// Update value based on x coordinate (called while dragging).
    pub fn update_value(&mut self, mx: f64) {
        let x = mx as f32;
        let rel = (x - self.rect[0]) / (self.rect[2] - 0.0);
        self.value = rel.clamp(0.0, 1.0);
    }

    /// Convenience draw method pushing to `GuiBatch`.
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch) {
        // track
        batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color: self.track_color,
            radii: [0.0; 4],
        });
        // thumb
        let thumb_w = self.rect[2] * 0.1;
        let tx = self.rect[0] + (self.rect[2] - thumb_w) * self.value;
        batch.push(crate::renderer::GuiQuad {
            pos: [tx, self.rect[1]],
            size: [thumb_w, self.rect[3]],
            color: self.thumb_color,
            radii: [0.0; 4],
        });
    }
}

impl Widget for Slider {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        // track
        cmds.push(RenderCommand::Quad {
            rect: Rect { x: self.rect[0], y: self.rect[1], width: self.rect[2], height: self.rect[3] },
            color: self.track_color,
            radii: [0.0; 4],
        });
        // thumb
        let thumb_w = self.rect[2] * 0.1;
        let tx = self.rect[0] + (self.rect[2] - thumb_w) * self.value;
        cmds.push(RenderCommand::Quad {
            rect: Rect { x: tx, y: self.rect[1], width: thumb_w, height: self.rect[3] },
            color: self.thumb_color,
            radii: [0.0; 4],
        });
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        // we consider the whole control as hittable (for focusing purposes)
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    fn mouse_move(&mut self, mx: f64, _my: f64) {
        if self.dragging {
            self.update_value(mx);
        }
    }

    fn mouse_input(&mut self, mx: f64, _my: f64, pressed: bool) {
        if pressed {
            if self.thumb_hit(mx, _my) {
                self.dragging = true;
            }
        } else {
            self.dragging = false;
        }
    }
}
