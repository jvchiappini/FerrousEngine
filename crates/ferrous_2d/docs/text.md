# Pure2D text

FerrousEngine renders 2D text with MSDF glyph atlases produced by
`ferrous_font`. The engine bakes one atlas per font, then draws one GPU
instance per glyph through the Pure2D pipeline. Nothing is rasterised on the
CPU at draw time: layout only advances a pen and emits quads.

## Coordinate contract

* World space: origin at the target centre, **Y up**, 1 unit = 1 logical pixel
  in the default Pure2D camera.
* Text is laid out with the pen starting at `x = 0` on the baseline `y = 0`.
* `size_px` is the em size in pixels. Advances, kerning, ascender, descender and
  line gap are font metrics in em units multiplied by `size_px`.
* Each glyph quad covers the atlas cell, `[-0.3, 1.3]²` em around the pen
  position (`FontAtlas::cell_bounds`). The MSDF shader clips the actual ink, so
  quad bounds are not ink bounds.
* `TextAlign::Left` keeps the pen origin at the start of the line;
  `Center` / `Right` shift the finished line by `-advance / 2` / `-advance`.
* Text depth is the `z` translation of the transform passed to `draw_text_2d`.
  Every glyph of one call shares it.

## API

```rust
use ferrous_font::TextAlign;
use glam::{Mat4, Vec3};

let font = renderer.register_font_2d(&bytes, ferrous_font::charset::ascii())?;
renderer.draw_text_2d(
    font,
    "Hello, world",
    Mat4::from_translation(Vec3::new(-200.0, 40.0, 5.0)),
    32.0,              // em size in px
    TextAlign::Center, // relative to the transform
    [1.0, 1.0, 1.0, 1.0], // linear RGBA
);
renderer.free_font_2d(font);
```

`register_font_2d` bakes the atlas once (MSDF generation is the expensive
part); `draw_text_2d` reuses an internal layout scratch buffer. In steady state
a frame only uploads its own `TextInstance`s into a geometrically grown buffer.

The headless harness mirrors the API:

```rust
let font = harness.register_font(&bytes, ferrous_font::charset::ascii())?;
harness.draw_text(font, "Title", Mat4::IDENTITY, 64.0, TextAlign::Left, color);
```

`measure_text_2d` (and `harness.measure_text`) returns
`ferrous_font::LineMetrics` without drawing.

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

* Single line only. A `layout_block` would call `layout_line` per line and
  advance the baseline by `(ascender - descender + line_gap) * size_px`.
* `kern` format 0 only. GPOS kerning is detected (`FontParser::has_gpos_kerning`)
  but not applied.
* Missing characters use `.notdef` when the font has a non-empty glyph 0;
  otherwise they are skipped and counted in `LineMetrics::missing_glyphs`.
* Straight-alpha blending only, like the shape and sprite pipelines.
* Multi-view scissor segmentation does not split text yet. The multi-view fast
  path only uploads shapes, so text and sprites are not drawn in that mode
  (pre-existing limitation, not specific to text).
