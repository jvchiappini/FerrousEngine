# Pure2D text

FerrousEngine renders 2D text with MSDF glyph atlases produced by
`ferrous_font`. The engine bakes one atlas per font face, then draws one GPU
instance per glyph through the Pure2D pipeline. Nothing is rasterised on the
CPU at draw time: layout only advances a pen and emits quads.

Text layout lives in `ferrous_font::layout` and is pure CPU code, so the same
arithmetic measures text without a GPU. `ferrous_2d` re-exports it under the
`*2d` names (`TextStyle2d`, `GlyphLayout2d`, `TextMetrics`, ...).

## Coordinate contract

* World space: origin at the target centre, **Y up**, 1 unit = 1 logical pixel
  in the default Pure2D camera.
* Text is laid out with the pen starting at `x = 0` on the baseline `y = 0`.
* `size_px` is the em size in pixels. Advances, kerning, ascender, descender and
  line gap are font metrics in em units multiplied by `size_px`.
* Each glyph quad covers the atlas cell, `[-0.3, 1.3]²` em around the glyph's
  pen position (`FontAtlas::cell_bounds`). The MSDF shader clips the actual ink,
  so quad bounds are not ink bounds. `GlyphLayout2d::ink` *is* ink bounds, read
  from the `glyf` header.
* `TextAlign::Left` keeps the pen origin at the start of the line;
  `Center` / `Right` shift the finished line by `-advance / 2` / `-advance`
  **on that line**. Each line of a block is aligned on its own.
* Text depth is the `z` translation of the transform passed to
  `draw_text_2d`. Every glyph of one call shares it.

## Style

`TextStyle2d` replaces the loose `size_px` + `align` pair:

| field | meaning |
|---|---|
| `size_px` | em size in pixels |
| `align` | per-line `Left`, `Center` or `Right` |
| `tracking` | extra advance per character, in **em units**, inserted **between** characters |
| `line_height` | multiplier of the font's natural line step `ascender - descender + line_gap` |

`TextStyle2d::new(size_px, align)` keeps the old defaults: no tracking, natural
line spacing. A non-finite or negative `size_px` is treated as zero, a
non-finite `tracking` as zero and a non-finite or non-positive `line_height` as
`1.0`, so layout never emits NaN geometry.

### Tracking

Tracking is applied after every character **except the last character of a
line**, so it is `(characters - 1) * tracking * size_px` in total. Whitespace
counts as a character, and tracking is applied on both sides of it. Because the
last character gets no trailing space, centred text stays centred.

### Line height and `\n`

A string is split on `\n`. Line 0's baseline is the transform origin; line `i`'s
baseline is `i * (ascender - descender + line_gap) * line_height * size_px`
**below** it (negative `y`). A trailing `\r` (Windows line ending) is not drawn
as a glyph. An empty string is one empty line.

## Drawing

```rust
use ferrous_2d::render::{TextAlign, TextStyle2d};
use glam::{Mat4, Vec3};

let font = renderer.register_font_2d(&bytes, ferrous_font::charset::ascii())?;
let style = TextStyle2d {
    size_px: 48.0,
    align: TextAlign::Center,
    tracking: 0.02,   // +2 % em between characters
    line_height: 1.2, // 20 % looser than the font's natural spacing
};
renderer.draw_text_2d_styled(
    font,
    "Hello\nworld",
    Mat4::from_translation(Vec3::new(0.0, 40.0, 5.0)),
    style,
    [1.0, 1.0, 1.0, 1.0], // linear RGBA
);
renderer.free_font_2d(font);
```

`draw_text_2d` and `draw_text_2d_with_gradient` are thin wrappers over
`draw_text_2d_styled` / `draw_text_2d_with_gradient_styled`; they keep the
single-line, no-tracking behaviour of earlier releases.

## Measurement

`measure_text_block_2d(font, text, style)` returns `TextMetrics` without drawing or
allocating glyph records:

| field | exact definition |
|---|---|
| `width` | the widest line's total advance, including kerning and tracking |
| `line_count` | `text.split('\n').count()`, at least 1 (an empty string is one line) |
| `height` | `(ascender - descender) + (line_count - 1) * line_step`, in pixels; zero when `size_px` is zero |
| `ink` | union of every visible glyph's ink box, in block-local pixels, Y up; empty when nothing draws |
| `lines` | one `TextLineMetrics` per line, top to bottom |

`line_step` is `(ascender - descender + line_gap) * line_height * size_px`.
`TextLineMetrics` carries the line's `advance`, its `baseline_y`, its own `ink`,
`quad_count` and `missing_glyphs`.

A line's `advance` includes kerning and tracking between its characters and any
trailing whitespace advance, but never a trailing tracking step.

