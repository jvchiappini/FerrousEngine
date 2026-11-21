# Text in GUIMaker

The core never rasterises, opens a font file or touches a glyph atlas. It asks
a `TextSystem` for a `TextShape` — line boxes, glyph advances, the shaped width
and height — and lays the node out from that. `guimaker_ferrous` implements the
trait over `ferrous_font` (kerning, `GPOS`, real weights) in phase 5.

## The trait

`guimaker::text::TextSystem` has two methods:

- `shape(text, style, max_width)` wraps at `max_width` when it is positive;
- `shape_clamped(text, style, max_width, max_lines, ellipsis)` applies a line
  clamp and `Ellipsis::None`, `End` or `Middle` (the middle form keeps both ends
  of a path).

`TextStyle` is the resolved type style (size, weight, line height, tracking,
family) plus `tabular` for fixed-width digits. `TextShape` carries `Line` and
`Glyph` records, so a future caret and selection layer can reuse the same
data.

## The metrics table

`guimaker::text::MetricsTextSystem` is the deterministic, GPU-free
implementation used by layout tests and benches. It reads
`src/text/metrics.rs`, a table of real **Inter Regular** advances for printable
ASCII plus the ellipsis, en/em dash and right quote, in font units at
`UPEM = 2048`, with the real ascender and descender. The table is generated
from the font file, not guessed:

```text
python scripts/gen_guimaker_metrics.py <path>/Inter-Regular.ttf
```

Tabular digits use a fixed advance so timecode and numeric fields do not shift;
the adapter shapes the real `tnum` feature in phase 5. An unknown glyph falls
back to half an em, which makes a missing table entry visible in layout tests
instead of silently approximating.

## What is tested

- real advances for a known pair (`AV`);
- word wrapping at a width;
- a two-line clamp with an end ellipsis;
- middle ellipsis on a long path;
- tabular digits;
- the one-empty-line shape for an empty run.

The `TextShape` height is `lines * line_height`; the baseline of line *n* is
`n * line_height + ascent`.
