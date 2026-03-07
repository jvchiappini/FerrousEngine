use std::rc::Rc;
use std::cell::RefCell;

use crate::button::Button;
use crate::checkbox::Checkbox;
use crate::dropdown::Dropdown;
use crate::label::Label;
use crate::slider::Slider;
use crate::textinput::TextInput;
use crate::canvas::Canvas;
use crate::layout::{Rect, RenderCommand};
use crate::widget::Widget;
use crate::GuiKey;

/// Orientation of a [`Panel`] layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelDirection {
    Column,
    Row,
}

/// Shared handle to a [`Button`] created by a [`Panel`] builder.
pub type ButtonHandle = Rc<RefCell<Button>>;
/// Shared handle to a [`Slider`] created by a [`Panel`] builder.
pub type SliderHandle = Rc<RefCell<Slider>>;
/// Shared handle to a [`TextInput`] created by a [`Panel`] builder.
pub type TextInputHandle = Rc<RefCell<TextInput>>;
/// Shared handle to a [`Label`] created by a [`Panel`] builder.
pub type LabelHandle = Rc<RefCell<Label>>;
/// Shared handle to a [`Checkbox`] created by a [`Panel`] builder.
pub type CheckboxHandle = Rc<RefCell<Checkbox>>;
/// Shared handle to a [`Dropdown`] created by a [`Panel`] builder.
pub type DropdownHandle = Rc<RefCell<Dropdown>>;

/// Built panel returned by [`PanelBuilder::build`].
///
/// After building, widget handles can be accessed via the public `Vec` fields.
/// Because the widgets are wrapped in `Rc<RefCell<…>>` any mutation through a
/// handle is immediately visible to the panel (and vice versa) without any
/// manual synchronisation.
///
/// The panel itself implements [`Widget`] and can be added directly to a [`Ui`].
pub struct Panel {
    /// All button handles, in order of insertion.
    pub buttons: Vec<ButtonHandle>,
    /// All slider handles, in order of insertion.
    pub sliders: Vec<SliderHandle>,
    /// All text-input handles, in order of insertion.
    pub text_inputs: Vec<TextInputHandle>,
    /// All label handles, in order of insertion.
    pub labels: Vec<LabelHandle>,
    /// All checkbox handles, in order of insertion.
    pub checkboxes: Vec<CheckboxHandle>,
    /// All dropdown handles, in order of insertion.
    pub dropdowns: Vec<DropdownHandle>,
    /// Background colour of the panel area; `None` means transparent.
    pub bg_color: Option<[f32; 4]>,
    /// Bounding rect computed at build time.
    pub rect: [f32; 4],
    canvas: Canvas,
}

impl Widget for Panel {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        if let Some(col) = self.bg_color {
            cmds.push(RenderCommand::Quad {
                rect: Rect {
                    x: self.rect[0],
                    y: self.rect[1],
                    width: self.rect[2],
                    height: self.rect[3],
                },
                color: col,
                radii: [0.0; 4],
                flags: 0,
            });
        }
        self.canvas.collect(cmds);
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    fn mouse_move(&mut self, mx: f64, my: f64) {
        self.canvas.mouse_move(mx, my);
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        self.canvas.mouse_input(mx, my, pressed);
    }

