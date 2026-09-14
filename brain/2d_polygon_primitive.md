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
`params = [0b111, 0.0, 2.0, SHAPE_KIND_TRIANGLE]`. Now delegates to
[`polygon_triangle`](#12-shapeinstancepolygon_triangle-selective-edge-aa) with
the full edge mask.

Also exported:

```rust
/// params.w sentinel selecting the triangle SDF; only the triangle
/// constructors emit it.
pub const SHAPE_KIND_TRIANGLE: f32 = 2.0;
```

### 1.2 `ShapeInstance::polygon_triangle` (selective edge AA)

```rust
// crates/ferrous_2d/src/render/types.rs
pub fn polygon_triangle(
    a: glam::Vec2,
    b: glam::Vec2,
    c: glam::Vec2,
    color: [f32; 4],
    edge_aa_mask: u8,
) -> ShapeInstance

pub const TRIANGLE_EDGE_AB: u8 = 1 << 0; // edge a -> b
pub const TRIANGLE_EDGE_BC: u8 = 1 << 1; // edge b -> c
pub const TRIANGLE_EDGE_CA: u8 = 1 << 2; // edge c -> a
pub const TRIANGLE_EDGES_ALL: u8 = 0b111;
```

Same transform encoding as `triangle`, but the AA is applied **per edge**.
`edge_aa_mask` is a 3-bit mask: bit 0 = `a→b`, bit 1 = `b→c`, bit 2 = `c→a`.
A set bit anti-aliases that edge (external silhouette); a clear bit makes it a
**hard** edge — the fragment shader clips exactly at the edge line with no
coverage fade. `polygon_triangle(..., 0b111)` is byte-for-byte identical to
`triangle(...)` (it takes the same exact-Euclidean-SDF path); `0` gives hard
edges on all three sides.

### 1.3 Bulk draw

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
| `2.0` | **only** `triangle` / `polygon_triangle` | triangle SDF |

The shader selects the triangle branch with `params.w > 1.5`; the existing
`0.0`/`1.0` values still map to the rounded-rect/circle path, and no field,
constructor or struct-literal shape changed (`ShapeInstance` stays
source-compatible — Apéndice A).

### 3.1 Edge-AA mask (`params.x` for the triangle kind)

`params.x` is `border_thickness` for the rounded-rect/circle path, which the
triangle branch never evaluates, so it is repurposed **only for triangle
instances** as the edge-AA bitmask:

| `params.x` | meaning |
|---|---|
| `0b111` (`7.0`) | all three edges AA — the `triangle()` default and the exact-Euclidean-SDF path |
| any other `0..=6` | selective per-edge AA (bits 0/1/2 = `a→b` / `b→c` / `c→a`) |
| `0.0` | all edges hard (no AA) |

`polygon_triangle` masks the input with `TRIANGLE_EDGES_ALL` and stores it as
`params.x`. No existing constructor sets `params.w > 1.5`, so there is no
collision. `params.y` stays `0` and `params.z` stays `2.0` (same smoothing as
the rounded-rect SDF) for all triangle instances.

## 3.2 Selective edge AA — why and how

A triangulated fill used to show a light **seam** on internal diagonals: each
triangle applied AA to all three edges, so at a shared edge both fragments
faded to ~50% coverage and the background showed through the sum. With the
edge mask, both triangles mark the shared edge hard (bit clear): the fragment
shader hard-clips at that edge line and emits full coverage up to it, so the
two triangles tile exactly. External edges keep their AA.

Implementation (`shape_2d.wgsl`):

- `mask == 7u` keeps the original Inigo Quilez triangle SDF (unchanged output).
- Otherwise `triangle_masked_alpha`: for every bit-clear edge the point is
  rejected if outside that edge's half-plane (`>= 0`, so a point exactly on a
  shared edge is included by both triangles — avoiding a one-pixel crack);
  then the nearest bit-set edge's clamped-segment signed distance drives
  `1 - smoothstep(-fw, fw, d)`. Clamping to the segment endpoints makes two
  concurrent marked edges round the corner. The winding sign is derived from
  each triangle's own vertices, so the path stays two-sided.

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
→ **14/14 pass** (9 pre-existing + 5 new):

- `triangle_fills_centroid_with_aa_edges` — interior filled (`(154,101)`),
  exterior empty (`(207,47)`), a pixel exactly on the hypotenuse (`(168,88)`,
  world `x + y = 80`) is partial AA coverage (red channel strictly between 60
  and 200), and reversed winding renders identically (two-sided).
- `polygon_triangle_full_mask_equals_triangle` — `polygon_triangle(..., 0b111)`
  is pixel-identical to `triangle(...)`.
- `triangulated_quad_has_no_internal_seam` — a `(0,0)-(100,0)-(100,100)-
  (0,100)` quad split along the `p0-p2` diagonal, with each triangle's diagonal
  bit clear (`0b011` / `0b110`). The **entire** diagonal interior
  (`i = 140..=215`, `i + j = 255`) is full red (`>= 235`), both sides and an
  interior point are full red, and the outer left/bottom edges are partial AA
  (`81` and `174`). Witness: the same quad built from all-AA `triangle()` shows
  the seam (`191` on the diagonal).
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
   `ShapeInstance::polygon_triangle`, `TRIANGLE_EDGE_AB` / `TRIANGLE_EDGE_BC` /
   `TRIANGLE_EDGE_CA` / `TRIANGLE_EDGES_ALL`, `SHAPE_KIND_TRIANGLE`,
   `Renderer::draw_2d_shapes`, `RenderContext::draw_2d_shapes`. Existing
   constructors and struct literals compile and render unchanged.
2. **Flag encoding:** if a consumer built `ShapeInstance` by hand and wrote
   `params[3]` with a value `> 1.5`, that instance now selects the triangle
   SDF; if it also wrote `params[0]`, that is now the edge-AA mask. Historically
   only `0.0`/`1.0` were produced and neither slot was read by the shader, so no
   in-tree producer is affected. Documented in `types.rs` and `shape_2d.wgsl`.
3. **Triangulating a polygon (Koncreta):** fill convex polygons by fanning from
   the centroid (one triangle per side) and, for concave polygons, ear-clip into
   triangles. Build each triangle with `polygon_triangle`. A triangle edge is
   **external** iff it belongs to a single triangle of the triangulation and
   **internal** iff two triangles share it; set the mask bit only for external
   edges. Concretely, hash each undirected edge to its triangle-count/EAR or
   accumulate the two directed half-edges; bits are then `AB = 1`, `BC = 2`,
   `CA = 4` in the order the vertices are passed. When in doubt, `triangle()`
   (all bits set) is the safe visual default but will seam on shared internal
   edges; `polygon_triangle(..., 0)` gives a hard-edged fill.
4. **Recommended usage:** submit the whole triangulation with one
   `draw_2d_shapes(&[...])` call — H3 coalesces same-pipeline instances to one
   draw regardless of count.

## 7. Not verifiable on this host

- Real-GPU / native-surface behaviour and WebGPU-async build: the host has no
  display/GPU; only lavapipe (software Vulkan) native tests were run.
- Absolute frame time / fps: not a portable signal on a software rasteriser;
  omitted deliberately.
- Koncreta end-to-end migration (replacing its rect-based polygon fill with
  `triangle` + `draw_2d_shapes`) is a consumer change and was not exercised
  here.
