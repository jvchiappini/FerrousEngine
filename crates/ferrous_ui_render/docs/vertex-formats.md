# Vertex and instance formats

Reference for the byte layout of every vertex and instance struct consumed by
the GUI and Pure2D pipelines, the per-frame upload sizes derived from the
renderer's initial capacities, and the lossless packing changes that remain
pending because they need files owned by other lanes.

Sizes were measured with `size_of`/`offset_of` on the exact field lists in this
tree (Rust 1.96, `repr(C)`, 4-byte alignment everywhere). Counts come from the
engines' construction sites, not from a benchmark run. Line references were
re-checked against the working tree at hand-off; files owned by other lanes
(`gui_renderer.rs`, `renderer_core.rs`) may move again before merge.

## Struct sizes and initial capacities

| Struct | Defined at | Bytes | Alignment | Initial count | Full buffer |
|---|---|---|---|---|---|
| `GuiQuad` | `crates/ferrous_ui_render/src/gpu_types.rs:28` | 96 | 4 | 1,024 | 98,304 B |
| `TextQuad` (text) | `crates/ferrous_ui_render/src/gpu_types.rs:115` | 56 | 4 | 1,024 | 57,344 B |
| `TextQuad` (icons) | same struct, second buffer | 56 | 4 | 1,024 | 57,344 B |
| `SvgVertex` | `crates/ferrous_svg/src/lib.rs:43` | 28 | 4 | 65,536 | 1,835,008 B |
| SVG index (`u32`) | `crates/ferrous_ui_render/src/gui_renderer.rs:281` | 4 | 4 | 65,536 | 262,144 B |
| Static quad vertex (`[f32; 2]`) | `crates/ferrous_ui_render/src/gui_renderer.rs:94` | 8 | 4 | 4 | 32 B |
| Static quad index (`u16`) | `crates/ferrous_ui_render/src/gui_renderer.rs:95` | 2 | 2 | 6 | 12 B |
| `SpriteInstance` | `crates/ferrous_2d/src/render/types.rs:34` | 112 | 4 | 1,024 | 114,688 B |
| `ShapeGpu` (uploaded) | `crates/ferrous_2d/src/render/types.rs:1043` | 68 | 4 | 1,024 | 69,632 B |
| `ShapeInstance` (authoring only) | `crates/ferrous_2d/src/render/types.rs:353` | 120 | 4 | not uploaded | n/a |
| `TextInstance` | `crates/ferrous_2d/src/render/types.rs:162` | 116 | 4 | 1,024 | 118,784 B |
| `MeshVertex2d` | `crates/ferrous_2d/src/render/mesh.rs:28` | 28 | 4 | 1,024 | 28,672 B |
| 2-D mesh index (`u32`) | `crates/ferrous_2d/src/render/renderer.rs:242` | 4 | 4 | 6,144 | 24,576 B |

The UI capacities are set by `GuiRenderer::new` at
`crates/ferrous_renderer/src/renderer_core.rs:1245`; the 2-D capacities by
`Renderer2d::new_with_cache` at `crates/ferrous_renderer/src/renderer_core.rs:1263`
and `crates/ferrous_renderer/src/renderer_core.rs:1273`, clamped to at least
128 by `crates/ferrous_2d/src/render/renderer.rs:210`. The SVG capacities are
the literal `65_536` in `crates/ferrous_ui_render/src/gui_renderer.rs:274` and
`crates/ferrous_ui_render/src/gui_renderer.rs:281`; both buffers grow to the
next power of two when a frame exceeds them
(`crates/ferrous_ui_render/src/gui_renderer.rs:749` and
`crates/ferrous_ui_render/src/gui_renderer.rs:766`).

## Field maps

- `GuiQuad`: `pos` 0, `size` 8, `uv0` 16, `uv1` 24, `color` 32, `color_b` 48,
  `radii` 64, `tex_index` 80, `flags` 84, `z_order` 88, `node_id` 92.
- `TextQuad`: `pos` 0, `size` 8, `uv0` 16, `uv1` 24, `color` 32, `z_order` 48,
  `node_id` 52.
- `SvgVertex`: `position` 0, `color` 8, `z_order` 24.
- `SpriteInstance`: `transform_c0` 0, `transform_c1` 16, `transform_c2` 32,
  `transform_c3` 48, `color` 64, `uv_rect` 80, `properties` 96.
- `TextInstance`: `transform_c0` 0, `transform_c1` 16, `transform_c2` 32,
  `transform_c3` 48, `color` 64, `uv_rect` 80, `params` 96, `gradient` 112.
