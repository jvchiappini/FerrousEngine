# UI drawing effects

This page covers the generic effects a design system needs from the UI batch
and shader: per-corner radii, stacked soft shadows, rounded clip masks,
gradient borders, icon paths and backdrop blur. Each section states the public
API and the exact GPU contract.

All coordinates are physical pixels, colours are straight RGBA (`[f32; 4]`),
and radii are ordered `[top-left, top-right, bottom-left, bottom-right]`.

The drawing helpers take small specification structs (`BorderSpec`,
`GradientBorderSpec`, `ShadowSpec`, `LineSpec`, `TexturedRectSpec`,
`BackdropBlurSpec`) instead of 7-10 positional arguments.

---

## Per-corner radii

`GuiQuad::radii` is `[f32; 4]` and is uploaded unchanged
(`crates/ferrous_ui_render/src/gpu_types.rs`). `assets/shaders/gui.wgsl`
selects the radius per quadrant in `sdf_rounded_rect` and every rounded
primitive forwards the array:

- `GuiBatch::rect_radii` - solid fill.
- `GuiBatch::border` - outline (`crates/ferrous_ui_render/src/gui_batch.rs:1103`).
- `GuiBatch::shadow` - soft shadow (`.../gui_batch.rs:1199`).

The packing is pinned by
`per_corner_radii_are_already_packed_for_every_rounded_primitive`.

---

## Multiple soft shadows

Every `GuiBatch::shadow` call appends one `SHADOW_BIT` quad, so repeated calls
compose in painter order: draw the wide ambient shadow first, the tight key
shadow second, then the surface. Nothing is merged or deduplicated on purpose;
a translucent shadow must composite independently. Pinned by
`multiple_soft_shadows_compose_in_paint_order`.

---

## Rounded clip masks (real SDF mask)

`ClipShape` (`crates/ferrous_ui_render/src/gpu_types.rs`) carries a rectangle
and per-corner radii. `GuiBatch` keeps a parallel clip stack and exposes:

- `push_clip_rounded(rect, radii)` (`.../gui_batch.rs:1422`)
- `push_clip_shape(ClipShape)`
- `pop_clip()` closes either kind.

`DrawSegment` carries the shape in `clip` and sets the segment-level
`ROUNDED_CLIP_BIT`. A plain rectangular clip stays on the scissor path.

### GPU contract

The renderer packs one uniform per draw segment (`SegmentUniforms`: clip rect,
radii, params) into a dynamic-offset uniform buffer and binds it at **group 2
binding 0** (`crates/ferrous_ui_render/src/gui_renderer.rs:1412`,
`.../gui_renderer.rs:665`). `assets/shaders/gui_clip_common.wgsl` evaluates the
rounded-rectangle SDF against the fragment's framebuffer position and returns a
one-pixel anti-aliased coverage mask. The quad, text/icon and SVG shaders
multiply their alpha by that mask, so the clip applies to quads, text and
images alike.

The axis-aligned `set_scissor_rect` is still applied first as a cheap early
reject (`.../gui_renderer.rs`, segment loop); the SDF cuts the corners inside
that rectangle.

### Nested clips

`ClipShape::intersect` keeps the shape exact when the inner clip is fully
contained in the outer one; otherwise the intersection is not a rounded
rectangle and it degrades to the rectangular bounds with zero radii.

### Test

`rounded_clip_cuts_the_corner_and_keeps_the_centre`
(`crates/ferrous_renderer/tests/gui_effects.rs`) renders a 80x80 quad under an
80x80 clip with 24 px corners. The corner pixel (10, 10) reads
`alpha < 8` (transparent), the centre (48, 48) reads `alpha > 220`, and a pixel
outside the scissor is transparent.

---

## Gradient borders

A gradient border is a normal border whose base colour comes from the two-stop
gradient. `gui.wgsl` resolves `GRADIENT_BIT` before it applies the outline SDF,
so only the batch helper was needed.

- `GRADIENT_BORDER_BITS = BORDER_BIT | GRADIENT_BIT`.
- `GuiBatch::gradient_border(GradientBorderSpec)` (`.../gui_batch.rs:1128`)
  packs the start stop in `color`, the end stop in `color_b`, the width in
  `uv0.x`, and adds `GRADIENT_V_BIT` for a top-to-bottom ramp. The default is
  left-to-right.

