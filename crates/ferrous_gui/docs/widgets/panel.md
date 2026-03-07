# Panel / PanelBuilder

`PanelBuilder` is the recommended way to build a panel of controls quickly.
It places widgets automatically in a **column** (top-to-bottom) or **row**
(left-to-right), returns `Rc<RefCell<…>>` handles for every widget it creates,
and produces a `Panel` value that implements `Widget` and can be added to a
`Ui` directly.

## Type aliases

```rust
pub type ButtonHandle    = Rc<RefCell<Button>>;
pub type SliderHandle    = Rc<RefCell<Slider>>;
pub type TextInputHandle = Rc<RefCell<TextInput>>;
pub type LabelHandle     = Rc<RefCell<Label>>;
pub type CheckboxHandle  = Rc<RefCell<Checkbox>>;
pub type DropdownHandle  = Rc<RefCell<Dropdown>>;
```

## `Panel` struct

```rust
pub struct Panel {
    pub buttons:     Vec<ButtonHandle>,
    pub sliders:     Vec<SliderHandle>,
    pub text_inputs: Vec<TextInputHandle>,
    pub labels:      Vec<LabelHandle>,
    pub checkboxes:  Vec<CheckboxHandle>,
    pub dropdowns:   Vec<DropdownHandle>,
    pub constraint:  Option<Constraint>, // reactive layout (optional)
    // internal container widget — not pub
}
```

`Panel` implements `Widget`, so `ui.add(panel)` registers the whole panel
(and all its children) in one call.

## `PanelBuilder` API

### Constructors

```rust
// Column — items stacked top-to-bottom, panel width fixed
PanelBuilder::column(x: f32, y: f32, width: f32) -> PanelBuilder

// Row — items placed left-to-right, panel height fixed
PanelBuilder::row(x: f32, y: f32, height: f32) -> PanelBuilder
```

### Layout configuration

| Method | Default | Description |
|--------|---------|-------------|
| `.padding(f32)` | `8.0` | Inset from the panel edges |
| `.gap(f32)` | `4.0` | Space between items |
| `.item_size(f32)` | `28.0` | Height (column) or width (row) of each item |
| `.with_background(color)` | `None` | Filled background behind all items |
| `.with_constraint(c)` | `None` | Attach a reactive [`Constraint`](../constraint.md) |

### Adding widgets

