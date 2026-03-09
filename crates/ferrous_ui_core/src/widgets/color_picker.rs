use crate::{Widget, RenderCommand, DrawContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2};

#[derive(Clone, PartialEq, Eq)]
pub enum PickerShape {
    Circle,
    Rect,
    Triangle,
}

pub struct ColorPicker<App> {
    pub colour: [f32; 4],
    pub pressed: bool,
    pub shape: PickerShape,
    pub pick_pos: Option<[f32; 2]>,
    pub binding: Option<std::sync::Arc<crate::Observable<[f32; 4]>>>,
    #[allow(clippy::type_complexity)]
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, [f32; 4]) + Send + Sync>>,
}

impl<App> Default for ColorPicker<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App> ColorPicker<App> {
    pub fn new() -> Self {
        Self {
            colour: [1.0, 1.0, 1.0, 1.0],
            pressed: false,
            shape: PickerShape::Circle,
            pick_pos: None,
            binding: None,
            on_change_cb: None,
        }
    }

    pub fn with_colour(mut self, c: [f32; 4]) -> Self {
        self.colour = c;
        self.pick_pos = None;
        self
    }

    pub fn with_shape(mut self, shape: PickerShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn with_binding(
        mut self,
        observable: std::sync::Arc<crate::Observable<[f32; 4]>>,
        node_id: crate::NodeId,
    ) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }

    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, [f32; 4]) + Send + Sync + 'static) -> Self {
        self.on_change_cb = Some(Box::new(f));
        self
    }

    fn update_value(&mut self, ctx: &mut EventContext<App>, nx: f32, ny: f32) {
        match self.shape {
            PickerShape::Circle => {
                let dx = nx - 0.5;
                let dy = ny - 0.5;
                let dist = (dx * dx + dy * dy).sqrt();
                let hue = (dy.atan2(dx) / (2.0 * std::f32::consts::PI) + 1.0) % 1.0;
                let sat = (dist / 0.5).min(1.0);
                let (cnx, cny) = if dist > 0.5 {
                    (0.5 + 0.5 * dx / dist, 0.5 + 0.5 * dy / dist)
                } else {
                    (nx, ny)
                };
                self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
                self.pick_pos = Some([cnx, cny]);
            }
            PickerShape::Rect => {
                let nx = nx.clamp(0.0, 1.0);
                let ny = ny.clamp(0.0, 1.0);
                let hue = nx;
                let sat = 1.0 - ny;
                self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
                self.pick_pos = Some([nx, ny]);
            }
            PickerShape::Triangle => {
                let mut cnx = nx.clamp(0.0, 1.0);
                let mut cny = ny.clamp(0.0, 1.0);
                if cnx + cny > 1.0 {
                    let over = (cnx + cny - 1.0) * 0.5;
                    cnx -= over;
                    cny -= over;
                }
                let sat = 1.0 - cny;
                let hue = if sat <= 0.0 {
                    0.0
                } else {
                    (cnx / (1.0 - cny)).clamp(0.0, 1.0)
                };
                self.colour = hsv_to_rgba(hue, sat, 1.0, self.colour[3]);
                self.pick_pos = Some([cnx, cny]);
            }
        }

        if let Some(o) = &self.binding {
            let dirty = o.set(self.colour);
            ctx.tree.reactivity.notify_change(dirty);
        }

        if let Some(cb) = &self.on_change_cb {
            cb(ctx, self.colour);
        }
    }
}

impl<App> Widget<App> for ColorPicker<App> {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let col = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.colour);
        let r = &ctx.rect;
        
        let flags = match self.shape {
            PickerShape::Circle => 1,
            PickerShape::Rect => 2,
            PickerShape::Triangle => 3,
        };

        let radii = match self.shape {
            PickerShape::Circle => [r.width.min(r.height) * 0.5; 4],
            _ => [0.0; 4],
        };

        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y, r.width, r.height),
            color: col,
            radii,
            flags,
        });

        let (px, py) = if let Some([nx, ny]) = self.pick_pos {
            (r.x + nx * r.width, r.y + ny * r.height)
        } else {
            let (px_abs, py_abs) = color_to_point(col, [r.x, r.y, r.width, r.height], &self.shape);
            (px_abs, py_abs)
        };

        cmds.push(RenderCommand::Quad {
            rect: Rect::new(px - 4.0, py - 4.0, 8.0, 8.0),
            color: [1.0, 1.0, 1.0, 1.0],
            radii: [4.0; 4],
            flags: 0,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(100.0, 100.0)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown { pos, .. } => {
                // Hit test
                let hit = match self.shape {
                    PickerShape::Circle => {
                        let rx = ctx.rect.width * 0.5;
                        let ry = ctx.rect.height * 0.5;
                        let cx = ctx.rect.x + rx;
                        let cy = ctx.rect.y + ry;
                        let dx = (pos.x - cx) / rx;
                        let dy = (pos.y - cy) / ry;
                        dx * dx + dy * dy <= 1.0
                    }
                    PickerShape::Rect => {
                        pos.x >= ctx.rect.x && pos.x <= ctx.rect.x + ctx.rect.width &&
                        pos.y >= ctx.rect.y && pos.y <= ctx.rect.y + ctx.rect.height
                    }
                    PickerShape::Triangle => {
                        let u = (pos.x - ctx.rect.x) / ctx.rect.width;
                        let v = (pos.y - ctx.rect.y) / ctx.rect.height;
                        u >= 0.0 && v >= 0.0 && u + v <= 1.0
                    }
                };

                if hit {
                    self.pressed = true;
                    let nx = (pos.x - ctx.rect.x) / ctx.rect.width;
                    let ny = (pos.y - ctx.rect.y) / ctx.rect.height;
                    self.update_value(ctx, nx, ny);
                    EventResponse::Redraw
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::MouseUp { .. } => {
                if self.pressed {
                    self.pressed = false;
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::MouseMove { pos } if self.pressed => {
                let nx = (pos.x - ctx.rect.x) / ctx.rect.width;
                let ny = (pos.y - ctx.rect.y) / ctx.rect.height;
                self.update_value(ctx, nx, ny);
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}

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

fn rgb_to_hs(col: [f32; 4]) -> (f32, f32) {
    let r = col[0];
    let g = col[1];
    let b = col[2];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let hue = if d == 0.0 {
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
            let nx = hue;
            let ny = 1.0 - sat;
            let x = rect[0] + nx * rect[2];
            let y = rect[1] + ny * rect[3];
            (x, y)
        }
        PickerShape::Triangle => {
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
    }
}
