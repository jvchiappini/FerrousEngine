use crate::{layout::Rect, RenderCommand, Widget};
use std::sync::Arc;

/// Shape used by the colour picker.  The built‑in `Circle` variant
/// renders a circular swatch and provides a corresponding hit test; the
/// `Custom` variant allows the caller to supply arbitrary render commands
/// (for example a fancy polygon or gradient) during `collect`.
#[derive(Clone)]
pub enum PickerShape {
    /// A circle inscribed in the widget's bounding rect.  The radius is
    /// computed as half of the smaller of width/height.
    Circle,
    /// Custom drawing routine.  The closure receives a reference to the
    /// picker and a mutable command list; it may push one or more
    /// `RenderCommand` values describing the desired appearance.  The
    /// callback is also responsible for computing whatever hit test logic
    /// is appropriate when the user clicks/dragging the widget (the
    /// default `hit` implementation simply uses the bounding box).
    Custom(Arc<dyn Fn(&ColorPicker, &mut Vec<RenderCommand>)>),
}

/// A very lightweight colour‑picker widget.
#[derive(Clone)]
///
///
/// By default it behaves as a simple hue/saturation wheel: clicking or
/// dragging inside the circle will update `colour` accordingly.  The
/// `on_pick` callback allows applications to override the mapping from a
/// pointer position to a colour value, and the `shape` field can be used
/// to change the rendered appearance (a rectangular swatch, for instance).
pub struct ColorPicker {
    pub rect: [f32; 4],
    /// currently selected colour (rgba components 0.0..1.0)
    pub colour: [f32; 4],
    pub hovered: bool,
    pub pressed: bool,
    pub shape: PickerShape,
    /// optional callback invoked when the user picks a colour.  the
    /// parameters are normalized coordinates within the widget's rect
    /// (0.0..1.0).  the closure may mutate `self.colour` however it
    /// wants.
    pub on_pick: Option<Arc<dyn Fn(&mut ColorPicker, f32, f32)>>,
}

impl ColorPicker {
    /// Create a new picker with the given bounding rectangle.  Initial
    /// colour is white and the shape defaults to `Circle`.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            colour: [1.0, 1.0, 1.0, 1.0],
            hovered: false,
            pressed: false,
            shape: PickerShape::Circle,
            on_pick: None,
        }
    }

    /// Set the picker's colour explicitly.
    pub fn with_colour(mut self, c: [f32; 4]) -> Self {
        self.colour = c;
        self
    }

    /// Change the shape used for drawing/hit‑testing.
    pub fn with_shape(mut self, shape: PickerShape) -> Self {
        self.shape = shape;
        self
    }

    /// Register a callback that will be invoked whenever the user clicks
    /// or drags inside the widget.  The coordinates passed to the callback
    /// are normalized to the `[0,1]` range within the bounding rect.
    pub fn on_pick<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut ColorPicker, f32, f32) + 'static,
    {
        self.on_pick = Some(Arc::new(f));
        self
    }

    /// Default mapping used when no `on_pick` callback is supplied.  It
    /// interprets the widget as a hue/saturation wheel: the angle from the
    /// centre gives the hue, the distance from the centre gives the
    /// saturation, value is kept at 1.0 and alpha is unchanged.
    fn default_pick(&mut self, nx: f32, ny: f32) {
        let dx = nx - 0.5;
        let dy = ny - 0.5;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > 0.5 {
            return; // outside the circle
        }
        let angle = dy.atan2(dx);
        let hue = (angle / (2.0 * std::f32::consts::PI) + 1.0) % 1.0;
        let sat = dist / 0.5;
        self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
    }

    /// Hit test using either the shape or a simple bounding-box fallback.
    pub fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        match &self.shape {
            PickerShape::Circle => {
                let cx = self.rect[0] + self.rect[2] * 0.5;
                let cy = self.rect[1] + self.rect[3] * 0.5;
                let rx = self.rect[2] * 0.5;
                let ry = self.rect[3] * 0.5;
                let dx = (x - cx) / rx;
                let dy = (y - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
            PickerShape::Custom(_) => {
                x >= self.rect[0]
                    && x <= self.rect[0] + self.rect[2]
                    && y >= self.rect[1]
                    && y <= self.rect[1] + self.rect[3]
            }
        }
    }

    /// Convenience helper to draw the widget directly into a `GuiBatch`.
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch) {
        match &self.shape {
            PickerShape::Circle => {
                let radius = self.rect[2].min(self.rect[3]) * 0.5;
                batch.push(crate::renderer::GuiQuad {
                    pos: [self.rect[0], self.rect[1]],
                    size: [self.rect[2], self.rect[3]],
                    color: self.colour,
                    radii: [radius; 4],
                });
            }
            PickerShape::Custom(f) => {
                // we cannot push to a GuiBatch in the custom
                // callback; users should instead implement their own
                // drawing by reusing the colour field and pushing quads
                // from their own code.  as a convenience we still call the
                // callback with an empty Vec and ignore the result.
                let mut cmds = Vec::new();
                f(self, &mut cmds);
                for cmd in cmds {
                    cmd.to_batches(batch, &mut crate::renderer::TextBatch::new(), None);
                }
            }
        }
    }
}

impl Widget for ColorPicker {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        match &self.shape {
            PickerShape::Circle => {
                let radius = self.rect[2].min(self.rect[3]) * 0.5;
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: self.rect[0],
                        y: self.rect[1],
                        width: self.rect[2],
                        height: self.rect[3],
                    },
                    color: self.colour,
                    radii: [radius; 4],
                });
            }
            PickerShape::Custom(f) => {
                f(self, cmds);
            }
        }
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        self.hit(mx, my)
    }

    fn mouse_move(&mut self, mx: f64, my: f64) {
        self.hovered = self.hit(mx, my);
        if self.pressed && self.hit(mx, my) {
            let nx = ((mx as f32) - self.rect[0]) / self.rect[2];
            let ny = ((my as f32) - self.rect[1]) / self.rect[3];
            if let Some(cb_arc) = &self.on_pick {
                let cb = cb_arc.clone();
                cb(self, nx, ny);
            } else {
                self.default_pick(nx, ny);
            }
        }
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            if self.hit(mx, my) {
                self.pressed = true;
                // treat initial press as a pick as well
                let nx = ((mx as f32) - self.rect[0]) / self.rect[2];
                let ny = ((my as f32) - self.rect[1]) / self.rect[3];
                if let Some(cb_arc) = &self.on_pick {
                    let cb = cb_arc.clone();
                    cb(self, nx, ny);
                } else {
                    self.default_pick(nx, ny);
                }
            }
        } else {
            self.pressed = false;
        }
    }
}

/// utility: convert HSV to RGBA.
fn hsv_to_rgba(h: f32, s: f32, v: f32, a: f32) -> [f32; 4] {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        5 => (v, p, q),
        _ => (0.0, 0.0, 0.0),
    };
    [r, g, b, a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pick_center() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.default_pick(0.5, 0.5);
        // centre should give zero saturation -> white
        assert_eq!(cp.colour, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn hit_circle() {
        let cp = ColorPicker::new(10.0, 10.0, 80.0, 80.0);
        // point at centre should hit
        assert!(cp.hit(50.0, 50.0));
        // outside bounding box
        assert!(!cp.hit(0.0, 0.0));
    }
}
