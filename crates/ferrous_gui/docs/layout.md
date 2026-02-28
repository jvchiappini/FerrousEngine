<!--
Overview of the layout system used by `ferrous_gui`.
-->


# Layout system

The crate provides a lightweight layout engine inspired by CSS flexbox.
It is primarily used within the editor and other panels to position
widgets, but it can be employed anywhere a tree of rectangular nodes is
desirable.  The implementation prioritises simplicity over feature
completeness; only the necessary primitives are provided.

## Fundamental types

- **`Rect`** – represents a rectangle with `x`, `y`, `width`, and
  `height`.
- **`RectOffset`** – margin or padding, with distinct values for each side.
  Constructors such as `RectOffset::all(v)` create a uniform inset.
- **`Units`** – measurement units used by `Style` properties. Possible
  values are `Px(f32)`, `Percentage(f32)` and `Flex(f32)`.
- **`Alignment`** – how children are positioned within their parent
  (`Start`, `Center`, `End`, `Stretch`).
- **`DisplayMode`** – layout flow type: `Block`, `FlexRow`, or
  `FlexColumn`.
- **`Style`** – aggregates margin, padding, size, alignment and display
  mode rules for a node.
- **`Node`** – represents a single element in the layout tree. It contains
  a `Style`, optional text, background colour, font settings and a list of
  child nodes. Computed rectangles are stored in `node.rect`.

## Working with nodes

Nodes are typically constructed using builder methods that make it easy to
specify common layout attributes:

```rust
let mut root = Node::new()
    .with_display(DisplayMode::FlexColumn)
    .with_padding(10.0)
    .add_child(
        Node::new().with_size(Units::Px(100.0), Units::Px(30.0))
    );
```

Other helpers include `.with_margin(...)`, `.with_alignment(...)`,
`.set_text(...)`, `.set_background(...)`, and so forth.

Once the tree is assembled, invoke `compute_layout(parent_width, parent_height)`
to perform two passes:

1. **Bottom‑up pass** calculates each node’s desired size based on its
   children and style rules.
2. **Top‑down pass** assigns concrete `rect` values beginning from the
   root, using the supplied parent dimensions (usually the viewport size).

```rust
root.compute_layout(1024.0, 768.0);
```

The resulting `rect` fields can then be used for hit testing or rendering
quad backgrounds and text.

## Limitations

The layout subsystem does not support advanced features such as wrapping,
absolute positioning, or nested flex axes beyond the three display modes
listed above.  It is adequate for simple form‑style UIs and editor
panels, but heavier weight applications may prefer a full CSS engine.

