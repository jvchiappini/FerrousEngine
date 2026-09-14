# FerrousEngine — Filled Triangle/Polygon Primitive + Bulk Draw API

> Branch: `perf/2d-polygon` (cut from `main`). Scope: give CAD consumers a
> first-class **filled, anti-aliased triangle** (audit item H11) plus a bulk
> shape-submission API, so a convex polygon fill is one triangle per side
> instead of many `ShapeInstance::rect` instances.
>
> All numbers are **hardware-independent** (draw calls, instances uploaded,
> bytes uploaded). The host has no GPU (lavapipe software Vulkan), so no
> wall-clock/fps claim is made.

## 1. API added

### 1.1 `ShapeInstance::triangle`

```rust
// crates/ferrous_2d/src/render/types.rs
pub fn triangle(
    a: glam::Vec2,
    b: glam::Vec2,
    c: glam::Vec2,
    color: [f32; 4],
) -> ShapeInstance
```

Filled + anti-aliased triangle with world-space vertices `a`, `b`, `c`
(1 world unit = 1 pixel in Pure2D; origin at screen centre, Y up). Renders
two-sided: the result does not depend on winding. Emits
`params = [0.0, 0.0, 2.0, SHAPE_KIND_TRIANGLE]`.

Also exported:

```rust
/// params.w sentinel selecting the triangle SDF; only `triangle()` emits it.
pub const SHAPE_KIND_TRIANGLE: f32 = 2.0;
```

### 1.2 Bulk draw

```rust
// crates/ferrous_renderer/src/renderer_core.rs
pub fn draw_2d_shapes(&mut self, shapes: &[ferrous_2d::render::types::ShapeInstance])

// crates/ferrous_engine/src/render_context.rs
pub fn draw_2d_shapes(&mut self, shapes: &[ferrous_renderer::render_2d::ShapeInstance])
```

Exactly equivalent to calling `draw_2d_shape` once per element: the slice is
appended with a single `extend_from_slice` into the same flat `ShapeBatcher`,
so painter order, pixels and H3-coalesced draw calls are unchanged. Both are
additive; Apéndice A callers need no change.

## 2. Transform encoding (verified against the vertex shader)

Every triangle is the affine image of the canonical corner triangle
`(0,0)`, `(1,0)`, `(0,1)`, so it fits the existing `Mat4` columns with **no new
instance fields**:

```text
c0 = b - a
c1 = c - a
c2 = (0, 0, 1, 0)
c3 = a
```

`shape_2d.wgsl` generates a 4-vertex quad from `vertex_index`; the historical
rounded-rect path uses `corner - 0.5` (a unit quad centred on the transform
origin). With `c3 = a` that would put `a` at the quad centre and the triangle
would fall outside. The triangle branch therefore keeps the quad at the
canonical `{0,1}²` corner origin, so:

```text
local (0,0) -> a
local (1,0) -> b
local (0,1) -> c
```

and expands the quad about the triangle centroid `(1/3, 1/3)` by
`1 + pad / inradius` (capped at 2x, like the rect path) so the AA band stays
inside the rasterised quad. The rounded-rect/circle branch is byte-for-byte
unchanged.

## 3. Shape-kind flag encoding (additive)

`ShapeInstance::params.w` was documented as `is_filled` but was **never read**
by the shader, so it is repurposed as a shape-kind flag with explicit sentinels:

| `params.w` | producer | shader branch |
|---|---|---|
| `0.0` | `rect_outline` (legacy `is_filled = 0`) | rounded-rect/circle SDF |
| `1.0` | `rect*` / `circle*` / `line*` (legacy `is_filled = 1`) | rounded-rect/circle SDF |
| `2.0` | **only** `ShapeInstance::triangle` | triangle SDF |

The shader selects the triangle branch with `params.w > 1.5`; the existing
`0.0`/`1.0` values still map to the rounded-rect/circle path, and no field,
constructor or struct-literal shape changed (`ShapeInstance` stays
source-compatible — Apéndice A).

