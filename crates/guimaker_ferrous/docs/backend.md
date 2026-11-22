# The FerrousEngine backend

`guimaker_ferrous` is the only GUIMaker crate that knows how to draw. It takes
the engine-agnostic display list produced by `guimaker` and maps it onto
`ferrous_ui_render::GuiBatch`. It adds no layout, no theme and no drawing
policy of its own.

## Coordinates and colour

Every logical coordinate is multiplied by the frame `scale`:

| Display list | GuiBatch |
|---|---|
| `rect.x/y/width/height` | `x * scale`, `y * scale`, `w * scale`, `h * scale` |
| per-corner radii | `radii[i] * scale` |
| border width | `width * scale` |
| shadow offset, blur, spread | scaled component-wise |
| font size | `style.size * scale` |
| clip rect | scaled to physical pixels |

`guimaker::theme::Color` stores **linear-light** channels with straight alpha,
so `Color::to_linear()` is passed to the batch unchanged. The adapter never
re-encodes to sRGB; the renderer owns the final output transfer.

`encode` returns early when the viewport has no area or the scale is not finite
and positive, so a degenerate frame cannot emit NaN geometry.

## Command mapping

| `DrawCommand` | Engine call | Notes |
|---|---|---|
| `Rect` | `rect_radii` | per-corner radii scaled |
| `Border` | `border` | width in `uv0.x`, scaled |
| `Shadow` | `shadow` | one call per command, so a rect can carry several shadows |
| `Gradient` | `push_quad(GuiQuad)` | two-stop GPU gradient (`GRADIENT_BIT`), `GRADIENT_V_BIT` for a vertical angle, `GRADIENT_RADIAL_BIT` for radial |
| `Text` | `draw_text_baseline` or per-glyph `push_text` | see *Text* below |
| `Image` | `reserve_texture_slot` + textured `push_quad` | `TextureMap` resolves `source: u64`; fit changes the destination rect or the UVs |
| `Icon` | `push_svg` | `ferrous_svg::SvgMesh::from_str` subset; plain rectangle for unsupported paths |
| `ClipPush` | `push_clip` | rectangular |
| `ClipPop` | `pop_clip` | |
| `LayerPush` | `begin_layer` + alpha multiplication | barrier plus opacity fallback |
| `LayerPop` | `begin_layer` + alpha restore | closes the painter-order barrier |
| `BackdropBlur` | skipped | no tint in the command; see fallbacks |

Every primitive the adapter emits is stamped with the GUIMaker `NodeId` in its
`node_id` field (`GuiQuad`/`TextQuad`), so a later host can correlate GPU
picking with the view tree.

### Gradients

The pinned shader resolves exactly two colours. The adapter uses the first and
last stop and ignores the intermediate stops. A linear gradient is horizontal
by default; an angle whose sine dominates its cosine is mapped to the vertical
flag. A gradient with fewer than two stops is drawn as a solid first stop.

### Images

`TextureMap` holds `Arc<Texture2d>` handles keyed by the display list's
adapter-defined `u64` source id. The mapping owns no upload or reload logic.

- `Fill` uses the whole destination rect and the full UV square.
- `Contain` keeps the source aspect ratio and centres the image inside the
  command rect.
- `Cover` keeps the destination rect and crops the UVs symmetrically.

Rounded image corners are carried by the textured `GuiQuad` radii. A missing
source is skipped: the stateless `encode` entry point has no registry, and a
host that wants images registers them in `GuiMakerRenderer::textures_mut()`.

`GuiBatch` allows at most 16 texture slots per batch (`MAX_TEXTURE_SLOTS`); a
frame that needs more must split its batches.

### Icons

`ferrous_svg` at the pin is a stub: `SvgMesh::from_str` recognises `<rect` and
`<circle` and emits a placeholder triangle for everything else. The adapter
accepts only the two recognised forms, normalises the mesh vertices into the
icon rectangle and sets a matching bounding box for damage. Any other path
draws a plain filled rectangle in the icon colour. A real SVG tessellator is
the engine gap lane's job, not this crate's.