Pinned by `gradient_border_packs_both_flags_and_the_colour_ramp`.

---

## Vector icons (real path parser)

`ferrous_svg::SvgMesh::from_str` (`crates/ferrous_svg/src/lib.rs:88`) is a real
vector path parser and tessellator:

- `<path d="...">` elements, including several paths in one document;
- raw SVG path `d` data;
- `<rect>`, `<circle>`, `<ellipse>`, `<polygon>` and `<polyline>`, reduced to
  path commands and parsed by the same `svgparser` tokenizer
  (`.../lib.rs:344`);
- Lyon fill tessellation with the non-zero rule (`.../lib.rs:444`), and a
  bounding box computed from the emitted vertices.

Unsupported input is a typed error, not a guessed placeholder triangle.

### Test

`a_parsed_icon_path_is_filled_as_a_real_shape`
(`crates/ferrous_renderer/tests/gui_effects.rs`) uploads the diamond
`M 10 50 L 50 10 L 90 50 L 50 90 Z` through the GUI SVG pipeline. The pixel at
(15, 50) is inside the diamond but outside the old placeholder triangle, and it
reads filled; a pixel outside the diamond stays transparent.

### Adapter note

`crates/guimaker_ferrous/src/mapper.rs:638` still gates icons on
`<rect`/`<circle` before calling `SvgMesh::from_str`; that guard must be
removed (lane A/B owns the crate) so a real `<path>` icon reaches the new
parser. The mapper's border/shadow call sites also need the new spec structs
(see the lane report).

---

## Backdrop blur (real two-pass blur)

`GuiBatch::backdrop_blur(BackdropBlurSpec)` (`.../gui_batch.rs:1171`) asks for a
blur behind a rounded rect and packs `BACKDROP_BLUR_BIT`: `color` is the tint,
`uv0.x` the requested radius, `radii` the per-corner radii.

### GPU contract

The caller supplies the scene colour view
(`crates/ferrous_ui_render/src/gui_renderer.rs:774`,
`GuiRenderer::set_backdrop_source`) - for example a `SceneTarget2d::view()`.
`ferrous_ui_render` does not depend on `ferrous_renderer`; the view is just a
sampled colour view. The engine facade forwards it through
`ferrous_renderer::Renderer::set_gui_backdrop_source` (and
`UiPass::set_backdrop_source`).

At quarter resolution the renderer encodes:

1. a horizontal nine-tap Gaussian from the scene view into `blur_h`
   (`assets/shaders/backdrop_blur.wgsl`);
2. a vertical nine-tap Gaussian from `blur_h` into `blur_v`.

Both passes run before the GUI pass and write only the quarter-resolution
targets (`.../gui_renderer.rs:1627`). The GUI quad pipeline samples `blur_v`
with a linear sampler at group 2 binding 1/2, so the result is upsampled for
free at the rounded rect. Cost is bounded by the quarter-resolution target and
the fixed kernel, independent of the requested radius.

If no source has been supplied (`clip_params.y == 0`), `gui.wgsl` falls back to
the translucent tint masked by the rounded-rect SDF and ignores the radius.
That is the documented degraded path; it is never silently used when a source
is bound.

### Tests and measured cost

`backdrop_blur_samples_a_blurred_scene_not_a_tint`
(`crates/ferrous_renderer/tests/gui_effects.rs`) puts red on the left and blue
on the right of a 128x128 scene and draws a 64x64 blur rect across the split
with a fully transparent tint. At the boundary the output has both red and blue
channels and no green from the clear colour, which a tint (or a sharp sample)
cannot produce.

The same test times the two blur passes with `device.poll(Wait)`:

- 128x128 scene -> 32x32 blur, **0.76-1.14 ms/frame** on the headless adapter
  of this reference host (debug build; the test prints the value on every run).

The test fails above 50 ms so a regression cannot hide.

The command packing is pinned by `backdrop_blur_packs_the_flag_and_the_tint`.
