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
    /// Hue/saturation rectangle.  Hue varies left→right, saturation top→bottom.
    Rect,
    /// Triangular picker; usable area is the lower-left right triangle of
    /// the bounding rect.  Hue runs along the base, saturation decreases
    /// towards the top corner.
    Triangle,
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
    /// If the user has performed a pick interaction, store the last
    /// normalized coordinates.  This allows accurate placement of the
    /// selection indicator even when the colour value alone is
    /// ambiguous (e.g. hue==0/1 at the edges of the rectangle).
    pub pick_pos: Option<[f32; 2]>,
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
            pick_pos: None,
        }
    }

    /// Set the picker's colour explicitly.
    pub fn with_colour(mut self, c: [f32; 4]) -> Self {
        self.colour = c;
        // colour was overridden externally, forget previous pick coords
        self.pick_pos = None;
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
            match self.shape {
                PickerShape::Circle => {
                    let dx = nx - 0.5;
                    let dy = ny - 0.5;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist > 0.5 {
                        return; // outside circle
                    }
                    let angle = dy.atan2(dx);
                    let hue = (angle / (2.0 * std::f32::consts::PI) + 1.0) % 1.0;
                    let sat = dist / 0.5;
                    self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
                    self.pick_pos = Some([nx, ny]);
                }
                PickerShape::Rect => {
                    // ignore outside bounds
                    if nx < 0.0 || nx > 1.0 || ny < 0.0 || ny > 1.0 {
                        return;
                    }
                    let hue = nx;
                    let sat = 1.0 - ny;
                    self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
                    self.pick_pos = Some([nx, ny]);
                }
                PickerShape::Triangle => {
                    if nx < 0.0 || ny < 0.0 || nx + ny > 1.0 {
                        return;
                    }
                    let sat = 1.0 - ny;
                    let hue = if sat == 0.0 { 0.0 } else { nx / (1.0 - ny) };
                    self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
                    self.pick_pos = Some([nx, ny]);
                }
                PickerShape::Custom(_) => {
                    // leave unchanged
                }
            }
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
            PickerShape::Rect => {
                x >= self.rect[0]
                    && x <= self.rect[0] + self.rect[2]
                    && y >= self.rect[1]
                    && y <= self.rect[1] + self.rect[3]
            }
            PickerShape::Triangle => {
                let u = (x - self.rect[0]) / self.rect[2];
                let v = (y - self.rect[1]) / self.rect[3];
                u >= 0.0 && v >= 0.0 && u + v <= 1.0
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
                // draw a simple hue/sat wheel by stamping a grid of coloured
                // quads inside the circle.  this gives a basic gradient that
                // makes the control usable without requiring a texture.
                // push a single quad flagged as a colour wheel; the GPU
                // shader will render the full gradient smoothly.
                batch.push(crate::renderer::GuiQuad {
                    pos: [self.rect[0], self.rect[1]],
                    size: [self.rect[2], self.rect[3]],
                    color: self.colour,
                    radii: [self.rect[2].min(self.rect[3]) * 0.5; 4],
                    flags: 1,
                });
                // draw selection indicator at computed position
                let (px, py) = if let Some([nx, ny]) = self.pick_pos {
                    (self.rect[0] + nx * self.rect[2], self.rect[1] + ny * self.rect[3])
                } else {
                    color_to_point(self.colour, self.rect, &self.shape)
                };
                batch.push(crate::renderer::GuiQuad {
                    pos: [px - 4.0, py - 4.0],
                    size: [8.0, 8.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    radii: [4.0; 4],
                    flags: 0,
                });
            }
            PickerShape::Rect => {
                // rectangular hue/sat swatch; flag bit1 distinguishes from
                // circular wheel.  The shader will interpret the flags and
                // generate the appropriate gradient.
                batch.push(crate::renderer::GuiQuad {
                    pos: [self.rect[0], self.rect[1]],
                    size: [self.rect[2], self.rect[3]],
                    color: self.colour,
                    radii: [0.0; 4],
                    flags: 2,
                });
                let (px, py) = if let Some([nx, ny]) = self.pick_pos {
                    (self.rect[0] + nx * self.rect[2], self.rect[1] + ny * self.rect[3])
                } else {
                    color_to_point(self.colour, self.rect, &self.shape)
                };
                batch.push(crate::renderer::GuiQuad {
                    pos: [px - 4.0, py - 4.0],
                    size: [8.0, 8.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    radii: [4.0; 4],
                    flags: 0,
                });
            }
            PickerShape::Triangle => {
                // triangular hue/sat region; use flag bit2.
                batch.push(crate::renderer::GuiQuad {
                    pos: [self.rect[0], self.rect[1]],
                    size: [self.rect[2], self.rect[3]],
                    color: self.colour,
                    radii: [0.0; 4],
                    flags: 3,
                });
                let (px, py) = if let Some([nx, ny]) = self.pick_pos {
                    (self.rect[0] + nx * self.rect[2], self.rect[1] + ny * self.rect[3])
                } else {
                    color_to_point(self.colour, self.rect, &self.shape)
                };
                batch.push(crate::renderer::GuiQuad {
                    pos: [px - 4.0, py - 4.0],
                    size: [8.0, 8.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    radii: [4.0; 4],
                    flags: 0,
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
                // push a single command requesting a colour wheel; the
                // renderer will handle the actual gradient in the shader.
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: self.rect[0],
                        y: self.rect[1],
                        width: self.rect[2],
                        height: self.rect[3],
                    },
                    color: self.colour, // alpha may be used by shader
                    radii: [self.rect[2].min(self.rect[3]) * 0.5; 4],
                    flags: 1, // bit0 = colour wheel
                });
                // draw indicator as extra quad
                let (px, py) = if let Some([nx, ny]) = self.pick_pos {
                    (self.rect[0] + nx * self.rect[2], self.rect[1] + ny * self.rect[3])
                } else {
                    color_to_point(self.colour, self.rect, &self.shape)
                };
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: px - 4.0,
                        y: py - 4.0,
                        width: 8.0,
                        height: 8.0,
                    },
                    color: [1.0, 1.0, 1.0, 1.0],
                    radii: [4.0; 4],
                    flags: 0,
                });
            }
            PickerShape::Custom(f) => {
                f(self, cmds);
            }
            PickerShape::Rect => {
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: self.rect[0],
                        y: self.rect[1],
                        width: self.rect[2],
                        height: self.rect[3],
                    },
                    color: self.colour,
                    radii: [0.0; 4],
                    flags: 2,
                });
                let (px, py) = if let Some([nx, ny]) = self.pick_pos {
                    (self.rect[0] + nx * self.rect[2], self.rect[1] + ny * self.rect[3])
                } else {
                    color_to_point(self.colour, self.rect, &self.shape)
                };
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: px - 4.0,
                        y: py - 4.0,
                        width: 8.0,
                        height: 8.0,
                    },
                    color: [1.0, 1.0, 1.0, 1.0],
                    radii: [4.0; 4],
                    flags: 0,
                });
            }
            PickerShape::Triangle => {
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: self.rect[0],
                        y: self.rect[1],
                        width: self.rect[2],
                        height: self.rect[3],
                    },
                    color: self.colour,
                    radii: [0.0; 4],
                    flags: 3,
                });
                let (px, py) = if let Some([nx, ny]) = self.pick_pos {
                    (self.rect[0] + nx * self.rect[2], self.rect[1] + ny * self.rect[3])
                } else {
                    color_to_point(self.colour, self.rect, &self.shape)
                };
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: px - 4.0,
                        y: py - 4.0,
                        width: 8.0,
                        height: 8.0,
                    },
                    color: [1.0, 1.0, 1.0, 1.0],
                    radii: [4.0; 4],
                    flags: 0,
                });
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

