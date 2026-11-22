# Responsive layout: containers, fluid values and the sweep

Phase 3 makes every screen 100 % responsive. The rule is the same one CSS
container queries encode: **a node adapts to the width it actually has**, which
is its parent's content width, not the window's. The root's container is the
viewport. This is what lets the same card be comfortable in a 1920 px window and
correct inside a 320 px inspector pane.

## Breakpoints

```
Xs < 480 <= Sm < 768 <= Md < 1024 <= Lg < 1440 <= Xl
```

The thresholds are data: `Theme::breakpoints` (`Breakpoints { sm, md, lg, xl }`)
is serialisable, so an app can tune them in a theme file and a designer can edit
them. `Breakpoints::class(width)` maps a container width to a `Breakpoint`.

## `resp!` and `Responsive<T>`

Any responsive modifier takes a `Responsive<T>`; a plain token converts into a
constant one, so existing views read the same.

```rust
use guimaker::prelude::*;

let view: Element<()> = Element::column()
    .pad(resp![Xs: Spacing::S4, Md: Spacing::S6, Lg: Spacing::S8])
    .gap(resp![Xs: Spacing::S3, Lg: Spacing::S6])
    .direction(resp![Xs: Direction::Column, Lg: Direction::Row])
    .hidden_below(Breakpoint::Sm);
```

`resp![...]` picks, for a container width, the value of the largest declared
class at or below that width. A value declared from `Md` upwards also applies
below `Md` (there is nothing smaller to choose), so a single-entry `resp!` is a
constant.

Modifiers that accept a responsive value: `pad`, `pad_x`, `pad_y`, `gap`,
`align`, `justify`, `direction`, `width`, `height`, `min_width`, `min_height`,
`max_width`, `max_height`, `radius`, `columns`, `scroll_axes`. `hidden_below` and
`hidden_above` are the two visibility switches. `grow`, `shrink`, colours,
elevation, opacity and text content stay plain, because they are not functions
of the container.

## Fluid values

`clamp(min, fraction * container, max)`, with tokens as the bounds:

```rust
let view: Element<()> = Element::row()
    .fluid_width(Fluid::new(Spacing::S12, 0.25, Spacing::S24));

let title = text::<()>("Motioner")
    .text_role(TypeRole::Display)
    .fluid_text(FluidText::new(TypeRole::Heading, 0.02, TypeRole::Display));
```

The fraction is a layout ratio (like `SizeSpec::Fraction`), not a design value;
the bounds are tokens. `fluid_text` scales the size and keeps the line-height
ratio, so a title grows smoothly between the two roles instead of jumping at a
breakpoint.

`Columns::AutoFill(min)` grids and `max_width(ContentWidth::...)` columns do the
rest: a grid fits as many columns as the container allows, and a content column
stops growing at its cap. Centre a capped column with
`.align_self(Align::Center)` (or place it in a `center()` row).

## `priority`

Text never breaks a row's layout. `.priority(n)` (1 = gives way first, 9 = keeps
its size) sets the node's shrink weight and drops its content min-size, so a
label ellipsises while its sibling keeps its natural width:

```rust
Element::row()
    .child(text("A very long project name").priority(1).lines(1).ellipsis(Ellipsis::End))
    .child(text("Save").priority(9));
```

## Adaptive patterns

`guimaker::patterns` holds the five switches every desktop app repeats:

| Builder | Behaviour |
|---|---|
| `collapsing_sidebar(items)` | full sidebar at `Lg`+, 48 px icon rail at `Sm`, hidden below `Sm` |
| `sidebar_drawer(items)` | the overlay the narrow layout shows instead |
| `overflowing_toolbar(actions, more, budgets, more_from)` | actions past the width's budget become the caller's "More" element |
| `split_panes(a, b, min_a, min_b)` | side by side, stacked below `Md` |
| `dialog_to_sheet(title, body, actions)` | centred `Sm` card, full-screen sheet below `Sm` |
| `responsive_table(columns, rows)` | drops each column below the breakpoint it declares |

Each one is structural and token-only: wrap it in your shell, and the sweep
guards it.

## DPI

Layout is in logical pixels and every rectangle is snapped to the physical grid
at the end (`Rect::snap`), so a hairline border is exactly one device pixel at
1x, 1.25x, 1.5x and 2x. `tests/layout.rs` and `tests/responsive.rs` assert the
snapping at the four scales.

## The sweep

`guimaker::sweep` paints a view at ten widths (360 to 3840) x four scales and
checks every case:

- **overflow**: a child escapes its parent's rectangle when the parent does not
  clip or scroll;
- **overlap**: two siblings that are not in a `stack` intersect (wrapped flow
  included);
- **unsnapped**: an edge is not on the physical pixel grid;
- **offscreen**: the root is wider than the viewport.

```rust
use guimaker::sweep::sweep;

let report = sweep(&my_view, &Theme::dark());
assert!(report.is_clean(), "{}", report.summary());
```

`report.summary()` prints `width @ scalex: pass` per case plus a total, which is
what a failing test shows. The tolerance is `1 / scale + 0.5` logical pixels, so
sub-pixel rounding from snapping is not reported.

The start-screen recipe and every pattern in `guimaker::patterns` pass all forty
cases; `tests/responsive.rs` enforces it. A new component is not done until it
does too.

## Cost

A view without a single responsive modifier resolves its styles once and lays
out in one pass, exactly as in phase 2. A responsive view needs a second pass
because a container's width is only known after its parent is laid out: the
first pass uses the previous frame's rectangles (the viewport on a cold frame),
the engine re-resolves the styles against the fresh widths and runs the second
pass only if a value actually moved. `LayoutStats::responsive_passes` reports 1
or 2.

On the start screen (1920x1080, scale 1.5, release) the warm full pass is
0.187 ms, a hover does no layout (0.010 ms for the paint) and a one-label
relayout is 0.036 ms, all under the 0.3 ms hover/typing budget.

### The cold frame's second pass (recorded, phase 4)

The room-temperature numbers hide the worst case: the **first** frame of a
responsive view costs more than every later one. Phase 3 measured the start
screen at 0.300 ms in phase 2 (non-responsive) and 0.497 ms after the
responsive pass landed (phase 3), with the same 0.26-0.48 ms spread across the
40-case sweep. The cause is structural, not a regression:

1. the first pass has no container rectangles yet, so it resolves every
   responsive value against the viewport or the previous frame's (empty)
   rectangles;
2. the containers exist after that pass, so the engine re-resolves the styles
   against their real widths and runs a second pass, but only when a value
   moved (`LayoutStats::responsive_passes` reports 2);
3. every later frame reuses the previous widths and stays at one pass.

That extra pass is inside the phase 3 acceptance and is deliberate: paying it
once per window/scale/theme change is cheaper than a constraint solver on
every frame. Revisit it in phase 9B if the start screen's first-frame budget
(the prompt 17 gate: first frame under ~0.5 s end to end) needs the 0.2 ms
back; the candidates are caching the resolved widths across the first two
passes and seeding the viewport-width rule per breakpoint class. Do not
"optimise" it by dropping the second pass: a fluid value that only moves at a
container width would then be wrong until the next resize.