- `ShapeGpu`: `c0` 0, `c1` 8, `c3` 16, `dims` 24, `color` 32, `flags` 40,
  `ext0` 44, `ext1` 60, `gradient` 64.
- `MeshVertex2d`: `position` 0, `uv` 12, `alpha` 20, `premultiplied` 24.

None of these structs has interior or trailing padding: every field is 4-byte
aligned and the last field ends exactly at `size_of`. Compile-time assertions
now pin this in `crates/ferrous_ui_render/src/gpu_types.rs:127`,
`crates/ferrous_svg/src/lib.rs:51`,
`crates/ferrous_2d/src/render/types.rs:1228` and in each pipeline that owns a
vertex layout (`crates/ferrous_ui_render/src/pipelines/quad.rs:7`,
`crates/ferrous_ui_render/src/pipelines/text.rs:7`,
`crates/ferrous_ui_render/src/pipelines/id.rs:6`,
`crates/ferrous_ui_render/src/pipelines/svg.rs:6`). The layouts are therefore
already byte-tight for the fields the shaders consume; the changes below are
the only further lossless reductions, and all of them cross a file boundary.

## What is already packed

- `GuiQuad::flags` is a single `u32` bitfield (`crates/ferrous_ui_render/src/lib.rs:25`).
  The colour-wheel, textured, three gradient modes, border, shadow, opaque and
  line behaviours share one word, so no boolean padding remains to remove.
- `ShapeGpu` already carries every non-geometric boolean in one `u32`:
  minimum width, triangle kind, ring, triangle edge masks, hard caps, line,
  joined/round-join markers, arc, dash-in-px and the dash index
  (`crates/ferrous_2d/src/render/types.rs:1043`). `ShapeInstance` is authoring
  only; the renderer uploads the 68-byte packed form via
  `ShapeGpu::pack` (`crates/ferrous_2d/src/render/types.rs:1152`) from
  `crates/ferrous_2d/src/render/renderer.rs:911`.
- `TextInstance::params` is a full `vec4` although only `params.x` (the MSDF
  pixel range) is read. Trimming it is a shader-interface change, listed below.
- `SvgVertex` duplicates colour and depth per vertex, and the GUI renderer
  overwrites both from the draw command while flattening the arena
  (`crates/ferrous_ui_render/src/gui_renderer.rs:735`). This is redundancy, not
  padding; removing it requires a per-command attribute, listed below.

## Uploads cover only the live range

The renderer does not write the full-capacity buffers:

- UI quads write `batch.as_quad_bytes()` at
  `crates/ferrous_ui_render/src/gui_renderer.rs:613`, text at
  `crates/ferrous_ui_render/src/gui_renderer.rs:634` and icons at
  `crates/ferrous_ui_render/src/gui_renderer.rs:655`. Those views cover
  exactly `Vec::len()` instances (`crates/ferrous_ui_render/src/gui_batch.rs:485`
  through `crates/ferrous_ui_render/src/gui_batch.rs:505`) and are pinned by
  the `byte_views_cover_exactly_the_live_instances` test.
- SVG vertices and indices write the live scratch length at
  `crates/ferrous_ui_render/src/gui_renderer.rs:784` and
  `crates/ferrous_ui_render/src/gui_renderer.rs:789`.
- Pure2D sprites, shapes, text and meshes likewise write only the submitted
  slice at `crates/ferrous_2d/src/render/renderer.rs:982`,
  `crates/ferrous_2d/src/render/renderer.rs:946`,
  `crates/ferrous_2d/src/render/renderer.rs:1021`,
  `crates/ferrous_2d/src/render/renderer.rs:1081` and
  `crates/ferrous_2d/src/render/renderer.rs:1083`.

Heavy-scenario upload, computed as live count times struct size at the call
sites above (before and after this change, since no wire format changed):

| Pipeline | Bytes per frame |
|---|---|
| UI quads (1,024 x 96) | 98,304 |
| UI text (1,024 x 56) | 57,344 |
| UI icons (1,024 x 56) | 57,344 |
| SVG vertices (65,536 x 28) | 1,835,008 |
| SVG indices (65,536 x 4) | 262,144 |
| UI total | 2,310,144 |
| 2-D sprites (1,024 x 112) | 114,688 |
| 2-D shapes (1,024 x 68) | 69,632 |
| 2-D text (1,024 x 116) | 118,784 |
| 2-D mesh vertices (1,024 x 28) | 28,672 |
| 2-D mesh indices (6,144 x 4) | 24,576 |
| 2-D total | 356,352 |