| Method | Returns | Description |
|--------|---------|-------------|
| `.add_button(label)` | `&mut Self` | Button with a text label |
| `.add_button_with_radius(label, radius)` | `&mut Self` | Button with rounded corners |
| `.add_slider(min, max, value)` | `&mut Self` | Slider with range and initial value |
| `.add_text_input(placeholder)` | `&mut Self` | Single-line text field |
| `.add_label(text)` | `&mut Self` | Static text label |
| `.add_checkbox(label, checked)` | `&mut Self` | Boolean toggle |
| `.add_dropdown(options, selected)` | `&mut Self` | Drop-down combo box |
| `.add_row(items)` | `&mut Self` | Horizontal sub-row of [`RowItem`](#rowitem) widgets |

### Finalise

```rust
.build() -> Panel
```

Consumes the builder, creates all widgets in order, and returns the `Panel`.

## Full example

```rust
use ferrous_gui::{PanelBuilder, Panel, Ui};

struct SettingsPanel {
    panel: Panel,
}

impl SettingsPanel {
    pub fn new() -> Self {
        let panel = PanelBuilder::column(20.0, 20.0, 220.0)
            .padding(10.0)
            .gap(6.0)
            .item_size(30.0)
            .with_background([0.10, 0.10, 0.10, 0.95])
            .add_label("Graphics Settings")
            .add_dropdown(vec!["Low", "Medium", "High", "Ultra"], 2)
            .add_checkbox("VSync", true)
            .add_label("Volume")
            .add_slider(0.0, 100.0, 80.0)
            .add_label("Player Name")
            .add_text_input("Enter name…")
            .add_button("Apply")
            .build();

        Self { panel }
    }

    pub fn register(&self, ui: &mut Ui) {
        // Panel implements Widget; all children are registered transitively
        ui.add(self.panel.clone());
    }

    pub fn read(&self) {
        let quality = self.panel.dropdowns[0].borrow().selected;
        let vsync   = self.panel.checkboxes[0].borrow().checked;
        let volume  = self.panel.sliders[0].borrow().value;
        let name    = self.panel.text_inputs[0].borrow().text.clone();
        let clicked = self.panel.buttons[0].borrow().pressed;

        println!("quality={quality} vsync={vsync} vol={volume:.0} name={name}");
        if clicked {
            self.panel.buttons[0].borrow_mut().pressed = false;
            apply_settings(quality, vsync, volume, &name);
        }
    }
}
```

## Attaching callbacks to panel widgets

The `PanelBuilder::add_*` methods produce handles but do not accept callbacks
directly. Attach callbacks after calling `.build()`:

```rust
let panel = PanelBuilder::column(20.0, 20.0, 200.0)
    .add_button("Save")
    .add_slider(0.0, 1.0, 0.5)
    .build();

// Attach on_click after build
{
    let mut btn = panel.buttons[0].borrow_mut();
    // on_click is set via the builder fluent method,
    // so use a fresh Button if you need a callback pre-build:
    // PanelBuilder doesn't expose on_click directly.
    // Prefer polling `pressed` from update(), or build the button manually.
}

// For callbacks, build the widget manually and wrap in Rc:
use std::rc::Rc;
use std::cell::RefCell;

let save_btn: ButtonHandle = Rc::new(RefCell::new(
    Button::new(0.0, 0.0, 200.0, 30.0)
        .with_label("Save")
        .on_click(|| save_file()),
));
```

## Layout algorithm

For a **column** panel at `(x, y)` with `width`, items are placed at:

```
item_y = y + padding + i * (item_size + gap)
item_rect = [x + padding, item_y, width - 2*padding, item_size]
```

Panel height auto-sizes to enclose all items plus bottom padding.

For a **row** panel, `x` and `y` are swapped in the formula and panel width
auto-sizes instead.

## Notes

- `Panel` is `Clone` because all handles are `Rc` (cheap clone of the pointer).
- All widgets inside the panel share their `Rc<RefCell<…>>` with the handles
  returned by the builder — mutations visible through the handle are immediately
  reflected in `Ui` rendering.
- Nested panels are supported: call `ui.add(panel)` after building; or add a
  `Panel` as a child of a `Container`.

## `RowItem`

`RowItem` describes a single cell in a horizontal sub-row created by `add_row`.
The row occupies exactly one `item_size`-tall slot in the column.

```rust
pub enum RowItem {
    Button { label: &'static str, radius: f32 },
    Label  { text:  &'static str },
    Spacer { flex:  f32 },
}
```

| Variant | Description |
|---------|-------------|
| `Button { label, radius }` | A labelled push-button. Added to `panel.buttons`. |
| `Label { text }` | A static text label. Added to `panel.labels`. |
| `Spacer { flex }` | Invisible flexible gap. Width is `flex` divided by the total flex of all spacers in the row, multiplied by the remaining width after fixed items. |

### Layout algorithm for `add_row`

1. Count all non-`Spacer` items and assign each the same fixed width
   (`(row_width - 2*padding - (n-1)*gap) / n_fixed`).
2. Remaining width is divided proportionally among `Spacer` items by `flex`.
3. Items are placed left-to-right with `gap` between them.

### `add_row` example

```rust
use ferrous_gui::{PanelBuilder, RowItem};

let panel = PanelBuilder::column(20.0, 20.0, 240.0)
    .padding(8.0)
    .gap(4.0)
    .item_size(30.0)
    .add_label("Actions")
    .add_row(vec![
        RowItem::Button { label: "OK",     radius: 4.0 },
        RowItem::Spacer { flex: 1.0 },
        RowItem::Button { label: "Cancel", radius: 4.0 },
    ])
    .build();

// panel.buttons[0] → "OK", panel.buttons[1] → "Cancel"
```

## Reactive positioning

```rust
use ferrous_gui::{Constraint, SizeExpr, PanelBuilder};

// Panel pinned 16 px from the right and always vertically centred
let panel = PanelBuilder::column(0.0, 0.0, 200.0)
    .add_label("Tools")
    .add_button("Inspect")
    .with_constraint(
        Constraint::new()
            .x(SizeExpr::from_right(16.0))
            .y(SizeExpr::center())
    )
    .build();
```

See [constraint.md](../constraint.md).
