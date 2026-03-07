use crate::constraint::Constraint;
use crate::{layout::Rect, RenderCommand, Widget};

/// Checkbox widget with an optional text label to the right.
///
/// ## Example
/// ```rust
/// use ferrous_gui::Checkbox;
/// let mut cb = Checkbox::new(10.0, 10.0, "Enable shadows")
///     .checked(true)
///     .on_change(|v| println!("checked = {v}"));
/// ```
pub struct Checkbox {
    /// Bounding rect of the tick-box portion only `[x, y, w, h]`.
    pub rect: [f32; 4],
    /// Whether the checkbox is currently ticked.
    pub checked: bool,
    /// Optional text label rendered to the right of the box.
    pub label: Option<String>,
    /// Label font size (default 14.0).
    pub label_font_size: f32,
    /// Label colour.
    pub label_color: [f32; 4],
    /// Background colour of the box when unchecked.
    pub bg_color: [f32; 4],
    /// Colour of the fill when checked.
    pub check_color: [f32; 4],
    /// Corner radius of the box (default 0).
    pub radius: f32,
    /// Optional tooltip string.
    pub tooltip: Option<String>,
    /// Optional callback fired when the checked state changes.
    on_change: Option<Box<dyn Fn(bool) + Send + Sync>>,
    /// Optional reactive layout constraint.
    pub constraint: Option<Constraint>,
}

impl Checkbox {
    /// Create a new checkbox at `(x, y)`.  The box is 16 × 16 px by default.
    pub fn new(x: f32, y: f32, label: impl Into<String>) -> Self {
        Self {
            rect: [x, y, 16.0, 16.0],
            checked: false,
            label: Some(label.into()),
            label_font_size: 14.0,
            label_color: [1.0, 1.0, 1.0, 1.0],
            bg_color: [0.15, 0.15, 0.15, 1.0],
            check_color: [0.2, 0.6, 1.0, 1.0],
            radius: 3.0,
            tooltip: None,
            on_change: None,
            constraint: None,
        }
    }

    /// Set the box size (default 16 × 16).
    pub fn with_size(mut self, w: f32, h: f32) -> Self {
        self.rect[2] = w;
        self.rect[3] = h;
        self
    }

    /// Set initial checked state.
    pub fn checked(mut self, v: bool) -> Self {
        self.checked = v;
        self
    }

    /// Set the corner radius of the tick-box.
    pub fn with_radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }

    /// Attach a tooltip.
    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Register a callback fired when the checked state changes.
    pub fn on_change<F: Fn(bool) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Attach a reactive layout constraint.
    pub fn with_constraint(mut self, c: Constraint) -> Self {
        self.constraint = Some(c);
        self
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        // hit-test the full row (box + label area).  We use a generous
        // horizontal extent so clicking the label text also toggles the box.
        let x = mx as f32;
        let y = my as f32;
        let label_w = self
            .label
            .as_ref()
            .map_or(0.0, |l| l.len() as f32 * self.label_font_size * 0.6 + 6.0);
        let total_w = self.rect[2] + label_w;
        x >= self.rect[0]
            && x <= self.rect[0] + total_w
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    fn toggle(&mut self) {
        self.checked = !self.checked;
        if let Some(cb) = &self.on_change {
            cb(self.checked);
        }
    }
}

impl Widget for Checkbox {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        let r = [self.radius; 4];
        // outer box background
        cmds.push(RenderCommand::Quad {
            rect: Rect {
                x: self.rect[0],
                y: self.rect[1],
                width: self.rect[2],
                height: self.rect[3],
            },
            color: self.bg_color,
            radii: r,
            flags: 0,
        });
        // inner fill when checked (2 px inset)
        if self.checked {
            let pad = 3.0_f32;
            cmds.push(RenderCommand::Quad {
                rect: Rect {
                    x: self.rect[0] + pad,
                    y: self.rect[1] + pad,
                    width: (self.rect[2] - pad * 2.0).max(0.0),
                    height: (self.rect[3] - pad * 2.0).max(0.0),
                },
                color: self.check_color,
                radii: r,
                flags: 0,
            });
        }
        // label
        if let Some(label) = &self.label {
            cmds.push(RenderCommand::Text {
                rect: Rect {
                    x: self.rect[0] + self.rect[2] + 6.0,
                    y: self.rect[1] + (self.rect[3] - self.label_font_size) * 0.5,
                    width: 0.0,
                    height: 0.0,
                },
                text: label.clone(),
                color: self.label_color,
                font_size: self.label_font_size,
            });
        }
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        self.hit(mx, my)
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed && self.hit(mx, my) {
            self.toggle();
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        let label_w = self
            .label
            .as_ref()
            .map_or(0.0, |l| l.len() as f32 * self.label_font_size * 0.6 + 6.0);
        Some([
            self.rect[0],
            self.rect[1],
            self.rect[2] + label_w,
            self.rect[3],
        ])
    }

    fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    fn apply_constraint(&mut self, container_w: f32, container_h: f32) {
        if let Some(c) = &self.constraint.clone() {
            c.apply_to_rect(&mut self.rect, container_w, container_h);
        }
    }

    fn apply_constraint_with(&mut self, c: &crate::constraint::Constraint, cw: f32, ch: f32) {
        c.apply_to_rect(&mut self.rect, cw, ch);
    }
}
