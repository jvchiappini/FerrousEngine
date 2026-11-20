# Overdraw debug view (GUI / 2D)

A runtime, opt-in diagnostic that shows how many primitives were blended into
each pixel. It is a development tool, not part of the normal render path.

## What it shows

Every quad, glyph and icon is drawn with a diagnostic pipeline instead of the
normal colour pipeline. The fragment shader ignores the primitive's colour and
adds a fixed heat value to the target with **additive blending**
(`src = One`, `dst = One`):

```
out = current + vec4(0.02, 0.02, 0.0, 1.0)
```

A pixel that received `n` fragments therefore shows approximately `0.02 * n`
in the red and green channels, saturating at 1.0. The result is a heat map
where most of the screen is black or very dark and any region where UI layers
overlap (containers over backgrounds, shadows over fills, text over panels,
nested scissor regions) glows brighter. It does not rank *which* node caused
the overlap; it only measures how much work reached the target.

The heat value is `OVERDRAW_HEAT` in
`crates/ferrous_ui_render/src/pipelines/overdraw.rs`; the authoritative copy
lives in the WGSL constant of the same name in
`assets/shaders/gui_overdraw.wgsl`.

## How to enable it

The view is **default off**: `OverdrawConfig::default()` has
`enabled: false`, so goldens and normal frames are byte-identical.

1. Add the module to the crate root (one line, when the wiring lands):

   ```
   // crates/ferrous_ui_render/src/lib.rs
   pub mod overdraw;
   ```

   The implementation is already compiled through
   `crates/ferrous_ui_render/src/pipelines/mod.rs`, so this only exposes the
   façade in `crates/ferrous_ui_render/src/overdraw.rs`.

2. Store the pipelines next to the existing GUI pipelines. The two
   constructors reuse the existing bind-group/instance layouts:

   ```
   // GuiRenderer::new, next to transparent_pipeline (gui_renderer.rs:300-315)
   let overdraw = OverdrawView::new(
       &device,
       format,
       &layouts.quad_pipeline_layout,
       &layouts.text_pipeline_layout,
       sample_count,
       cache,
       OverdrawConfig::default(), // off
   );
   ```

   Keep it in the same struct field group as `opaque_pipeline` /
   `transparent_pipeline` / `text_pipeline` (`gui_renderer.rs:89-93`) and
   rebuild it in `GuiRenderer::set_format`/`resize` path
   (`gui_renderer.rs:505-515`) when `format` or `sample_count` changes.

3. In the draw loop (`GuiRenderer::render_with_timestamps`), choose the
   overdraw pipelines when `overdraw.config.is_active()`:

   - quads: bind `overdraw.quad` instead of `self.transparent_pipeline`
     (`gui_renderer.rs:1001`);
   - text: bind `overdraw.text` instead of `self.text_pipeline`
     (`gui_renderer.rs:1017`);
   - icons: same as text (`gui_renderer.rs:1034`);
   - SVG keeps `svg_pipeline` (not part of this view).

4. Expose the toggle through `Renderer2dConfig` in the orchestrator (for
   example `overdraw: OverdrawConfig`) and set
   `Render2dConfig.overdraw.enabled = true` for the diagnostic frame only.

The pipelines use `depth_write_enabled: false` and
`depth_compare: Always`. They **must not** add a depth clear or a depth write;
the existing GUI pass already clears depth to 1.0 for early-z, and the overdraw
view simply ignores it. Do not sort or split the batch for this view: painter
order is what produces the overlap counts.

## How to read it

- Black: one (or zero) fragment. The normal case for most of the screen.
- Dark red/green: a few fragments. Thin outlines, AA edges and text typically
  land here.
- Brighter or yellow-ish: many fragments stacked in the same pixel. Look for
  full-screen backdrops behind panels, box shadows painted over their own
  fill, unclipped scroll content, or effects that run over the whole target
  instead of the layer's bound.
- Saturated white in a channel: roughly 50 or more fragments
  (`0.02 * 50 == 1.0`). This is the region to optimise first.

The `alpha` channel also accumulates additively, so the target's alpha
saturates quickly; this is expected and is not part of the reading.

`OverdrawCounters` records the CPU-side work routed through the view
(`segments`, `primitives`) so a harness can assert the view actually ran. It
does not count GPU fragments.

## Layer bounds

`Layer2d::bounds` (`crates/ferrous_2d/src/render/types.rs`) is an optional
`[x, y, width, height]` rect in target pixels (top-left origin). `None` means
the **full target**, which is the current behaviour and the default. The
builder is `Layer2d::with_bounds(rect)`.

It exists so an orchestrator can scissor the Pure2D offscreen passes to the
region a layer can touch:

| Pass | File:line | Scissor |
|---|---|---|
| `Pure2D Blur Effect Pass` | `crates/ferrous_renderer/src/renderer_core.rs:7132` | layer bound expanded by the kernel reach |
| `Pure2D Effect Composite Pass` | `crates/ferrous_renderer/src/renderer_core.rs:7193` | layer bound expanded by the effect margin |
| `Pure2D Mask Pass` | `crates/ferrous_renderer/src/renderer_core.rs:6954` | layer bound expanded by any mask blur |
| `Pure2D Plan Pass` | `crates/ferrous_renderer/src/renderer_core.rs:6593` | layer bound (content is already inside it) |

Blur margin: `BlurKernel2d::new(radius)` uses
`taps = ceil(1.5 * radius) + 1` (`crates/ferrous_2d/src/render/effects.rs:69`),
so the kernel reaches `taps - 1 = ceil(1.5 * radius)` pixels. The scissor must
include at least that reach, not just `radius`: use
`BlurKernel2d::new(effect.blur_radius()).taps - 1` per direction. For a
`DropShadow` also add the absolute offset, and for a `Glow` the same blur reach.

