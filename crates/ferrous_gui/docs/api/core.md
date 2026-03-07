# Core API -- Ui, Canvas, Widget

## `Widget` trait

Every GUI element implements `Widget`:

```rust
pub trait Widget {
    fn collect(&self, cmds: &mut Vec<RenderCommand>);
    fn hit(&self, _mx: f64, _my: f64) -> bool { false }
    fn mouse_input(&mut self, _mx: f64, _my: f64, _pressed: bool) {}
    fn mouse_move(&mut self, _mx: f64, _my: f64) {}
    fn keyboard_input(&mut self, _text: Option<&str>,
                      _key: Option<GuiKey>, _pressed: bool) {}
    fn bounding_rect(&self) -> Option<[f32; 4]> { None }
    fn tooltip(&self) -> Option<&str> { None }
}
```

- `collect` — push draw commands for this frame
- `hit` — point-in-widget test used for focus tracking
- `mouse_input` / `mouse_move` — update hover/press/drag state
- `keyboard_input` — handle text and key events when focused
- `bounding_rect` — optional `[x, y, w, h]` used by containers for auto-sizing
- `tooltip` — optional string; callers query hovered widgets and render it

`Rc<RefCell<T>>` where `T: Widget` also implements `Widget`, so shared handles
can be added directly to a `Ui` or `Canvas`.

You can implement this trait to create completely custom widgets.

---

## `Canvas`

`Canvas` holds a heterogeneous collection of widgets and manages focus.

```rust
pub struct Canvas {
    children: Vec<Box<dyn Widget>>,
    focused:  Option<usize>,
}
```

| Method | Description |
|--------|-------------|
| `Canvas::new()` | Create empty canvas |
| `.add(widget)` | Push any `Widget + 'static` |
| `.mouse_move(mx, my)` | Forward cursor movement to all children |
| `.mouse_input(mx, my, pressed)` | Update focus on press; deliver to all children |
| `.keyboard_input(text, key, pressed)` | Deliver to focused child only |
| `.collect(cmds)` | Aggregate draw commands from all children |

---

## `Ui`

`Ui` wraps a `Canvas` and is the object you hold in your application.

```rust
pub struct Ui {
    canvas:   Canvas,
    viewport: Option<Rc<RefCell<ViewportWidget>>>,
}
```

### Construction and widget registration

```rust
let mut ui = Ui::new();
ui.add(button);       // any Widget + 'static
ui.add(slider);
```

### Input routing

The runner calls these automatically via `Ui::handle_window_event`. You do not
need to call them manually unless you manage your own event loop.

```rust
ui.mouse_move(mx, my);
ui.mouse_input(mx, my, pressed);
ui.keyboard_input(Some("a"), None, true);
ui.keyboard_input(None, Some(GuiKey::Backspace), true);
```

### Viewport helper

```rust
ui.register_viewport(vp_ref.clone());      // store + add to canvas
ui.set_viewport_rect(x, y, w, h);          // update rect on resize
ui.viewport_focused()                       // bool -- use to capture mouse for 3D camera
```

### Drawing

Called by the runner each frame. If you need manual control:

```rust
ui.draw(&mut quad_batch, &mut text_batch, Some(&font));
```

In your `draw_ui` callback you receive a [`DrawContext`](../../ferrous_app/docs/ferrous-app-trait.md#drawcontext)
which already holds `gui`, `text`, `font`, and `ctx` — use `dc.gui` and `dc.text` directly.

---

## `GuiKey`

A lightweight key enum used in `keyboard_input`, avoiding a direct dependency
on winit:

```rust
pub enum GuiKey {
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Enter,
    Escape,
    Tab,
}
```

When the `winit-backend` feature is enabled, `impl From<winit::keyboard::KeyCode>`
is provided. All variants above are mapped; unrecognised keys fall through to
`Backspace` (compile-time exhaustiveness requirement).

---

## `RenderCommand`

Widgets produce `RenderCommand` values that are later converted to GPU draw
calls by `UiPass`. You only need this when writing custom widgets.

```rust
pub enum RenderCommand {
    Quad {
        rect:  Rect,
        color: [f32; 4],
        radii: [f32; 4],   // per-corner radii [TL, TR, BL, BR]
        flags: u32,        // bit 0 = colour-wheel gradient
    },
    Text {
        rect:      Rect,   // origin; width/height informational only
        text:      String,
        color:     [f32; 4],
        font_size: f32,
    },
    /// Signal the renderer to begin scissoring to `rect`.
    PushClip { rect: Rect },
    /// End the most recent scissor region.
    PopClip,
}
```

`PushClip`/`PopClip` are emitted by `Container` when `clip = true`.
The `GuiBatch`/`TextBatch` conversion layer ignores them; a renderer pass that
wants clipping must consume them and set the GPU scissor rect.
