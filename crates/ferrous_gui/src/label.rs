use crate::{layout::Rect, RenderCommand, Widget};

/// First-class text label widget that can be added directly to a [`Ui`] or
/// [`Container`] just like any other interactive widget.
///
/// Unlike the purely layout-based `Text` node in `builders.rs`, this widget
/// implements `Widget` and can participate in the retained widget tree.
///
/// ## Example
/// ```rust
/// use ferrous_gui::{Label, Ui};
/// let mut ui = Ui::new();
/// ui.add(Label::new(10.0, 30.0, "Hello, world!").with_font_size(18.0));
/// ```
pub struct Label {
    /// Top-left origin of the label in window coordinates.
    pub pos: [f32; 2],
    /// Text content.
    pub text: String,
    /// RGBA text colour (default opaque white).
    pub color: [f32; 4],
    /// Font size in pixels (default 14.0).
    pub font_size: f32,
    /// Optional maximum width; currently informational only.
    pub max_width: Option<f32>,
    /// Optional tooltip string.
    pub tooltip: Option<String>,
}

impl Label {
    /// Create a new label at `(x, y)` with the given text.
    pub fn new(x: f32, y: f32, text: impl Into<String>) -> Self {
        Self {
            pos: [x, y],
            text: text.into(),
            color: [1.0, 1.0, 1.0, 1.0],
            font_size: 14.0,
            max_width: None,
            tooltip: None,
        }
    }

    /// Override the text colour.
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    /// Override the font size.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set a maximum layout width (informational hint).
    pub fn with_max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }

    /// Attach a tooltip shown on hover.
    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Update the label text at runtime.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl Widget for Label {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        cmds.push(RenderCommand::Text {
            rect: Rect {
                x: self.pos[0],
                y: self.pos[1],
                width: self.max_width.unwrap_or(0.0),
                height: 0.0,
            },
            text: self.text.clone(),
            color: self.color,
            font_size: self.font_size,
        });
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        // Approximate width using the same heuristic as Button label.
        let approx_w = self.text.len() as f32 * self.font_size * 0.6;
        Some([self.pos[0], self.pos[1], approx_w, self.font_size])
    }

    fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }
}