/// Convert an RGBA colour (assumed to originate from the default wheel
/// shader) back into a point in window space where the selection indicator
/// should be drawn.  This mirrors the HSV conversion used by the shader.
/// Given a colour value produced by the default `on_pick`/shader logic,
/// return the corresponding indicator position in window space.  The
/// behaviour must match `default_pick` for each supported shape.
// helper: extract hue (0..1) and saturation (0..1) from an RGB colour
// produced by our wheel/rect/triangle shader.  The algorithm is the
// same as the one used in `default_pick`, allowing the inverse mapping
// to remain consistent across shapes.
fn rgb_to_hs(col: [f32; 4]) -> (f32, f32) {
    let r = col[0];
    let g = col[1];
    let b = col[2];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let mut hue = if d == 0.0 {
        0.0
    } else {
        let mut h = if max == r {
            (g - b) / d
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        if h < 0.0 {
            h += 6.0;
        }
        (h / 6.0).fract()
    };
    let mut sat = if max == 0.0 { 0.0 } else { d / max };
    sat = sat.clamp(0.0, 1.0);
    (hue, sat)
}

fn color_to_point(col: [f32; 4], rect: [f32; 4], shape: &PickerShape) -> (f32, f32) {
    let (hue, sat) = rgb_to_hs(col);
    match shape {
        PickerShape::Circle => {
            let angle = hue * 2.0 * std::f32::consts::PI;
            let dist = sat * 0.5;
            let cx = rect[0] + rect[2] * 0.5;
            let cy = rect[1] + rect[3] * 0.5;
            let px = cx + dist * angle.cos() * rect[2];
            let py = cy + dist * angle.sin() * rect[3];
            (px, py)
        }
        PickerShape::Rect => {
            // mapping used by default_pick: hue = nx, sat = 1 - ny
            let nx = hue;
            let ny = 1.0 - sat;
            let x = rect[0] + nx * rect[2];
            let y = rect[1] + ny * rect[3];
            (x, y)
        }
        PickerShape::Triangle => {
            // same base formula as earlier, but using hsv extraction helper
            let ny = 1.0 - sat;
            let nx = if (1.0 - ny).abs() < std::f32::EPSILON {
                0.0
            } else {
                hue * (1.0 - ny)
            };
            let x = rect[0] + nx * rect[2];
            let y = rect[1] + ny * rect[3];
            (x, y)
        }
        PickerShape::Custom(_) => (rect[0], rect[1]),
    }
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
    fn default_pick_rect() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.shape = PickerShape::Rect;
        // top-left corner corresponds to hue=0, sat=1 -> red
        cp.default_pick(0.0, 0.0);
        assert!(cp.colour[0] > 0.9 && cp.colour[1] < 0.1 && cp.colour[2] < 0.1);
        // bottom-right corner should be hue=1, sat=0 -> white
        cp.default_pick(1.0, 1.0);
        assert_eq!(cp.colour, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn default_pick_triangle() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.shape = PickerShape::Triangle;
        // base middle should give hue=0.5, sat=1 (cyanish)
        cp.default_pick(0.5, 0.0);
        assert!(cp.colour[1] > 0.5, "expected green component to be large");
        // picking at the top corner (nx=0, ny=1) yields zero saturation,
        // which produces white; verify we actually compute that case.
        cp.default_pick(0.0, 1.0);
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

    #[test]
    fn hit_rect() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 50.0);
        cp.shape = PickerShape::Rect;
        // centre should hit
        assert!(cp.hit(50.0, 25.0));
        // outside on the left
        assert!(!cp.hit(-1.0, 25.0));
    }

    #[test]
    fn hit_triangle() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.shape = PickerShape::Triangle;
        // point in lower-left half should hit
        assert!(cp.hit(25.0, 25.0));
        // point near top-right outside the triangular region
        assert!(!cp.hit(90.0, 90.0));
    }

    #[test]
    fn color_to_point_rect_and_triangle() {
        let rect = [0.0, 0.0, 100.0, 100.0];
        // for rect we actually generate a colour via default_pick to
        // ensure it's realistic.  pick at normalized coords (0.25,0.3).
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.shape = PickerShape::Rect;
        cp.default_pick(0.25, 0.3);
        let p = color_to_point(cp.colour, rect, &PickerShape::Rect);
        assert!((p.0 - 0.25 * rect[2]).abs() < 0.5);
        // for triangle with hue=.5 sat=1 we expect point near base middle
        // triangle case: sample at (nx,ny)=(0.5,0.1)
        let mut cp2 = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp2.shape = PickerShape::Triangle;
        cp2.default_pick(0.5, 0.1);
        let p2 = color_to_point(cp2.colour, rect, &PickerShape::Triangle);
        // expected roughly at nx=0.5, ny=0.1
        assert!((p2.0 - 50.0).abs() < 5.0);
        assert!((p2.1 - 10.0).abs() < 5.0);
    }

    // round‑trip test for triangle mapping: pick some sample points and
    // verify the indicator computation returns the original coordinates.
    #[test]
    fn triangle_round_trip() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.shape = PickerShape::Triangle;
        // sample a few barycentric points inside the triangle
        for &(nx, ny) in &[ (0.1, 0.1), (0.3, 0.2), (0.0, 0.0), (0.5, 0.4) ] {
            assert!(nx + ny <= 1.0);
            let mut t = cp.clone();
            t.default_pick(nx, ny);
            let (px, py) = color_to_point(t.colour, t.rect, &t.shape);
            let rx = (px - t.rect[0]) / t.rect[2];
            let ry = (py - t.rect[1]) / t.rect[3];
            if (rx - nx).abs() >= 0.02 || (ry - ny).abs() >= 0.02 {
                eprintln!("round-trip failed for ({},{}) -> ({},{}) colour={:?}",
                    nx, ny, rx, ry, t.colour);
            }
            assert!((rx - nx).abs() < 0.02, "nx {} -> {}", nx, rx);
            assert!((ry - ny).abs() < 0.02, "ny {} -> {}", ny, ry);
        }
    }

    #[test]
    fn rect_round_trip() {
        let mut cp = ColorPicker::new(0.0, 0.0, 100.0, 100.0);
        cp.shape = PickerShape::Rect;
        for &(nx, ny) in &[ (0.0,0.0), (0.25,0.5), (0.75,0.2) /*skip boundary hue=1*/ ] {
            let mut t = cp.clone();
            t.default_pick(nx, ny);
            let (px, py) = color_to_point(t.colour, t.rect, &t.shape);
            let rx = (px - t.rect[0]) / t.rect[2];
            let ry = (py - t.rect[1]) / t.rect[3];
            assert!((rx - nx).abs() < 0.02, "{} -> {}", nx, rx);
            assert!((ry - ny).abs() < 0.02, "{} -> {}", ny, ry);
        }
    }
}
