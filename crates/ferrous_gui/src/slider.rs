use crate::{layout::Rect, RenderCommand, Widget};

/// Slider widget for selecting a value within a configurable range.
///
/// The `value` field always stores the **real** value in `[min, max]`.
/// The default range is `0.0–1.0` preserving backwards compatibility.
///
/// ## Example
/// ```rust
/// use ferrous_gui::Slider;
/// let slider = Slider::new(10.0, 10.0, 200.0, 20.0, 50.0)
///     .range(0.0, 100.0);
/// assert_eq!(slider.value, 50.0);
/// ```
pub struct Slider {
    pub rect: [f32; 4], // x, y, width, height
    /// Current value in `[min, max]`.
    pub value: f32,
    pub dragging: bool,
    pub thumb_color: [f32; 4],
    pub track_color: [f32; 4],
    /// Minimum of the slider range (default 0.0).
    pub min: f32,
    /// Maximum of the slider range (default 1.0).
    pub max: f32,
    /// optional tooltip string
    pub tooltip: Option<String>,
    /// Optional callback fired whenever the value changes.
    on_change: Option<Box<dyn Fn(f32) + Send + Sync>>,
}

impl Slider {
    /// Create a new slider at `(x,y)` with given width/height and initial
    /// value.  The range defaults to `[0.0, 1.0]`; call `.range(min, max)`
    /// to change it.
    pub fn new(x: f32, y: f32, w: f32, h: f32, value: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            value: value.clamp(0.0, 1.0),
            dragging: false,
            thumb_color: [0.8, 0.8, 0.8, 1.0],
            track_color: [0.2, 0.2, 0.2, 1.0],
            min: 0.0,
            max: 1.0,
            tooltip: None,
            on_change: None,
        }
    }

    /// Set the value range. `value` is clamped to the new range.
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self.value = self.value.clamp(min, max);
        self
    }

    /// Set initial value (clamped to current range).
    pub fn with_value(mut self, v: f32) -> Self {
        self.value = v.clamp(self.min, self.max);
        self
    }

    /// Attach a tooltip shown on hover.
    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Register a callback that fires whenever the slider value changes.
    pub fn on_change<F: Fn(f32) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Returns the normalised `[0,1]` position of `value` within the range.
    fn normalized(&self) -> f32 {
        if (self.max - self.min).abs() < f32::EPSILON {
            0.0
        } else {
            (self.value - self.min) / (self.max - self.min)
        }
    }

    /// Hit test the thumb (not whole track).
    pub fn thumb_hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        let thumb_w = self.rect[2] * 0.1;
        let tx = self.rect[0] + (self.rect[2] - thumb_w) * self.normalized();
        x >= tx && x <= tx + thumb_w && y >= self.rect[1] && y <= self.rect[1] + self.rect[3]
    }

    /// Update value based on x coordinate (called while dragging).
    pub fn update_value(&mut self, mx: f64) {
        let x = mx as f32;
        let rel = (x - self.rect[0]) / self.rect[2];
        let new_val = (self.min + rel * (self.max - self.min)).clamp(self.min, self.max);
        if (new_val - self.value).abs() > f32::EPSILON {
            self.value = new_val;
            if let Some(cb) = &self.on_change {
                cb(self.value);
            }
        }
    }

    /// Convenience draw method pushing to `GuiBatch`.
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch) {
        // track
        batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color: self.track_color,
            radii: [0.0; 4],
            flags: 0,
        });
        // thumb
        let thumb_w = self.rect[2] * 0.1;
        let tx = self.rect[0] + (self.rect[2] - thumb_w) * self.normalized();
        batch.push(crate::renderer::GuiQuad {
            pos: [tx, self.rect[1]],
            size: [thumb_w, self.rect[3]],
            color: self.thumb_color,
            radii: [0.0; 4],
            flags: 0,
        });
    }
}

impl Widget for Slider {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        // track
        cmds.push(RenderCommand::Quad {
            rect: Rect {
                x: self.rect[0],
                y: self.rect[1],
                width: self.rect[2],
                height: self.rect[3],
            },
            color: self.track_color,
            radii: [0.0; 4],
            flags: 0,
        });
        // thumb
        let thumb_w = self.rect[2] * 0.1;
        let tx = self.rect[0] + (self.rect[2] - thumb_w) * self.normalized();
        cmds.push(RenderCommand::Quad {
            rect: Rect {
                x: tx,
                y: self.rect[1],
                width: thumb_w,
                height: self.rect[3],
            },
            color: self.thumb_color,
            radii: [0.0; 4],
            flags: 0,
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

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            if self.thumb_hit(mx, my) {
                self.dragging = true;
            }
        } else {
            self.dragging = false;
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
    }

    fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }
}
