# FerrousEngine — Per-Cap AA Control for Line Segments

> Branch: `perf/2d-polygon`. Fixes the visual seam that appears where Koncreta's
> thick flat-capped wall segments connect (miters, T-junctions, chains): both
> segments anti-aliased their touching cap and the background showed through the
> sum. The fix lets a consumer mark connected caps as **hard** so the segments
> tile exactly, while free caps keep the normal AA.
>
> Additive and Apéndice A-safe; no existing constructor's behaviour changed.

## 1. API added

```rust
// crates/ferrous_2d/src/render/types.rs
pub fn line_with_cap_style_aa(
    from: glam::Vec2,
    to: glam::Vec2,
    width: f32,
    color: [f32; 4],
    cap_style: crate::components::CapStyle,
    aa_start: bool,
    aa_end: bool,
) -> ShapeInstance

pub fn line_with_cap_aa(
    from: glam::Vec2,
    to: glam::Vec2,
    width: f32,
    color: [f32; 4],
    cap_style: u8,          // 0 = Flat, 1 = Round, 2 = Square
    aa_start: bool,
    aa_end: bool,
) -> ShapeInstance
```

- `aa_start` controls the cap at `from`, `aa_end` the cap at `to`.
- `true` (normal AA): the cap fades over the AA band (free ends).
- `false` (hard cap): the shader clips exactly at the end plane and saturates
  coverage just inside it; two segments that share that endpoint, both hard,
  render edge-to-edge with no gap and no double-blend seam.
- `line_with_cap_style(...)` and `line_with_cap(...)` are unchanged: they now
  delegate with `aa_start = aa_end = true`, so their geometry, params and output
  are identical. `line_with_cap_style_aa(..., true, true)` is pixel-identical to
  `line_with_cap_style(...)` (asserted).

## 2. Flag encoding (additive, `transform_c2.xy`)

The two flags are stored in `ShapeInstance::transform_c2.x` / `.y`:

| channel | value | meaning |
|---|---|---|
| `transform_c2.x` | `0.0` | `from`/start cap has normal AA |
| | `1.0` | `from`/start cap is hard (AA disabled) |
| `transform_c2.y` | `0.0` | `to`/end cap has normal AA |
| | `1.0` | `to`/end cap is hard (AA disabled) |

`transform_c2` is the model matrix **z basis**, but the 2D shader's canonical
quad always has `local z = 0`, so `transform_c2` never contributes to the
geometry and its `xy` is otherwise unused. Every existing constructor goes
through `from_model`, which writes the identity z basis `(0, 0, 1, 0)`, i.e.
`transform_c2.xy = (0, 0)` = both caps AA. There is therefore no collision with
rect / circle / line / triangle / polygon_triangle, and the default shader path
is bit-for-bit the old one. `params` is untouched, so the triangle edge mask
(`params.x`) and shape kind (`params.w`) are unaffected.

## 3. Shader implementation (`assets/shaders/shape_2d.wgsl`)

- `cap_hard` is passed to the fragment shader as a `flat` varying populated from
  `transform_c2.xy`.
- The rounded-box SDF is generalised to an asymmetric box: a hard cap pushes
  that side of the analysis box out by `filter_width * 4 + 1` local units, so
  the SDF is saturated (alpha ≈ 1) near the true cap plane. The true plane is
  then clipped: `p.x < -half.x` for a hard start, `p.x > half.x` for a hard end.
- With both flags `0` the extension terms are exactly `0`, so the formula
  reduces to the original `abs(p) - half + radius` SDF (pixel-identical).
- The clip uses `>=` / `<=` inclusiveness so a point exactly on the shared
  endpoint is covered by both segments — neither a one-pixel gap nor an
  unblended double edge (same trade-off already used for internal triangle
  edges). Long edges and AA caps keep the existing thin-line stabilisation and
  `smoothstep` AA.

## 4. Test evidence

`cargo test -p ferrous_renderer --test pure2d_headless -- --test-threads=1`
→ **17/17 pass** (12 pre-existing + 5 from the triangle work, all still green;
3 new line-cap tests):

- `line_cap_aa_full_flags_match_line_with_cap_style` — `line_with_cap_style_aa`
  with `(true, true)` produces a pixel-identical buffer to
  `line_with_cap_style`.
- `collinear_segments_with_hard_joint_have_no_seam` — two collinear flat-capped
  walls (`width = 20`) share the endpoint `(0, 0)` with that cap hard. Pixels at
  `x = ±0.5` for the whole joint column are full red (`>= 235`); the free left
  cap is still partial AA (`81`). Witness: the same chain with both caps AA
  shows the seam at the joint (`200` on the shared boundary).
- `hard_end_against_side_of_another_segment_has_no_gap` — a horizontal wall with
  a hard right end butts into the middle of a vertical through-wall; the whole
  `x, y ∈ [-6, 6]` junction box is full red.

Other verification:

- `cargo check --workspace` ✅
- `cargo test -p ferrous_2d --lib` ✅ (1/1)
- `cargo test -p ferrous_renderer --lib` ✅ (4/4)
- `cargo test -p ferrous_renderer --test sprite_texture_recycle` ✅ (2/2)
- `cargo run -p ferrous_renderer --example cad_capture` ✅ still reports
  mid-grey `0.5` → `128` (non-sRGB) / `187` (sRGB).

## 5. Migration / consumer notes (Koncreta)

1. **Additive, no action required:** `line_with_cap_style_aa` and
   `line_with_cap_aa`. Existing calls to `line_with_cap`,
   `line_with_cap_style` and the `ShapeInstance` struct literal compile and
   render unchanged.
2. **How to mark connected walls:** when building a wall run (chain, miter path
   or T), pass `aa_start = false` for an endpoint that coincides with another
   segment and `aa_end = false` for the other; leave `true` at free/exterior
   ends. A shared endpoint must be hard on **both** segments for the seam to
   vanish (both sides stop fading). If in doubt, start with all `true`
   (current look) and only harden the junctions.
3. **Round/square caps:** the flag applies to the cap plane. Hard ends are most
   useful with `CapStyle::Flat` (CAD walls). For round caps a hard end clips at
   the cap tip, which has negligible visual effect; free round caps should stay
   `true`.
4. **Spare channel:** `transform_c2.xy` is now a documented 2D-only channel.
   Code that reads `ShapeInstance` as a raw model matrix (none in-tree) should
   treat those two components as flags for line instances.

## 6. Not verifiable on this host

- Real-GPU / native-surface behaviour and the WebGPU-async build: the host has
  no display/GPU; only lavapipe (software Vulkan) native tests were run.
- Absolute frame time / fps: not a portable signal on a software rasteriser.
- Koncreta's actual miter/T topology: the consumer-side marking of which
  endpoints are shared was not exercised here.
