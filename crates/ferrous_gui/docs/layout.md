# Layout System

`ferrous_gui` includes a lightweight layout engine modelled on CSS flexbox.
It computes where to place rectangular nodes based on margin, padding, size,
and alignment rules.

---

## Core types

| Type | Description |
|------|-------------|
| `Rect` | `{ x, y, width, height: f32 }` -- computed rectangle for a node |
| `RectOffset` | Per-side margin or padding. `RectOffset::all(v)` for uniform. |
| `Units` | `Px(f32)`, `Percentage(f32)`, `Flex(f32)` |
| `Alignment` | `Start`, `Center`, `End`, `Stretch` |
| `DisplayMode` | `Block`, `FlexRow`, `FlexColumn` |
| `Style` | Aggregates all layout properties for one node |
| `Node` | One element in the layout tree; contains `Style`, children, text, background |

---

## Building a layout tree

### Using `Node` directly

```rust
use ferrous_gui::layout::{Node, DisplayMode, Units, Alignment};

let mut root = Node::new()
    .with_display(DisplayMode::FlexColumn)
    .with_padding(16.0)
    .add_child(
        Node::new()
            .with_size(Units::Px(200.0), Units::Px(40.0))
            .set_background([0.2, 0.2, 0.8, 1.0])
            .set_text("Save")
            .set_text_color([1.0, 1.0, 1.0, 1.0])
    )
    .add_child(
        Node::new()
            .with_size(Units::Px(200.0), Units::Px(40.0))
            .set_background([0.3, 0.3, 0.3, 1.0])
            .set_text("Cancel")
    );

root.compute_layout(800.0, 600.0);
// root.children[0].rect.x / .y now contain computed positions
```

### Using declarative builders

`Row`, `Column`, `UiButton`, and `Text` are thin wrappers around `Node` with a
fluent API. They all implement `Into<Node>`.

```rust
use ferrous_gui::{Column, Row, UiButton, Text};

let panel: Node = Column::new()
    .with_padding(12.0)
    .add_child(
        Row::new()
            .with_padding(4.0)
            .add_child(UiButton::new("Open").with_margin(4.0))
            .add_child(UiButton::new("Save").with_margin(4.0))
    )
    .add_child(Text::new("Status: ready").with_margin(8.0))
    .into();
```

---

## `Column`

Stacks children vertically (`DisplayMode::FlexColumn`).

```rust
Column::new()
    .with_padding(10.0)   // inner padding
    .with_margin(5.0)     // outer margin
    .add_child(/* any Into<Node> */)
```

## `Row`

Arranges children horizontally (`DisplayMode::FlexRow`).

```rust
Row::new()
    .with_padding(8.0)
    .add_child(UiButton::new("A"))
    .add_child(UiButton::new("B"))
```

## `UiButton`

A `Node` pre-configured with a blue background, white text, and centred
alignment. Declarative only -- not interactive (use `Button` for click events).

```rust
UiButton::new("Click me")
    .with_padding(8.0)
    .with_margin(4.0)
```

## `Text`

A `Node` containing plain text with no background.

```rust
Text::new("Hello, world!").with_margin(6.0)
```

---

## Computing layout

```rust
let mut root = /* build your Node tree */;
root.compute_layout(parent_width, parent_height);
```

This runs two passes:

1. **Bottom-up** -- each node calculates its desired size from children and style.
2. **Top-down** -- concrete `rect` values are assigned from the root down.

After `compute_layout`, read `node.rect` (and `node.children[i].rect`) for
positions and sizes to use in hit-testing or custom rendering.

---

## Limitations

- No text wrapping or multi-line support
- No absolute positioning (`position: absolute`)
- No nested flex axes (e.g. `flex-wrap`)
- Adequate for panels, toolbars, and form layouts; not a full CSS engine
