<!--
Reference documentation for the gizmo subsystem in ferrous_renderer.
Covers GizmoDraw, GizmoPipeline, execute_gizmo_pass(), vertex generation,
and how the renderer fits into the three-crate gizmo architecture.
-->

# Gizmo Rendering

The gizmo system draws editor handles (translate arrows, plane squares)
as coloured line segments on top of the 3-D scene.  The renderer is
responsible only for **drawing**; all interaction logic (picking, dragging,
style customisation) lives in `ferrous_app::AppContext::update_gizmo`.

---

## Three-crate split

```
ferrous_core          owns state + style types
    GizmoState        mutable per-frame state (mode, highlights, dragging, style)
    GizmoStyle        full visual config (colors, arm_length, arrows, planes)
    Axis / Plane      X/Y/Z and XY/XZ/YZ enums
    GizmoMode         Translate | Rotate | Scale

ferrous_app           owns interaction
    AppContext::update_gizmo(handle, &mut GizmoState)
        sync transform → pick axis/plane → drag → queue GizmoDraw

ferrous_renderer      owns drawing          ← this document
    GizmoDraw         data handed to the renderer each frame
    GizmoPipeline     wgpu LineList pipeline
    execute_gizmo_pass()   builds vertices + draws
```

---

## GizmoDraw

Defined in `src/scene/gizmo.rs`.  One instance is pushed into
`Renderer::gizmo_draws` per gizmo per frame (via `AppContext::update_gizmo`).
The `Runner` drains `ctx.gizmos` into `renderer.queue_gizmo` after
`draw_3d` returns.

```rust
pub struct GizmoDraw {
    /// Translation-only world matrix (position_matrix()).
    /// Entity scale and rotation are stripped so handles are always
    /// the same world-space size, aligned to world axes.
    pub transform: Mat4,

    /// Which operation the gizmo represents (currently only Translate
    /// generates visible handles).
    pub mode: GizmoMode,

    /// Axis currently hovered or being dragged — rendered yellow.
    /// None = all axes use their normal colour.
    pub highlighted_axis: Option<Axis>,

    /// Plane handle currently hovered or being dragged.
    /// Mutually exclusive with highlighted_axis at pick time.
    pub highlighted_plane: Option<Plane>,

    /// Full visual style cloned from GizmoState.style.
    /// Drives all colors, sizes, and feature flags this frame.
    pub style: GizmoStyle,
}
```

> **Why `position_matrix()` and not `world_matrix()`?**
> `world_matrix()` = T × R × S — the entity's scale would make the
> handles grow with the object.  `position_matrix()` = T only, so the
> handles are always `style.arm_length` world units long regardless of
> how large the entity is.

---

## GizmoPipeline

Defined in `src/pipeline/gizmo.rs`.  Constructed once in `Renderer::new`.

```
Topology:        LineList        (every pair of vertices = one line)
Shader:          assets/shaders/gizmo.wgsl   (vs_main / fs_main)
Bind groups:     group 0 = camera uniform (same BGL as WorldPipeline)
Vertex layout:   Vertex { position: [f32;3], color: [f32;3] }
depth_compare:   Always          — gizmo always renders on top of scene
depth_write:     false           — gizmo does not occlude anything
MSAA:            sample_count from renderer config
```

The pipeline reuses the camera bind-group layout from `PipelineLayouts`,
so no extra bind-group management is needed.

---

## execute_gizmo_pass()

Called from `Renderer::render_to_view` / `render_to_target` after
`WorldPass::execute` and before `UiPass::execute`.  It is skipped
entirely when `self.gizmo_draws` is empty.

### Vertex generation

For each `GizmoDraw` the pass builds a flat `Vec<Vertex>` on the CPU:

#### 1. Axis shafts (always)

Three arms along +X, +Y, +Z in gizmo-local space, then transformed by
`gizmo.transform`:

```
p0 = transform.transform_point3(Vec3::ZERO)
p1 = transform.transform_point3(axis_vec × style.arm_length)
→ 2 vertices per arm, 6 total
```

Color = `style.axis_color(axis)` or `style.axis_highlight(axis)` when
`highlighted_axis == Some(axis)`.

#### 2. Arrowheads (when `style.show_arrows`)

A 4-fin cross at the tip of each arm, stable at any camera angle:

```
arr_len  = style.arrow_length()   // = arm_length × arrow_length_ratio
half_tan = tan(arrow_half_angle_deg in radians)

perp  = stable perpendicular to axis_vec
up2   = perp
side  = axis_vec × perp

fins: [up2, -up2, side, -side]
each fin:
    tip  = axis_vec × arm_length          (world-space tip)
    base = axis_vec × (arm_length - arr_len) + fin_dir × (arr_len × half_tan)
    → 2 vertices per fin, 8 per arm, 24 total
```

