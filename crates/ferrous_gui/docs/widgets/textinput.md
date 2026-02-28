<!--
Reference documentation for the `TextInput` widget.
-->

# TextInput widget

`TextInput` is a minimalist single-line editable text field.  It stores a
string buffer and handles focus and basic keyboard input.  It is not a
full-fledged text editor; for example, there is no cursor or selection
support, but it is sufficient for simple forms.

## Data members

```rust
#[derive(Debug, Clone)]
pub struct TextInput {
    pub rect: [f32; 4],
    pub text: String,
    pub focused: bool,
    pub placeholder: String,
    pub bg_color: [f32; 4],
    pub text_color: [f32; 4],
}
```

- **`rect`** – bounding box in window coordinates.
- **`text`** – current contents of the control.
- **`focused`** – whether the widget has keyboard focus.
- **`placeholder`** – string displayed when `text` is empty.
- **`bg_color`**, **`text_color`** – colours used for background and
  rendered text.

## Methods

- `new(x, y, w, h)` – construct with default colours and empty text.
- `hit(mx, my)` – returns true if the point lies inside `rect`.
- `insert_char(c)` – append a character (no-op if not focused).
- `backspace()` – remove last character (focused only).
- `draw(quad_batch, text_batch, font)` – emit a background quad and,
  if a font is provided, draw the current text (or placeholder).

Keyboard events are handled via the `Widget` trait.  When the widget is
focused, `keyboard_input` will append printable characters and handle
backspace.

## Example usage

```rust
let mut input = TextInput::new(60.0, 60.0, 200.0, 24.0);
input.placeholder = "Enter name".into();
ui.add(input.clone());

// later, after event processing:
println!("Current contents: {}", input.text);
```

The consumer of the widget typically keeps a mutable reference (or clone)
so that the text can be read or modified at will.

## Rendering

The `draw` helper draws a filled rectangle using `bg_color`.  If a font is
provided, text is rendered vertically centred with a 4‑pixel left margin.
The placeholder text uses the same colour as normal text, so it may be
beneficial to choose a lighter shade when setting `placeholder`.

## Behaviour notes

- Clicking inside the rect gives the widget focus; clicking elsewhere
  removes focus.
- Only characters that are not classified as control characters are
  inserted.
- There is no support for selecting or moving the cursor; editing is
  always at the end of the buffer.
