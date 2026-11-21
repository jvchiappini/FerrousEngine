# View tree, layout and explanations

Phase 2 is the heart of the library: a declarative tree, a styling API that
takes tokens, taffy 0.7.7 layout memoised by a content hash, input routing, a
flat display list, damage, and the inspect/explain/debug data prompt 19 reads.

## Build a view

```rust
use guimaker::prelude::*;

let view: Element<()> = Element::column()
    .key("start")
    .pad(Spacing::S6)
    .gap(Spacing::S4)
    .align(Align::Stretch)
    .bg(ColorRole::Bg)
    .child(text("Motioner").text_role(TypeRole::Title))
    .child(
        Element::row()
            .key("actions")
            .gap(Spacing::S2)
            .align(Align::Center)
            .child(text("New project").pad_x(Spacing::S4).bg(ColorRole::AccentFill))
            .child(Element::spacer()),
    );
```

Containers: `column`, `row`, `stack`, `grid(Columns)`, `center`, `spacer`,
`divider`, `scroll(ScrollAxes)`, `wrap`, `split`, `aspect`, `canvas`, `image`.
Modifiers are token-typed: `pad`, `pad_x`, `pad_y`, `gap`, `align`, `justify`,
`grow`, `shrink`, `width`/`height(SizeSpec)`, `min_width`, `max_width` (a
`Spacing` or a `ContentWidth`), `bg`, `border`, `border_width`, `radius`,
`elevation`, `opacity`, `clip`, `tone`, `text_role`, `lines`, `ellipsis`,
`hidden`, `disabled`, `key`, `tooltip`, `on_click`. `SizeSpec::Px` is the only
raw-pixel escape hatch and the design lint reports it.

## One frame

`Ui::paint` is a pure function of the view, the keyed store, the theme, the
viewport, the scale and this frame's input:

1. build the arena (skipped when the view hash is unchanged);
2. lay out on taffy, memoised by a content hash (unchanged subtrees are not
   re-synced at all);
3. route the queued pointer events through the laid-out tree;
4. paint a flat display list, reusing each node's commands when its hash and
   interaction state are unchanged;
5. return the damage list and the messages.

A still frame reuses the previous display list, reports no damage and performs
zero heap allocations (there is a counting-allocator test for it).

## Layout time

The start-screen recipe (`guimaker::recipe::start_screen`) at 1920x1080 and
scale 1.5, release, divan medians (phase 3 numbers; `cargo bench -p guimaker
--bench layout`):

| Workload | Median |
|---|---|
| full layout pass (warm engine, viewport change) | 0.187 ms |
| hover frame (pointer move, one damaged node) | 0.010 ms |
| relayout after one changed label | 0.036 ms |
| cold frame (build + layout + paint) | 0.497 ms |

The hover/typing budget is 0.3 ms CPU per frame; the full pass is 0.187 ms and
a hover does no layout work at all. The recipe is responsive now (65 nodes), so
a viewport change re-resolves the tokens and a fluid value that moved costs a
second taffy pass; a cold frame pays that pass once (the containers are only
known after the first pass), which is why that is the one row above 0.3 ms.
Phase 2 (64 nodes, no responsive values) measured 0.164 / 0.0075 / 0.023 /
0.300 ms on the same bench, so the per-frame paths stay inside the budget.

The full layout at each sweep width is printed by `cargo test --release
-p guimaker --test responsive layout_cost_by_width -- --nocapture`. The 8 192-node
flat tree lays out in 3.885 ms, against 6.328 ms for the engine's full layout
after the move and 6.545 ms before it.


## Inspect

`Ui::inspect()` returns a stable, indented dump of the laid-out tree:

```text
column start rect=0.0,0.0,1920.0,1080.0 pad=32.0/32.0/32.0/32.0 gap=24.0 bg=role text=body
  row start/header rect=32.0,32.0,1856.0,38.0 gap=12.0 text=body
    text start/header/brand/title rect=32.0,32.0,63.3,20.0 text=title text="Motioner"
```

It is the first thing to read when a layout looks wrong. Snapshots at three
sizes × two scales live in `tests/snapshots/`.

## Explain

`Ui::explain("sidebar")` prints the winning constraint in plain English, the
overflow amount, the siblings it overlaps and a concrete fix:

```text
`sidebar` is 96 x 16 px: width from the s24 spacing token; height from its
content. It is 48 px wider than `shell`'s 48 px content width, so it overflows
it. Fix: a fraction width with a minimum (`width(Fraction(0.25)).min_width(S3)`),
or hide it below the breakpoint.
```

## Debug data (prompt 19)

`Ui::debug()` returns a `FrameDebug`: every node's stable id and path, rect,
resolved tokens, cache state, repaint reason, and the number of draw commands it
produced. Every `DrawCommand` also carries its `node`, so the frame debugger can
attribute a draw to a node without a lookup. No panel is part of this phase.

## Responsive

Every modifier that depends on the space a node has takes a
[`Responsive<T>`](responsive.md) value: `pad(resp![Xs: S4, Lg: S8])`,
`direction(resp![Xs: Column, Md: Row])`, `hidden_below(Sm)`, `columns(...)`.
The breakpoint is the container's width (the parent's content width), so a card
adapts to its pane, not to the window. Fluid widths and type sizes
(`Fluid`, `FluidText`) clamp a fraction of the container between two tokens, and
`.priority(n)` lets a label ellipsise before its siblings shrink. The adaptive
patterns live in `guimaker::patterns`, and `guimaker::sweep` renders a view at
ten widths and four scales and fails on overflow, overlap, an off-grid edge or
an off-screen root.

## Hit testing

The spatial index is a uniform grid over the laid-out interactive rectangles,
with clips baked in. It is rebuilt only when layout changes; a query reads one
cell and returns candidates front-to-back. The last hit and its rect are
cached, so a pointer move inside the same element does one rectangle test. The
index moved here from `ferrous_ui_core`; the engine keeps only the draw side.