#### 3. Plane square outlines (when `style.show_planes`)

One small square per plane handle, positioned between the two axis arms:

```
PLANE_OFF  = style.plane_offset()   // = arm_length × plane_offset_ratio
PLANE_SIZE = style.plane_size()     // = arm_length × plane_size_ratio

(a, b) = plane.axes()
corners:
    c0 = a × PLANE_OFF        + b × PLANE_OFF
    c1 = a × (OFF + SIZE)     + b × PLANE_OFF
    c2 = a × (OFF + SIZE)     + b × (OFF + SIZE)
    c3 = a × PLANE_OFF        + b × (OFF + SIZE)
4 edges (c0→c1, c1→c2, c2→c3, c3→c0)
→ 8 vertices per plane, 24 total
```

Color = `style.plane_color(plane)` or `style.plane_highlight(plane)`
when `highlighted_plane == Some(plane)`.  The pipeline currently uses
only the RGB components of the RGBA `PlaneColors` values.

#### Total vertex budget (worst case)

| Section | Count |
|---|---|
| Axis shafts | 6 |
| Arrowheads (3 arms × 8) | 24 |
| Plane squares (3 planes × 8) | 24 |
| **Total per gizmo** | **54** |

### Draw call

```rust
// upload
let vb = device.create_buffer_init(&BufferInitDescriptor {
    contents: bytemuck::cast_slice(&vertices),
    usage: VERTEX,
    ..
});

// record
render_pass.set_pipeline(&gizmo_pipeline);
render_pass.set_bind_group(0, &camera_bind_group, &[]);
render_pass.set_vertex_buffer(0, vb.slice(..));
render_pass.draw(0..vertex_count, 0..1);
```

After the pass `gizmo_draws` is cleared.

---

## GizmoStyle — renderer-visible fields

The renderer reads the following `GizmoStyle` fields from each
`GizmoDraw`.  All defaults are Blender-like.

| Field | Default | Renderer use |
|---|---|---|
| `arm_length` | `1.5` | length of shaft and reference for derived sizes |
| `plane_offset_ratio` | `0.25` | `plane_offset() = arm_length × ratio` |
| `plane_size_ratio` | `0.22` | `plane_size()   = arm_length × ratio` |
| `show_arrows` | `true` | enables arrowhead fin generation |
| `arrow_half_angle_deg` | `20.0` | fin spread = `tan(radians(deg))` |
| `arrow_length_ratio` | `0.12` | `arrow_length() = arm_length × ratio` |
| `show_planes` | `true` | enables plane square generation |
| `x_axis` / `y_axis` / `z_axis` | red / green / blue | `AxisColors { normal, highlighted }` |
| `xy_plane` / `xz_plane` / `yz_plane` | blue / green / red | `PlaneColors { normal, highlighted }` (RGBA) |

---

## Depth behaviour

Gizmos intentionally **ignore scene depth**:

```rust
depth_write_enabled: false,
depth_compare: wgpu::CompareFunction::Always,
```

This matches the convention used by Blender, Unity, and Godot: editor
handles are always visible so the user can interact with them even when
the selected object is occluded.  Because depth writes are disabled the
gizmo does not occlude any geometry drawn after it.

---

## Extending

### Adding a new handle shape

1. Add geometry-generation code inside the `for gizmo in &self.gizmo_draws` loop in `execute_gizmo_pass` (`src/lib.rs`).
2. Push pairs of `Vertex` — each pair is one line segment.
3. Add the controlling flag to `GizmoStyle` in `ferrous_core` and re-export it from `ferrous_core::scene`.
4. Read the flag from `gizmo.style` in the renderer loop.

### Switching to a triangle pipeline for filled handles

Create a second pipeline (e.g. `GizmoFillPipeline`) with
`topology: TriangleList` and a separate vertex buffer.  Register it in
`Renderer` alongside the existing `GizmoPipeline`.  Drive both from
`execute_gizmo_pass` in two separate render passes, or merge them into
one pass with `set_pipeline` calls between draw calls.

### Rotation and scale handles

When `gizmo.mode == GizmoMode::Rotate`, generate arc geometry (line-strip
approximation of a circle in each axis plane) instead of straight shafts.
When `gizmo.mode == GizmoMode::Scale`, replace the arrowhead fins with a
small cube cap.

---

## See also

- [`ferrous_core::scene::gizmo`](../../../ferrous_core/src/scene/gizmo.rs) — `GizmoState`, `GizmoStyle`, all enums
- [`ferrous_app::context::update_gizmo`](../../../ferrous_app/src/context.rs) — picking + drag API
- [`extending/new_pipeline.md`](extending/new_pipeline.md) — how to add a new wgpu pipeline
- [`flowmaps/gizmo.md`](../../../../flowmaps/gizmo.md) — full system-level flow diagrams
