# Dropdown

`Dropdown` is a combo-box widget. While closed it shows the selected option
plus a chevron button; clicking it opens an item list below. Selecting an item
closes the list and fires `on_change`.

## Fields

```rust
pub struct Dropdown {
    pub rect:          [f32; 4],    // [x, y, w, h] of the closed control
    pub options:       Vec<String>,
    pub selected:      usize,       // index into options
    pub open:          bool,
    pub hovered_item:  Option<usize>,
    pub tooltip:       Option<String>,
    pub bg_color:      [f32; 4],
    pub hover_color:   [f32; 4],
    pub text_color:    [f32; 4],
    // on_change: Box<dyn Fn(usize, &str)>
}
```

## Construction

```rust
// Minimal (no options yet)
let dd = Dropdown::new(20.0, 20.0, 200.0, 28.0);

// With options and initial selection
let dd = Dropdown::new(20.0, 20.0, 200.0, 28.0)
    .with_options(vec!["Low", "Medium", "High"])
    .with_selected(1);   // "Medium" is selected

// With callback
let dd = Dropdown::new(20.0, 20.0, 200.0, 28.0)
    .with_options(vec!["1x", "2x", "4x", "8x"])
    .with_selected(0)
    .on_change(|idx, label| {
        println!("MSAA changed to {label} (index {idx})");
    });

// With tooltip
let dd = Dropdown::new(20.0, 20.0, 200.0, 28.0)
    .with_options(vec!["Windowed", "Borderless", "Fullscreen"])
    .with_tooltip("Display mode");
```

## Builder API

| Method | Description |
|--------|-------------|
| `with_options(vec)` | Accepts `Vec<&str>` or `Vec<String>` |
| `with_selected(idx)` | Initial selected index (clamped to `options.len() - 1`) |
| `with_tooltip(text)` | Tooltip returned via `Widget::tooltip()` |
| `on_change(fn)` | Callback `fn(usize, &str)` fired on selection |

## Reading state

```rust
// Index
let idx = self.panel.dropdowns[0].borrow().selected;

// String
let preset = self.panel.dropdowns[0].borrow()
    .selected_str()
    .map(|s| s.to_owned());
```

## Rendering — closed state

One background quad + the selected option text (centred/left-aligned) + a
small chevron quad on the right edge.

## Rendering — open state

A quad per item is drawn below the control. The currently hovered item uses
`hover_color`; the selected item is drawn with a subtle accent. The item list
expands downward; no overflow / scroll is implemented.

## Keyboard behaviour

While open:

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move `hovered_item` |
| `Enter` | Confirm hovered item |
| `Escape` | Close without selection change |

## Notes

- `Dropdown` is not `Clone`/`Debug`. Use `Rc<RefCell<Dropdown>>` for shared
  access — `DropdownHandle` is exported from `panel`.
- The open item list is drawn as part of the widget's `collect` output.
  Because it renders above other widgets (z-order), ensure the dropdown is
  added to the `Ui` **after** widgets it must overlay.
