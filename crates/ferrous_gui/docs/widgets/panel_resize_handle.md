# PanelResizeHandle

Invisible drag handle used to resize an adjacent panel.  It behaves like the
"splitter" bars you see in IDEs such as VS Code or IntelliJ: the user hovers
and then drags a slim zone to change the panel width (horizontal axis) or
height (vertical axis).

## Struct

```rust
pub enum ResizeAxis {
    Horizontal, // drag in X → change width
    Vertical,   // drag in Y → change height
}

pub struct PanelResizeHandle {
    pub rect:        [f32; 4],   // [x, y, width, height] hit-test zone
    pub value:       f32,        // current splitter position along axis
    pub min:         f32,        // allowed range minimum
    pub max:         f32,        // allowed range maximum
    pub axis:        ResizeAxis, // horizontal or vertical
    pub hover_color: [f32; 4],   // visual indicator colour when active
    pub dragging:    bool,
    pub hovered:     bool,
    drag_start_cursor: f32,      // internal: cursor coord at drag start
    drag_start_value:  f32,      // internal: `value` at drag start
}
```

All fields are `Copy`/`Clone` which makes the type easy to store in
`Rc<RefCell<…>>` if sharing is required.

## Construction

```rust
let handle = PanelResizeHandle::new(
    100.0, 0.0,    // x,y top-left of tiny hit area (6–8 px wide typical)
    6.0, 600.0,    // width,height of hit area
    100.0,         // initial value (splitter position)
    50.0, 400.0,   // min/max allowed positions
    ResizeAxis::Horizontal,
);

// customize hover colour
let handle = handle.with_hover_color([1.0, 0.0, 0.0, 0.5]);
```

## Behaviour & API

`PanelResizeHandle` implements [`Widget`](../widget.md) so it can be placed in
a `Canvas` like any other element.

- `hit(mx,my)` returns `true` when the cursor is inside `rect`.
- `mouse_move` updates the `hovered` flag and, when dragging, modifies
  `value` by the cursor delta along the correct axis, clamping to `[min,max]`.
  The `rect` is automatically repositioned so the hit zone remains centred on
  `value`.
- `mouse_input` begins a drag if the handle is hovered when the button is
  pressed, and ends dragging on release.
- `collect(cmds)` emits a single visible quad only while `hovered` or
  `dragging`; this quad uses `hover_color` and matches the current `rect`.

Example of reacting to changes:

```rust
if handle.dragging {
    panel.width = handle.value; // or .height for Vertical axis
}
```

Because `rect` is public, you can reposition the hit area if the layout
changes (e.g. after a container resize) and call
`handle.apply_constraint_with(...)` if you use reactive constraints.

## Notes

- The handle itself is invisible until hovered or dragged; applications are
  responsible for drawing any additional visual cues such as a bar or icon.
- `value` and `rect` are kept in sync automatically during dragging; manual
  updates to `value` should also adjust `rect` if needed.

```rust
// manually move handle to a new value
handle.value = 200.0;
handle.rect[0] = handle.value - handle.rect[2] / 2.0; // horizontal example
```

See [slider.md](slider.md) for a similar drag-based widget and more details
on working with `Widget` implementations.