## Per-glyph layout and drawing

`layout_text_2d(font, text, style, &mut out)` fills `out` with one
`GlyphLayout2d` per character (whitespace included) and returns the same
`TextMetrics` as `measure_text_block_2d`. Each entry carries:

| field | meaning |
|---|---|
| `ch` | the source character |
| `byte_range` | its byte range in the original string |
| `line` | zero-based line index |
| `x` | pen x on its baseline, after kerning, tracking and the line's alignment shift |
| `baseline_y` | the line's baseline (`0` for line 0, negative below) |
| `advance` | the glyph's own advance in pixels, excluding tracking |
| `quad` | atlas cell `[left, bottom, right, top]` relative to the glyph origin |
| `uv` | atlas UV rectangle |
| `ink` | absolute ink box in block space |
| `visible` | `false` for whitespace and for a missing glyph without `.notdef` |

`layout_text_2d` positions and `draw_text_2d_styled` renders the same
arithmetic; a test reads both paths back and compares the pixels.

`draw_glyphs_2d(font, &[GlyphDraw2d])` draws pre-laid-out glyphs with a
per-glyph `transform`, straight linear `color` and optional `gradient`. It is
the per-character animation entry point: mutate each entry's transform or
colour, then draw. With an identity transform for every glyph the result is
pixel-identical to `draw_text_2d_styled` for the same text and style, including
whitespace, which produces no quad.

## Kerning

Kerning comes from two sources, in this order:

1. **GPOS `PairPos`** (lookup type 2, formats 1 and 2, horizontal `xAdvance`
   only). Every type-2 lookup in the lookup list is read, regardless of which
   script or feature references it.
2. **`kern` format 0** (legacy TrueType), used only when GPOS does not cover
   the pair.

A GPOS-covered pair wins even when its adjustment is zero. Fonts such as Inter
ship only GPOS pair kerning; Roboto has both. Kerning is not applied across
whitespace.

## Font faces and weights

One registered face is one static TTF/OTF. Weights are separate files: register
`Inter-Regular.ttf`, `Inter-SemiBold.ttf` and `Inter-Bold.ttf` as three fonts
and pick the right `FontId2d` in the application. Variable-font axes are not
applied; a variable TrueType face is read at its default instance. CFF/CFF2
outlines are not supported.

`register_font_2d` returns a typed `FontError` instead of panicking:

* `FontError::Unsupported { reason }` for a valid font this parser cannot draw
  (CFF outlines, CFF2 variable fonts, no `glyf` table);
* `FontError::Parse { message }` for malformed bytes;
* `FontError::Atlas { message }` when the atlas cannot be built or uploaded.

## Ordering

Text joins the unified Pure2D painter-order command stream, so it interleaves
with shapes and sprites by `z`. For equal `z` the total order is:

1. shapes (submission order),
2. sprites (ascending texture id, then submission order),
3. text (submission order).

Consecutive glyphs of the same atlas collapse into one draw call; an
interleaved shape, sprite or a different font splits the run.

## Shader sharing

`assets/shaders/msdf_common.wgsl` holds the median decode, atlas-cell size and
Chlumsky `screenPxRange` opacity helpers. Both the GUI text pipeline
(`assets/shaders/text.wgsl`) and the Pure2D text pipeline
(`assets/shaders/text_2d.wgsl`) concatenate that file at pipeline creation, so
the two paths cannot drift apart.

The Pure2D shader derives the quad size in screen pixels from the instance's
model basis and the ortho projection, and takes the encoded distance range from
`FontAtlas::px_range` through the instance. Scaling, rotating and zooming
therefore keep a constant screen-space AA width.

## Limitations

* **No shaping engine.** There is no HarfBuzz: no ligatures, no mark
  attachment beyond what the font places in its own glyphs, no complex-script
  reordering and no bidi. Kerning is the only GPOS feature applied.
* GPOS support is limited to `PairPos` formats 1 and 2 and the horizontal
  `xAdvance` value; placement, vertical advances, device tables and contextual
  lookups are ignored.
* Missing characters use `.notdef` when the font has a non-empty glyph 0;
  otherwise they are skipped and counted in `TextLineMetrics::missing_glyphs`.
* Straight-alpha blending only, like the shape and sprite pipelines.
* Gradient fills evaluate in world space (see [gradients.md](gradients.md));
  use `Renderer::draw_text_2d_with_gradient_styled` or
  `Pure2dHarness::draw_text_styled_with_gradient`.
* Multi-view scissor segmentation does not split text yet. The multi-view fast
  path only uploads shapes, so text and sprites are not drawn in that mode
  (pre-existing limitation, not specific to text).

## Back to the index

See the [2D subsystem overview](README.md) for the rest of the Pure2D API.
