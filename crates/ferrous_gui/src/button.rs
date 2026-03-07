use crate::constraint::Constraint;
use crate::{layout::Rect, RenderCommand, Widget};

/// Simple rectangular button widget used for interactive UIs.
///
/// Tracks hover/press state and produces a coloured quad when collected.
/// Supports an optional centred text label, per-corner rounding, a tooltip
/// string and an `on_click` callback that is fired on mouse-release inside
/// the widget.
///
/// ## Example
/// ```rust
/// use ferrous_gui::Button;
/// let btn = Button::new(10.0, 10.0, 120.0, 32.0)
///     .with_label("Save")
///     .with_radius(6.0)
///     .with_tooltip("Save the current file");
/// ```
pub struct Button {
    pub rect: [f32; 4], // x, y, width, height
    pub hovered: bool,
    pub pressed: bool,
    /// base colour (will be tinted when hovered/pressed)
    pub color: [f32; 4],
    /// corner radius in pixels; zero means sharp corners
    /// radii for each corner ([top-left, top-right, bottom-left, bottom-right]).
    pub radii: [f32; 4],
    /// optional text label rendered centred inside the button
    pub label: Option<String>,
    /// font size used when rendering the label (default 14.0)
    pub label_font_size: f32,
    /// label text colour (default white)
    pub label_color: [f32; 4],
    /// optional tooltip string shown on hover
    pub tooltip: Option<String>,
    /// optional callback fired when the button is released while hovered.
    /// Stored as a boxed closure so it is not `Clone`/`Debug`.
    on_click: Option<Box<dyn Fn() + Send + Sync>>,
    /// Optional reactive layout constraint. Resolved each frame by
    /// [`crate::ui::Ui::resolve_constraints`].
    pub constraint: Option<Constraint>,
}

impl Button {
    /// Create a new button at given position/size.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            hovered: false,
            pressed: false,
            color: [0.2, 0.2, 0.8, 1.0],
            radii: [0.0; 4],
            label: None,
            label_font_size: 14.0,
            label_color: [1.0, 1.0, 1.0, 1.0],
            tooltip: None,
            on_click: None,
            constraint: None,
        }
    }

    /// Set the label text that will be centered inside the button.
    pub fn with_label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// Override the font size used for the label (default 14.0).
    pub fn with_label_font_size(mut self, size: f32) -> Self {
        self.label_font_size = size;
        self
    }

    /// Override the label text colour.
    pub fn with_label_color(mut self, color: [f32; 4]) -> Self {
        self.label_color = color;
        self
    }

    /// Attach a tooltip string shown when the button is hovered.
    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Register a callback that fires when the button is clicked.
    pub fn on_click<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    /// Set a uniform radius for all four corners.
    pub fn with_radius(mut self, r: f32) -> Self {
        self.radii = [r; 4];
        self
    }

    /// Set individual radii for the corners. Order is
    /// `[top-left, top-right, bottom-left, bottom-right]`.
    pub fn with_radii(mut self, radii: [f32; 4]) -> Self {
        self.radii = radii;
        self
    }

    /// Shorter alias for `with_radii`. Accepts four values directly.
    pub fn round(mut self, tl: f32, tr: f32, bl: f32, br: f32) -> Self {
        self.radii = [tl, tr, bl, br];
        self
    }

    /// Round only the top-left corner.
    pub fn round_tl(mut self, r: f32) -> Self {
        self.radii[0] = r;
        self
    }

    /// Round only the top-right corner.
    pub fn round_tr(mut self, r: f32) -> Self {
        self.radii[1] = r;
        self
    }

    /// Round only the bottom-left corner.
    pub fn round_bl(mut self, r: f32) -> Self {
        self.radii[2] = r;
        self
    }

    /// Round only the bottom-right corner.
    pub fn round_br(mut self, r: f32) -> Self {
        self.radii[3] = r;
        self
    }

    /// Attach a reactive layout constraint.  The constraint is resolved
    /// every frame by [`crate::ui::Ui::resolve_constraints`].
    pub fn with_constraint(mut self, c: Constraint) -> Self {
        self.constraint = Some(c);
        self
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

    /// Returns the computed tint colour for the current hover/press state.
    fn tinted_color(&self) -> [f32; 4] {
        if self.pressed {
            [
                (self.color[0] * 0.7).min(1.0),
                (self.color[1] * 0.7).min(1.0),
                (self.color[2] * 0.7).min(1.0),
                self.color[3],
            ]
        } else if self.hovered {
            [
                (self.color[0] * 1.3).min(1.0),
                (self.color[1] * 1.3).min(1.0),
                (self.color[2] * 1.3).min(1.0),
                self.color[3],
            ]
        } else {
            self.color
        }
    }

    /// Convenience drawing method that pushes the button background quad
    /// directly into a `GuiBatch`. Does **not** emit a label; use
    /// [`draw_with_text`] when a font is available.
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch) {
        batch.push(crate::renderer::GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color: self.tinted_color(),
            radii: self.radii,
            flags: 0,
        });
    }

    /// Draw the button background **and** its centred label into the
    /// provided batches.  When `font` is `None` the label is silently
    /// skipped (matching the pre-label behaviour).
    #[cfg(feature = "text")]
    pub fn draw_with_text(
        &self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
        font: Option<&ferrous_assets::Font>,
    ) {
        self.draw(quad_batch);
        if let (Some(label), Some(f)) = (&self.label, font) {
            // Approximate horizontal centering: each character ≈ font_size * 0.6
            let approx_text_w = label.len() as f32 * self.label_font_size * 0.6;
            let tx = self.rect[0] + (self.rect[2] - approx_text_w) * 0.5;
            let ty = self.rect[1] + (self.rect[3] - self.label_font_size) * 0.5;
            text_batch.draw_text(f, label, [tx, ty], self.label_font_size, self.label_color);
        }
    }
}

impl Widget for Button {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        let color = self.tinted_color();
        let rect = Rect {
            x: self.rect[0],
            y: self.rect[1],
            width: self.rect[2],
            height: self.rect[3],
        };
        cmds.push(RenderCommand::Quad {
            rect,
            color,
            radii: self.radii,
            flags: 0,
        });
        // Emit a centred text command when a label is set.
        if let Some(label) = &self.label {
            let approx_text_w = label.len() as f32 * self.label_font_size * 0.6;
            let tx = self.rect[0] + (self.rect[2] - approx_text_w) * 0.5;
            let ty = self.rect[1] + (self.rect[3] - self.label_font_size) * 0.5;
            cmds.push(RenderCommand::Text {
                rect: Rect {
                    x: tx,
                    y: ty,
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

    fn mouse_move(&mut self, mx: f64, my: f64) {
        self.hovered = self.hit(mx, my);
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            // press only when the cursor is over us
            self.pressed = self.hit(mx, my);
        } else {
            // release: fire on_click if released while still hovered
            if self.pressed && self.hit(mx, my) {
                if let Some(cb) = &self.on_click {
                    cb();
                }
            }
            self.pressed = false;
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
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
