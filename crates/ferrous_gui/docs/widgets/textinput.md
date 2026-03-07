# TextInput

`TextInput` is a single-line editable text field with a visual blinking cursor
at the insertion point, keyboard navigation, and an optional `on_change`
callback.

## Fields

```rust
pub struct TextInput {
    pub rect:          [f32; 4],
    pub text:          String,
    pub focused:       bool,
    pub placeholder:   String,
    pub bg_color:      [f32; 4],
    pub text_color:    [f32; 4],
    pub cursor_pos:    usize,        // character (not byte) index
    pub cursor_color:  [f32; 4],     // default: opaque white
    pub tooltip:       Option<String>,
    // on_change: Box<dyn Fn(&str)>  (set via .on_change(|s|{…}))
}
```

- `cursor_pos` — character index of the insertion point.  
  Rendered as a 2 px wide quad inside the text area when focused.
- `placeholder` — shown (same colour as text) when `text` is empty.

## Construction

```rust
let mut input = TextInput::new(x, y, width, height);
input.placeholder = "Enter your name…".into();
input.bg_color    = [0.12, 0.12, 0.12, 1.0];
input.text_color  = [0.95, 0.95, 0.95, 1.0];

// With on_change callback
let input = TextInput::new(20.0, 20.0, 240.0, 32.0)
    .on_change(|s| println!("text: {s}"))
    .with_tooltip("Your name");
```

## Builder API

| Method | Description |
|--------|-------------|
| `on_change(fn)` | Callback `fn(&str)` fired on every character change |
| `with_tooltip(text)` | Tooltip returned via `Widget::tooltip()` |

## Keyboard behaviour

| Key | Action |
|-----|--------|
| Printable character | Insert at cursor |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character after cursor |
| `←` / `→` | Move cursor left / right |
| `Home` | Jump to beginning |
| `End` | Jump to end |

Clicking the widget gives focus and moves the cursor to the end of the
buffer. Clicking outside removes focus.

## Cursor rendering

When focused a 2 px × (height − 6 px) white quad is drawn at the cursor's
approximate pixel position (based on `cursor_pos × font_size × 0.6`). The
cursor is always visible while focused; blink animation can be implemented in
application code by toggling `cursor_color[3]` each frame.

## `draw` signature

```rust
// Feature "text" enabled (default)
pub fn draw(&self, quad_batch: &mut GuiBatch,
            text_batch: &mut TextBatch, font: Option<&Font>);
```

Draws background, text (or placeholder), and the cursor bar.

## Programmatic editing

```rust
widget.insert_char('A');    // insert at cursor_pos
widget.backspace();         // delete before cursor
widget.delete_forward();    // delete after cursor
widget.cursor_left();
widget.cursor_right();
widget.cursor_home();
widget.cursor_end();
```

## Notes

- `TextInput` is not `Clone`/`Debug`. Use `Rc<RefCell<TextInput>>` for shared
  access — the type alias `TextInputHandle` is exported from `panel`.
- No text selection support; for multi-line or rich editing implement a custom
  `Widget`.
