use crate::{layout::Rect, RenderCommand, Widget};

/// Drop-down (combo-box) widget.
///
/// Renders a closed button showing the currently selected option.  When
/// clicked it opens an inline pop-up list below the button; clicking an
/// item selects it and collapses the list.
///
/// ## Example
/// ```rust
/// use ferrous_gui::Dropdown;
/// let mut dd = Dropdown::new(10.0, 10.0, 160.0, 28.0)
///     .with_options(vec!["Option A", "Option B", "Option C"])
///     .with_selected(0)
///     .on_change(|i, s| println!("selected {i}: {s}"));
/// ```
pub struct Dropdown {
    /// Bounding rect of the closed button `[x, y, w, h]`.
    pub rect: [f32; 4],
    /// List of selectable option strings.
    pub options: Vec<String>,
    /// Index of the currently selected option (`None` when the list is empty).
    pub selected: Option<usize>,
    /// Whether the dropdown list is currently open.
    pub open: bool,
    /// Background colour of the closed button.
    pub button_color: [f32; 4],
    /// Background colour of the open list.
    pub list_color: [f32; 4],
    /// Background colour of the hovered item.
    pub hover_color: [f32; 4],
    /// Text / label colour.
    pub text_color: [f32; 4],
    /// Font size for option labels (default 14.0).
    pub font_size: f32,
    /// Corner radius of the button.
    pub radius: f32,
    /// Optional tooltip string.
    pub tooltip: Option<String>,
    /// Hovered item index while the list is open.
    hovered_item: Option<usize>,
    /// Optional callback fired when the selection changes.
    on_change: Option<Box<dyn Fn(usize, &str) + Send + Sync>>,
}

impl Dropdown {
    /// Create a new dropdown at `(x, y)` with given size. Options can be set
    /// via [`with_options`](Self::with_options).
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            options: Vec::new(),
            selected: None,
            open: false,
            button_color: [0.2, 0.2, 0.25, 1.0],
            list_color: [0.15, 0.15, 0.2, 1.0],
            hover_color: [0.3, 0.3, 0.5, 1.0],
            text_color: [1.0, 1.0, 1.0, 1.0],
            font_size: 14.0,
            radius: 4.0,
            tooltip: None,
            hovered_item: None,
            on_change: None,
        }
    }

    /// Set the list of options.  Clears any existing selection if the new
    /// list is shorter.
    pub fn with_options(mut self, opts: Vec<impl Into<String>>) -> Self {
        self.options = opts.into_iter().map(Into::into).collect();
        if self.selected.map_or(false, |i| i >= self.options.len()) {
            self.selected = None;
        }
        if self.selected.is_none() && !self.options.is_empty() {
            self.selected = Some(0);
        }
        self
    }

    /// Set the initially selected index.
    pub fn with_selected(mut self, idx: usize) -> Self {
        if !self.options.is_empty() {
            self.selected = Some(idx.min(self.options.len() - 1));
        }
        self
    }

    /// Attach a tooltip.
    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Register a callback fired when the selection changes.
    pub fn on_change<F: Fn(usize, &str) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Returns the currently selected option string, if any.
    pub fn selected_str(&self) -> Option<&str> {
        self.selected
            .and_then(|i| self.options.get(i))
            .map(|s| s.as_str())
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    fn item_rect(&self, idx: usize) -> [f32; 4] {
        let h = self.rect[3];
        [
            self.rect[0],
            self.rect[1] + h + idx as f32 * h,
            self.rect[2],
            h,
        ]
    }

    fn hit_button(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    fn hit_item(&self, mx: f64, my: f64) -> Option<usize> {
        for i in 0..self.options.len() {
            let r = self.item_rect(i);
            let x = mx as f32;
            let y = my as f32;
            if x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3] {
                return Some(i);
            }
        }
        None
    }

    fn select(&mut self, idx: usize) {
        self.selected = Some(idx);
        self.open = false;
        self.hovered_item = None;
        if let Some(cb) = &self.on_change {
            cb(idx, &self.options[idx]);
        }
    }
}

impl Widget for Dropdown {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        let r = [self.radius; 4];
        let label = self.selected_str().unwrap_or("—");

        // closed button
        cmds.push(RenderCommand::Quad {
            rect: Rect {
                x: self.rect[0],
                y: self.rect[1],
                width: self.rect[2],
                height: self.rect[3],
            },
            color: self.button_color,
            radii: r,
            flags: 0,
        });
        // chevron indicator (small triangle approximated as a right-aligned quad)
        let chev_w = 10.0_f32;
        cmds.push(RenderCommand::Quad {
            rect: Rect {
                x: self.rect[0] + self.rect[2] - chev_w - 4.0,
                y: self.rect[1] + (self.rect[3] - chev_w * 0.5) * 0.5,
                width: chev_w,
                height: chev_w * 0.5,
            },
            color: self.text_color,
            radii: [0.0; 4],
            flags: 0,
        });
        // selected label text
        let ty = self.rect[1] + (self.rect[3] - self.font_size) * 0.5;
        cmds.push(RenderCommand::Text {
            rect: Rect {
                x: self.rect[0] + 6.0,
                y: ty,
                width: 0.0,
                height: 0.0,
            },
            text: label.to_owned(),
            color: self.text_color,
            font_size: self.font_size,
        });

        // open list
        if self.open {
            for (i, opt) in self.options.iter().enumerate() {
                let ir = self.item_rect(i);
                let is_hovered = self.hovered_item == Some(i);
                let bg = if is_hovered {
                    self.hover_color
                } else {
                    self.list_color
                };
                cmds.push(RenderCommand::Quad {
                    rect: Rect {
                        x: ir[0],
                        y: ir[1],
                        width: ir[2],
                        height: ir[3],
                    },
                    color: bg,
                    radii: [0.0; 4],
                    flags: 0,
                });
                let ity = ir[1] + (ir[3] - self.font_size) * 0.5;
                cmds.push(RenderCommand::Text {
                    rect: Rect {
                        x: ir[0] + 6.0,
                        y: ity,
                        width: 0.0,
                        height: 0.0,
                    },
                    text: opt.clone(),
                    color: self.text_color,
                    font_size: self.font_size,
                });
            }
        }
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        if self.hit_button(mx, my) {
            return true;
        }
        if self.open {
            return self.hit_item(mx, my).is_some();
        }
        false
    }

    fn mouse_move(&mut self, mx: f64, my: f64) {
        if self.open {
            self.hovered_item = self.hit_item(mx, my);
        }
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if !pressed {
            return;
        }
        if self.open {
            if let Some(i) = self.hit_item(mx, my) {
                self.select(i);
                return;
            }
            // clicked outside the list → close without selecting
            if !self.hit_button(mx, my) {
                self.open = false;
            }
        } else if self.hit_button(mx, my) {
            self.open = true;
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        let h = self.rect[3];
        let total_h = if self.open {
            h + self.options.len() as f32 * h
        } else {
            h
        };
        Some([self.rect[0], self.rect[1], self.rect[2], total_h])
    }

    fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }
}