    fn keyboard_input(&mut self, text: Option<&str>, key: Option<GuiKey>, pressed: bool) {
        self.canvas.keyboard_input(text, key, pressed);
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Fluent builder for a [`Panel`] that positions widgets automatically in a
/// column or row.
///
/// ## Example – column of buttons
/// ```rust
/// use ferrous_gui::panel::PanelBuilder;
/// let panel = PanelBuilder::column(10.0, 10.0, 180.0)
///     .padding(8.0)
///     .gap(6.0)
///     .add_button("Save")
///     .add_button("Load")
///     .add_button("Quit")
///     .build();
/// // panel.buttons[0] is the "Save" button handle, etc.
/// ```
pub struct PanelBuilder {
    origin: [f32; 2],
    /// Fixed size on the main axis (width for Column, height for Row).
    /// Items are sized on the cross-axis.
    fixed: f32,
    /// Default height (Column) or width (Row) of each item (default 28.0).
    item_size: f32,
    direction: PanelDirection,
    padding: f32,
    gap: f32,
    bg_color: Option<[f32; 4]>,
    items: Vec<PanelItem>,
}

enum PanelItem {
    Button(String, f32 /* radius */),
    Slider(f32 /* min */, f32 /* max */, f32 /* value */),
    TextInput(String /* placeholder */),
    Label(String),
    Checkbox(String, bool /* initial */),
    Dropdown(Vec<String>, usize /* selected */),
}

impl PanelBuilder {
    /// Start building a vertically stacked (column) panel.
    /// `fixed_width` is the width of every item.
    pub fn column(x: f32, y: f32, fixed_width: f32) -> Self {
        Self {
            origin: [x, y],
            fixed: fixed_width,
            item_size: 28.0,
            direction: PanelDirection::Column,
            padding: 4.0,
            gap: 4.0,
            bg_color: None,
            items: Vec::new(),
        }
    }

    /// Start building a horizontally stacked (row) panel.
    /// `fixed_height` is the height of every item.
    pub fn row(x: f32, y: f32, fixed_height: f32) -> Self {
        Self {
            origin: [x, y],
            fixed: fixed_height,
            item_size: 100.0,
            direction: PanelDirection::Row,
            padding: 4.0,
            gap: 4.0,
            bg_color: None,
            items: Vec::new(),
        }
    }

    /// Padding inside the panel boundary (applied to all four sides).
    pub fn padding(mut self, v: f32) -> Self {
        self.padding = v;
        self
    }

    /// Gap between consecutive items.
    pub fn gap(mut self, v: f32) -> Self {
        self.gap = v;
        self
    }

    /// Override the default item cross-axis size (height in Column, width in Row).
    pub fn item_size(mut self, s: f32) -> Self {
        self.item_size = s;
        self
    }

    /// Optional background colour for the whole panel.
    pub fn with_background(mut self, color: [f32; 4]) -> Self {
        self.bg_color = Some(color);
        self
    }

    /// Add a button with a text label.
    pub fn add_button(mut self, label: impl Into<String>) -> Self {
        self.items.push(PanelItem::Button(label.into(), 4.0));
        self
    }

    /// Add a button with a label and a specific corner radius.
    pub fn add_button_with_radius(mut self, label: impl Into<String>, radius: f32) -> Self {
        self.items.push(PanelItem::Button(label.into(), radius));
        self
    }

    /// Add a slider with a range and initial value.
    pub fn add_slider(mut self, min: f32, max: f32, value: f32) -> Self {
        self.items.push(PanelItem::Slider(min, max, value));
        self
    }

    /// Add a text-input field with a placeholder string.
    pub fn add_text_input(mut self, placeholder: impl Into<String>) -> Self {
        self.items.push(PanelItem::TextInput(placeholder.into()));
        self
    }

    /// Add a static label.
    pub fn add_label(mut self, text: impl Into<String>) -> Self {
        self.items.push(PanelItem::Label(text.into()));
        self
    }

    /// Add a checkbox.
    pub fn add_checkbox(mut self, label: impl Into<String>, checked: bool) -> Self {
        self.items.push(PanelItem::Checkbox(label.into(), checked));
        self
    }

    /// Add a dropdown with option strings and an initial selection index.
    pub fn add_dropdown(mut self, options: Vec<impl Into<String>>, selected: usize) -> Self {
        let opts: Vec<String> = options.into_iter().map(Into::into).collect();
        self.items.push(PanelItem::Dropdown(opts, selected));
        self
    }

    /// Consume the builder and produce a ready-to-use [`Panel`].
    pub fn build(self) -> Panel {
        let mut canvas = Canvas::new();
        let mut buttons = Vec::new();
        let mut sliders = Vec::new();
        let mut text_inputs = Vec::new();
        let mut labels = Vec::new();
        let mut checkboxes = Vec::new();
        let mut dropdowns = Vec::new();

        let p = self.padding;
        let mut cursor = p; // advances along the main axis

        let cross = match self.direction {
            PanelDirection::Column => self.fixed - p * 2.0, // item width
            PanelDirection::Row => self.fixed - p * 2.0,    // item height
        };

        for item in &self.items {
            let (ix, iy, iw, ih) = match self.direction {
                PanelDirection::Column => {
                    (self.origin[0] + p, self.origin[1] + cursor, cross, self.item_size)
                }
                PanelDirection::Row => {
                    (self.origin[0] + cursor, self.origin[1] + p, self.item_size, cross)
                }
            };

            let item_main = match self.direction {
                PanelDirection::Column => self.item_size,
                PanelDirection::Row => self.item_size,
            };

            match item {
                PanelItem::Button(label, radius) => {
                    let btn = Rc::new(RefCell::new(
                        Button::new(ix, iy, iw, ih)
                            .with_label(label.clone())
                            .with_radius(*radius),
                    ));
                    canvas.add(Rc::clone(&btn));
                    buttons.push(btn);
                }
                PanelItem::Slider(min, max, value) => {
                    let s = Rc::new(RefCell::new(
                        Slider::new(ix, iy, iw, ih, *value).range(*min, *max),
                    ));
                    canvas.add(Rc::clone(&s));
                    sliders.push(s);
                }
                PanelItem::TextInput(placeholder) => {
                    let mut ti = TextInput::new(ix, iy, iw, ih);
                    ti.placeholder = placeholder.clone();
                    let ti = Rc::new(RefCell::new(ti));
                    canvas.add(Rc::clone(&ti));
                    text_inputs.push(ti);
                }
                PanelItem::Label(text) => {
                    let lbl = Rc::new(RefCell::new(
                        Label::new(ix, iy + (ih - 14.0) * 0.5, text.clone()),
                    ));
                    canvas.add(Rc::clone(&lbl));
                    labels.push(lbl);
                }
                PanelItem::Checkbox(label, checked) => {
                    let cb = Rc::new(RefCell::new(
                        Checkbox::new(ix, iy + (ih - 16.0) * 0.5, label.clone())
                            .checked(*checked),
                    ));
                    canvas.add(Rc::clone(&cb));
                    checkboxes.push(cb);
                }
                PanelItem::Dropdown(options, selected) => {
                    let dd = Rc::new(RefCell::new(
                        Dropdown::new(ix, iy, iw, ih)
                            .with_options(options.clone())
                            .with_selected(*selected),
                    ));
                    canvas.add(Rc::clone(&dd));
                    dropdowns.push(dd);
                }
            }

            cursor += item_main + self.gap;
        }

        // Compute the panel bounding rect.
        let total_main = if self.items.is_empty() {
            0.0
        } else {
            cursor - self.gap + p
        };
        let (pw, ph) = match self.direction {
            PanelDirection::Column => (self.fixed, total_main),
            PanelDirection::Row => (total_main, self.fixed),
        };

        Panel {
            buttons,
            sliders,
            text_inputs,
            labels,
            checkboxes,
            dropdowns,
            bg_color: self.bg_color,
            rect: [self.origin[0], self.origin[1], pw, ph],
            canvas,
        }
    }
}