`params.x/y/z` keep their meaning (border, corner radius, AA smoothing); the
triangle constructor sets `x = y = 0` and `z = 2.0` (same smoothing as the
rounded-rect SDF). The triangle SDF is the Inigo Quilez signed-distance
function, computed in world space with the legacy `world_pixel_size` AA
filter width — winding-agnostic, hence two-sided.

## 4. Measured deltas (same machine, lavapipe/Vulkan)

Test: `polygon_fill_triangle_fan_vs_scanline_rects_64gon`
(`crates/ferrous_renderer/tests/pure2d_headless.rs`), 256×256 off-screen
target, regular 64-gon radius 100.

| approach | instances_uploaded | draw_calls | bytes_uploaded |
|---|---|---|---|
| old scan-line rect approximation (1 rect / world unit) | **201** | 1 | **19 296** |
| new triangle fan (1 triangle / side) | **64** | 1 | **6 144** |
| delta | **-68.2 %** | 0 | **-68.2 %** |

`ShapeInstance` is 96 bytes (6 × 16-byte vec4). The draw-call count is already
`1` for both because the H3 coalescing pass merges same-pipeline consecutive
runs; the primitive's win is fewer instances (consumer construction cost) and
fewer uploaded bytes. Draw calls stay at the minimum (1) for a same-pipeline
batch. Pixels: the fan fills the polygon centre (asserted).

Scale note: 1 triangle per polygon side is the direct primitive cost; a
scan-line rect approximation scales with the polygon's vertical extent, so the
gap grows with size.

## 5. Test evidence

`cargo test -p ferrous_renderer --test pure2d_headless -- --test-threads=1`
→ **12/12 pass** (9 pre-existing + 3 new):

- `triangle_fills_centroid_with_aa_edges` — interior filled (`(154,101)`),
  exterior empty (`(207,47)`), a pixel exactly on the hypotenuse (`(168,88)`,
  world `x + y = 80`) is partial AA coverage (red channel strictly between 60
  and 200), and reversed winding renders identically (two-sided).
- `bulk_draw_2d_shapes_matches_individual_calls` — identical counters and
  pixel-identical output versus per-element `draw_2d_shape`.
- `polygon_fill_triangle_fan_vs_scanline_rects_64gon` — the table above.

Other verification:

- `cargo check --workspace` ✅
- `cargo test -p ferrous_2d --lib` ✅ (1/1)
- `cargo test -p ferrous_renderer --lib` ✅ (4/4)
- `cargo test -p ferrous_renderer --test sprite_texture_recycle` ✅ (2/2)
- `cargo run -p ferrous_renderer --example cad_capture` ✅ still reports
  mid-grey `0.5` → `128` (non-sRGB) / `187` (sRGB).

## 6. Migration notes

1. **Additive, no action required:** `ShapeInstance::triangle`,
   `SHAPE_KIND_TRIANGLE`, `Renderer::draw_2d_shapes`,
   `RenderContext::draw_2d_shapes`. Existing constructors and struct literals
   compile and render unchanged.
2. **Flag encoding:** if a consumer built `ShapeInstance` by hand and wrote
   `params[3]` with a value `> 1.5`, that instance now selects the triangle
   SDF. Historically only `0.0`/`1.0` were produced and `params[3]` was not
   read, so no in-tree producer is affected. Documented in
   `types.rs` and `shape_2d.wgsl`.
3. **Recommended usage:** fill a convex polygon by fanning triangles from its
   centroid (one per side) and submitting them with `draw_2d_shapes`. For
   concave polygons, ear-clip to triangles and submit the fan/triangulation in
   one slice.

## 7. Not verifiable on this host

- Real-GPU / native-surface behaviour and WebGPU-async build: the host has no
  display/GPU; only lavapipe (software Vulkan) native tests were run.
- Absolute frame time / fps: not a portable signal on a software rasteriser;
  omitted deliberately.
- Koncreta end-to-end migration (replacing its rect-based polygon fill with
  `triangle` + `draw_2d_shapes`) is a consumer change and was not exercised
  here.