Wiring: the bound is `LayerRecord.desc.bounds`
(`crates/ferrous_renderer/src/renderer_core.rs:226-232`). `render_layered_2d`
iterates `plan_order` and calls `execute_plan` (line 6365), then
`apply_layer_effects` (line 6383, which calls `run_effect_chain` at line 6195)
and `run_layer_mask` (line 6384), passing only the layer index. Thread the
expanded rect through those helpers and call `rpass.set_scissor_rect` right
after each `begin_render_pass` (the plan pass at 6593, the mask pass at 6954,
the blur pass at 7132, the effect composite at 7193). The root plan/root
composite keeps `None` (full target). Clamp the expanded rect to the target;
an empty intersection means the pass can be skipped entirely.

Clears and texture copies still cover the whole texture: the pass clear
(`LoadOp::Clear` in the blur/effect/mask passes) and the
`copy_texture_to_texture` finalisation in `run_effect_chain`
(`renderer_core.rs:7088-7094`) and the mask copy (`renderer_core.rs:6972-6976`)
are not scissored. A scissored effect pass therefore saves fragment work but
does not reduce the clear/copy cost; correctness also requires the layer
texture to be cleared outside the bound (it already is) and the composite pass
to keep sampling the full layer.

## What it does not do

- It does not measure fill-rate in milliseconds or bytes. It counts fragments
  that reached the target, not their cost; blending, sampling and shader work
  are not modelled.
- It does not attribute overdraw to a node, layer or widget. The view sees
  primitives, not ownership; use the GUI batch ranges and `node_id` tooling for
  that.
- It is not an occlusion culling pass. It does not skip covered nodes or
  reorder opaque geometry; see the phase report verdicts (front-to-back
  ordering would change pixels because the later quad must win).
- It does not apply to the SVG pipeline, the 3D passes, or Primitive2D world
  geometry outside the GUI quad/text paths.
- It does not preserve the scene colours: the heat map replaces them, and
  additive alpha accumulation saturates the target alpha.
- It does not scissor itself. The view draws wherever the normal pass would;
  layer bounds are a separate optimisation.

## Phase P5 verdicts

These are the three GPU-efficiency claims the phase had to answer. The numbers
are reasoned estimates; no new measurement was taken for this diagnostic.

### 1. Front-to-back opaque drawing with early-z

The opaque pipeline exists (`quad.rs:13-143`, built with
`depth_write_enabled = true` at `gui_renderer.rs:292-299`) but it still uses
`BlendState::ALPHA_BLENDING` (`quad.rs:123`) and the render loop always binds
`transparent_pipeline` (`gui_renderer.rs:1001`; the split is a TODO at
`gui_renderer.rs:999`). Painter order is authoritative: a quad's `z_order`
becomes depth `1 - z_order` (`gui.wgsl:107`), and the later quad must win.

Reordering two overlapping opaque quads **can change pixels**:

- at equal `z_order` both pass `LessEqual`, so the quad drawn last wins; a
  reorder swaps the winner;
- a later quad with a lower `z_order` (legal, nothing enforces monotonicity)
  would be drawn first and then lose the depth test to an earlier, nearer
  quad, so the earlier quad wins where painter order says the later one must;
- AA edges have `alpha < 1` in the SDF, and the pipeline still alpha-blends, so
  even "opaque" quads are order-dependent at their borders;
- text uses `z = z_order` (not inverted) and never writes depth, so it cannot
  join a depth-sorted opaque batch.

**Verdict: reject.** Keep painter order. A depth prepass in painter order could
let later overlaps still resolve correctly, but it cannot remove visible
fragment work; it only helps fully hidden quads, which needs the bounds and
occluder metadata below.

### 2. Skip fully covered nodes

The direct-paint path that Motioner uses supplies `node_id: 0` and no per-quad
bounds, and `DrawSegment` (`gpu_types.rs:154-161`) carries only draw ranges and
an optional scissor, with no AABB. To skip a fully covered node you would need,
at minimum:

- a per-primitive target-pixel AABB, including SDF AA padding, border width and
  shadow blur/offset (the shadow and glow extend beyond the quad rect);
- a truthful opaque flag and a coverage/occlusion structure maintained as the
  frame paints (a coverage buffer or a CPU spatial index);
- node identity and paint order so a whole node's primitives can be dropped
  without changing what is visible.

Without any of those, "fully covered" cannot be proven before drawing.
**Verdict: reject (keep drawing).** Estimated saving today is zero; the
heavy-UI workload has roughly 150k glyph instances, but they are occluded mostly
by Motioner panels that are not registered as occluders. Revisit with a
depth/coverage prepass (P6/P7), not with CPU metadata.

### 3. `OPAQUE_BIT`

Set when a solid or two-stop gradient quad is fully opaque:
`to_batches.rs:51-55` (solid), `to_batches.rs:83-87` (gradient) and
`to_batches.rs:251-255` (generic quad). It is currently unused:
`gui.wgsl` reads flags `0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x200`
but never `0x100`, and the render loop never selects the opaque pipeline
(`gui_renderer.rs:1001`). With alpha blending and no depth writes the bit has
no effect on pixels.

**Verdict: keep setting it** (it is free and documents the authored intent), but
do not rely on it. Consuming it requires a per-segment opaque/transparent split,
binding the arm set to `depth_write_enabled = true`, and proving that reordering
within the opaque subset is safe — which verdict 1 rejects for the current
data model.