### Layers

`GuiBatch::begin_layer` is a painter-order barrier (a new segment); it has no
opacity. The adapter therefore keeps an opacity stack and multiplies the alpha
of every colour it emits while a layer is open. This is a straight-alpha
multiply, which is the documented fallback until the renderer grows a real
off-screen opacity layer.

## Text

`FerrousTextSystem` implements `guimaker::text::TextSystem` over
`ferrous_font::LayoutData`:

- advances, `kern` tables and `GPOS` pair kerning are the font's;
- tracking is inserted between characters, never after the last one;
- wrapping is greedy at word boundaries, with a character break only for a word
  wider than the line;
- line boxes use the core's `TextStyle::ascent()` and the requested
  `line_height`, so the display list's line geometry is independent of the face.

Faces are selected by `(FontFamily, FontWeight)`. `BundledGlyphs` returns the
engine's `Roboto-Regular` for every request; `FerrousTextSystem::with_source`
accepts a `GlyphSource` that returns the real per-weight files. A source that
has no face for a weight falls back to its `Sans` regular face.

The painter uses the run's shaped `Line` byte ranges and `TextStyle`:

- a run with no spans and no tracking uses `GuiBatch::draw_text_baseline`, so
  the engine applies its atlas cell and kerning;
- a run with spans or tracking is emitted one visible glyph at a time, using
  `BaselineGlyphs` (`advance`, `kerning`, `cell_bounds`, `uv`) and the span
  colour for that byte offset.

Emphasis flags (`strong`, `mono`, `link`) do not switch faces in this lane: the
host supplies one `DrawContext::font`. Span colour is always applied.

### Known text limitations

- `Ellipsis::Middle` degrades to an end marker when shaping, because the display
  list carries the original string and not a shortened display string. Layout
  width is still clamped.
- The adapter draws `Line::start..Line::end`. When clamping removed glyphs, the
  line's byte range is shortened to the visible prefix; the ellipsis glyph is
  present in the `TextShape` for measurement and caret work but cannot be drawn
  until the display list carries the shortened string.
- A run with no spans and no tracking draws in white (`Color::WHITE`), the only
  colour the display list leaves available in that case. The core's frame
  builder normally emits a span for the resolved text tone.

## Documented fallbacks at the pin

The backend targets FerrousEngine `dd382bd`, which has:

- rectangular scissor clips only (`push_clip`/`pop_clip`);
- one soft shadow per `GuiBatch::shadow` call (the adapter issues one call per
  `Shadow` command, so several shadows per rect work);
- a two-stop gradient shader;
- no backdrop blur;
- the `ferrous_svg` stub.

Therefore:

| Capability | Fallback |
|---|---|
| rounded clips | `push_clip(rect)`; the radii are ignored until the engine ships a rounded clip shape |
| gradient borders | not requested by the display list today; a plain `Border` is drawn |
| more than two gradient stops | first and last stop |
| arbitrary gradient angle | horizontal or vertical (the dominant axis) |
| backdrop blur | the command is skipped: it carries no tint and the pin has no blur pass |
| layer opacity | alpha multiplied per primitive (see *Layers*) |
| unsupported SVG icon path | filled rectangle in the icon colour |
| missing image texture | the image is skipped |
| `Ellipsis::Middle` | end marker (see *Known text limitations*) |

When the engine gap lane lands, the adapter switches each row to the new batch
API and deletes the fallback. The public GUIMaker API does not change.

## Dependency note

`DrawContext` lives in `ferrous_engine`, so this crate depends on the engine
crate to expose the requested `encode` signature. The GUIMaker boundary script
currently classifies `ferrous_engine` as an application crate that GUIMaker
layers may not use. The reviewer must either allow `guimaker_ferrous ->
ferrous_engine` or split the host adapter into a crate that owns that edge; the
mapping itself does not need anything beyond `GuiBatch`, `Font`,
`ferrous_ui_render`, `ferrous_font`, `ferrous_assets` and `ferrous_svg`.