The buffers themselves stay at their grown capacity, so these are upload
bytes, not resident bytes. The initial resident totals are the "Full buffer"
column above.

## Rejected: precision-losing formats

No colour, UV, position, radius, depth or alpha field was narrowed. `f16`
colours, `unorm8` colours/UVs and packed 16-bit positions all round the values
the shaders interpolate, so they can move a pixel when coverage lands on a
half-texel or an anti-aliasing edge. Even a one-LSB change breaks the goldens.
`ShapeGpu`'s existing `f16` colour is part of the established 2-D output and
was left untouched; narrowing it further would move pixels.

## Pending lossless changes (cross-lane)

Each entry states the exact saving at the heavy-scenario count. None was
applied in this lane because the edit needs a file owned elsewhere.

1. `TextQuad::node_id` is declared by `text.wgsl` at location 7 but never read
   by its body. Dropping it makes the struct 52 bytes (upload 53,248 B per
   1,024 glyphs, saving 4,096 B for text and 4,096 B for icons). Requires
   removing the `i_node_id` argument from `assets/shaders/text.wgsl:56` and the
   location-7 attribute from `crates/ferrous_ui_render/src/pipelines/text.rs`.
2. `GuiQuad::tex_index` fits in four bits (`MAX_TEXTURE_SLOTS = 16`,
   `crates/ferrous_ui_render/src/lib.rs:20`). Folding it into the high bits of
   `flags` makes the struct 92 bytes (94,208 B per 1,024 quads, saving
   4,096 B) while every defined flag occupies bits 0..=9. Requires decoding in
   `assets/shaders/gui.wgsl:98` and `assets/shaders/gui_web.wgsl`, plus the
   offset updates in `crates/ferrous_ui_render/src/pipelines/quad.rs` and
   `crates/ferrous_ui_render/src/pipelines/id.rs` (the ID pass shares the
   instance buffer).
3. SVG indices are `u32` and the buffer is sized at 65,536 entries
   (`crates/ferrous_ui_render/src/gui_renderer.rs:281`), but the draw count is
   bounded by the 65,536-vertex arena in the common case. `u16` indices would
   halve that upload to 131,072 B (saving 131,072 B per frame) and halve the
   index buffer to 131,072 B. Not provably safe for every frame: the arena
   grows past 65,536 vertices at
   `crates/ferrous_ui_render/src/gui_renderer.rs:749`, and index values are
   arena-absolute. A `u16` path needs a capacity guard, `IndexFormat::Uint16`
   at `crates/ferrous_ui_render/src/gui_renderer.rs:925`, and a matching index
   type in `SvgMesh` (`crates/ferrous_svg/src/lib.rs:25`).
4. `SvgVertex` carries colour (16 B) and depth (4 B) per vertex, but the only
   consumer overwrites both per command
   (`crates/ferrous_ui_render/src/gui_renderer.rs:739`). A position-only
   8-byte vertex plus one per-command attribute would cut the SVG vertex
   upload from 1,835,008 B to 524,288 B (saving 1,310,720 B), at the cost of
   splitting the merged indexed draw per command
   (`crates/ferrous_ui_render/src/gui_renderer.rs:940`) and changing
   `assets/shaders/svg.wgsl`.
5. `TextInstance::params` keeps three unused floats. Making it a scalar
   shrinks the instance to 112 bytes (114,688 B per 1,024 glyphs, saving
   4,096 B) but changes `params` in `assets/shaders/text_2d.wgsl:28` from
   `vec4` to `f32` and the location-6 format in
   `crates/ferrous_2d/src/render/types.rs:223`.
6. `MeshVertex2d::premultiplied` is constant per draw (set in
   `crates/ferrous_2d/src/render/mesh.rs:193`) yet uploaded per vertex.
   Moving it to a per-draw uniform or instance word would save 4 bytes per
   vertex (4,096 B at 1,024 vertices) but redefines the textured-mesh input
   contract in `assets/shaders/textured_mesh_2d.wgsl:28` and the batcher.
7. `MeshBatcher2d::indices` is `u32`
   (`crates/ferrous_2d/src/render/mesh.rs:126`) and the GPU index buffer is
   `u32` (`crates/ferrous_2d/src/render/renderer.rs:1067`). The arena can
   exceed 65,535 vertices across draws, so `u16` needs a per-frame guard and a
   renderer change outside this lane.

## Related stale documentation

`crates/ferrous_ui_render/docs/README.md:197` still describes `GuiQuad` as
80 bytes and lists `flags` without the line bit; the authoritative layout is
the 96-byte one above. That page is outside this lane's write set.
