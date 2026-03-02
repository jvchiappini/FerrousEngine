````markdown
# gizmo

> **Description:** Architecture and data flow for the editor gizmo system — translate/rotate/scale handles. Covers the three-crate split (core state, app interaction, renderer drawing), the `GizmoStyle` customisation API, picking math, and drag mechanics.

---

## Three-Crate Split

```mermaid
graph LR
    CORE["ferrous_core\nferrous_core::scene::gizmo\n──────────────────────────\nGizmoState  — mutable per-frame state\nGizmoMode   — Translate / Rotate / Scale\nAxis        — X / Y / Z\nPlane       — XY / XZ / YZ\nGizmoStyle  — full visual config\nAxisColors  — normal + highlighted RGB\nPlaneColors — normal + highlighted RGBA\naxis_vector() — Axis → Vec3"]

    APP["ferrous_app\nAppContext::update_gizmo()\n──────────────────────────\nPicking  — axis (dist-to-segment)\n         — plane (shoelace point-in-quad)\nDragging — axis  (1D screen-space projection)\n         — plane (2D sum of two axis projections)\nQueuing  — pushes GizmoDraw into ctx.gizmos"]

    REND["ferrous_renderer\nferrous_renderer::scene::gizmo\n──────────────────────────\nGizmoDraw — transform + mode + highlights + style\nexecute_gizmo_pass() — builds line-list vertices\n  axis shafts + arrowheads (4-fin cross)\n  plane square outlines (when style.show_planes)\ndepth_compare: Always — always on top\ndepth_write_enabled: false — no occlusion"]

    CORE -->|"GizmoState, GizmoStyle\npassed by &mut from editor"| APP
    APP  -->|"GizmoDraw (clone of style)\nqueued into ctx.gizmos"| REND
```

---

## GizmoStyle — Full Field Reference

```mermaid
graph TD
    STY["GizmoStyle (Default: Blender-like)"]

    STY --> GEO["Geometry\n─────────────────\narm_length: f32 = 1.5\nplane_offset_ratio: f32 = 0.25\nplane_size_ratio: f32 = 0.22\n\nDerived helpers:\n  plane_offset() = arm_length × plane_offset_ratio\n  plane_size()   = arm_length × plane_size_ratio\n  arrow_length() = arm_length × arrow_length_ratio"]

    STY --> ARR["Arrowheads\n─────────────────\nshow_arrows: bool = true\narrow_half_angle_deg: f32 = 20.0\narrow_length_ratio: f32 = 0.12\n\n4-fin cross pattern:\n  2 perpendicular vectors to the axis\n  each fin = tip → base+offset"]

    STY --> PL["Plane handles\n─────────────────\nshow_planes: bool = true"]

    STY --> AXCOL["Axis colours (AxisColors)\n─────────────────\nx_axis.normal      = [1.0, 0.2, 0.2]  (red)\nx_axis.highlighted = [1.0, 1.0, 0.0]  (yellow)\ny_axis.normal      = [0.2, 1.0, 0.2]  (green)\ny_axis.highlighted = [1.0, 1.0, 0.0]\nz_axis.normal      = [0.2, 0.4, 1.0]  (blue)\nz_axis.highlighted = [1.0, 1.0, 0.0]"]

    STY --> PLCOL["Plane colours (PlaneColors)\n─────────────────\nxy_plane.normal      = [0.2, 0.2, 1.0, 0.5]\nxy_plane.highlighted = [0.4, 0.4, 1.0, 0.8]\nxz_plane.normal      = [0.2, 1.0, 0.2, 0.5]\nxz_plane.highlighted = [0.4, 1.0, 0.4, 0.8]\nyz_plane.normal      = [1.0, 0.2, 0.2, 0.5]\nyz_plane.highlighted = [1.0, 0.4, 0.4, 0.8]"]
```

---

## Customisation Examples

```mermaid
flowchart TD
    EX1["Minimal gizmo (just lines, no arrows, no planes)\n────────────────────────────────────────\ngizmo.style.show_arrows = false;\ngizmo.style.show_planes = false;"]

    EX2["Larger gizmo (bigger scene)\n────────────────────────────────────────\ngizmo.style.arm_length = 4.0;"]

    EX3["Monochrome debug gizmo\n────────────────────────────────────────\ngizmo.style.x_axis = AxisColors::new([0.8, 0.8, 0.8], [1.0, 1.0, 1.0]);\ngizmo.style.y_axis = AxisColors::new([0.8, 0.8, 0.8], [1.0, 1.0, 1.0]);\ngizmo.style.z_axis = AxisColors::new([0.8, 0.8, 0.8], [1.0, 1.0, 1.0]);"]

    EX4["Wide plane squares\n────────────────────────────────────────\ngizmo.style.plane_offset_ratio = 0.15;\ngizmo.style.plane_size_ratio   = 0.35;"]

    EX5["Narrow arrowhead\n────────────────────────────────────────\ngizmo.style.arrow_half_angle_deg = 10.0;\ngizmo.style.arrow_length_ratio   = 0.08;"]
```

---

## Per-Frame Data Flow

```mermaid
sequenceDiagram
    participant ED  as ferrous_editor\nEditorApp::draw_3d()
    participant CTX as AppContext\nupdate_gizmo()
    participant WLD as ferrous_core\nWorld
    participant REND as ferrous_renderer\nexecute_gizmo_pass()

    ED->>CTX: ctx.update_gizmo(handle, &mut self.gizmo)

    Note over CTX: Step 1 — Sync transform
    CTX->>WLD: world.transform(handle) → Transform
    CTX->>CTX: gizmo.update_world_transform(tr)

    Note over CTX: Step 2 — Build VP matrix from camera_eye / camera_target
    CTX->>CTX: view = Mat4::look_at_rh(eye, target, up)\nproj = Mat4::perspective_rh(45°, aspect, 0.1, 2000)\nvp = proj × view

    Note over CTX: Step 3 — Pick on left-click
    CTX->>CTX: Axis picking: dist-to-segment for X/Y/Z arms\n  threshold = 24 px screen space
    CTX->>CTX: Plane picking: shoelace signed-area for XY/XZ/YZ quads\n  camera-angle independent (CW + CCW)
    CTX->>CTX: Planes > Axes when overlapping

    Note over CTX: Step 4 — Drag translation
    CTX->>CTX: Axis drag: screen_dot / slen × arm_len → world_delta
    CTX->>WLD: world.translate(handle, av × world_delta)
    CTX->>CTX: Plane drag: sum of two axis contributions
    CTX->>WLD: world.translate(handle, total)

    Note over CTX: Step 5 — Queue draw
    CTX->>CTX: draw = GizmoDraw::new(gizmo.position_matrix(), mode)\n  draw.style = gizmo.style.clone()  ← carries full style
    CTX->>REND: ctx.gizmos.push(draw)  [drained by Runner]

    Note over REND: execute_gizmo_pass()
    REND->>REND: for each GizmoDraw:\n  build axis shaft vertices (2 pts each)\n  build arrowhead fins (4 fins × 2 pts, if style.show_arrows)\n  build plane square edges (4 segments, if style.show_planes)\n  colors from style.axis_color() / axis_highlight()\n                 style.plane_color() / plane_highlight()
    REND->>REND: upload vertex buffer → draw(LineList)
    REND->>REND: gizmo_draws.clear()
```

---

## Picking Algorithms

```mermaid
flowchart TD
    subgraph "Axis Picking — Distance to Segment"
        AP1["Project origin O and tip T into screen pixels"]
        AP2["Mouse M in screen pixels"]
        AP3["t = clamp( dot(M-O, T-O) / |T-O|² , 0, 1 )"]
        AP4["closest = O + t×(T-O)"]
        AP5["dist = |M - closest|"]
        AP6["if dist < 24 px → axis candidate"]
        AP1 --> AP2 --> AP3 --> AP4 --> AP5 --> AP6
    end

    subgraph "Plane Picking — Shoelace Point-in-Quad"
        PP1["Project 4 world corners into screen pixels\nSkip if any corner behind camera (w ≤ 0)"]
        PP2["Compute signed area via shoelace:\nquad_area = Σ(xᵢ×yⱼ - xⱼ×yᵢ)\nsign = quad_area.signum()"]
        PP3["For each edge i→j:\ncross = (xⱼ-xᵢ)×(my-yᵢ) - (yⱼ-yᵢ)×(mx-xᵢ)\nif cross×sign < 0 → outside"]
        PP4["All 4 edges pass → inside"]
        PP1 --> PP2 --> PP3 --> PP4
    end

    subgraph "Priority"
        PR["Plane match → use plane (clear axis)\nAxis match only → use axis (clear plane)\nNeither → clear both, dragging = false"]
    end
```

---

## Drag Translation Math

```mermaid
flowchart TD
    subgraph "Axis Drag (1D)"
        AD1["axis_vec = axis_vector(highlighted_axis)"]
        AD2["Project origin and tip into screen\nscreen_dir = (ts - os)  [px vector]"]
        AD3["screen_dot = dot(mouse_delta_px, screen_dir) / |screen_dir|"]
        AD4["world_delta = screen_dot / |screen_dir| × arm_len"]
        AD5["world.translate(handle, axis_vec × world_delta)"]
        AD1 --> AD2 --> AD3 --> AD4 --> AD5
    end

    subgraph "Plane Drag (2D)"
        PD1["(a, b) = plane.axes()"]
        PD2["For each of [a, b]:\n  project origin + av×arm_len → screen direction\n  screen_dot / |screen_dir| × arm_len → delta_along_av"]
        PD3["total = a×delta_a + b×delta_b"]
        PD4["world.translate(handle, total)"]
        PD1 --> PD2 --> PD3 --> PD4
    end
```

---

## Renderer Vertex Generation

```mermaid
flowchart TD
    VG["For each GizmoDraw in gizmo_draws"]

    VG --> SA["Axis shafts (3 arms)\n  p0 = m.transform_point3(ZERO)\n  p1 = m.transform_point3(axis_vec × arm_len)\n  → 2 vertices per arm = 6 total"]

    SA --> AH["Arrowheads (if style.show_arrows)\n  perp = stable perpendicular to axis\n  up2  = perp\n  side = axis × perp\n  4 fins: [up2, -up2, side, -side]\n  each fin: tip → base + fin_dir × tan(half_angle) × arr_len\n  → 8 vertices per arm = 24 total"]

    AH --> PS["Plane squares (if style.show_planes)\n  (a, b) = plane.axes()\n  4 corners at (PLANE_OFF, PLANE_OFF+SIZE offsets)\n  4 edge segments\n  → 8 vertices per plane = 24 total"]

    PS --> UP["Upload to wgpu vertex buffer\nDraw: LineList topology\ndepth_compare: Always  — gizmo always on top\ndepth_write_enabled: false  — no depth write"]
```

---

## position_matrix() vs world_matrix()

```mermaid
graph LR
    WM["world_matrix()\nMat4::from_scale_rotation_translation\n= full TRS\nGizmo SCALES with the entity\n❌ Don't use for gizmo draw"]

    PM["position_matrix()\nMat4::from_translation(position)\n= translation only\nGizmo always fixed-size, world-aligned\n✅ Used in GizmoDraw.transform"]

    WM -->|"includes entity scale → handles grow"| BAD["handles proportional to entity ❌"]
    PM -->|"strips scale/rotation → handles fixed"| GOOD["handles always ARM_LEN units ✅"]
```

---

## File Reference

| File | Role |
|---|---|
| `ferrous_core/src/scene/gizmo.rs` | `GizmoState`, `GizmoStyle`, `AxisColors`, `PlaneColors`, `GizmoMode`, `Axis`, `Plane`, `axis_vector()` |
| `ferrous_core/src/scene/mod.rs` | Re-exports `GizmoStyle`, `AxisColors`, `PlaneColors`, `Axis`, `Plane`, `GizmoMode`, `GizmoState`, `axis_vector` |
| `ferrous_renderer/src/scene/gizmo.rs` | `GizmoDraw` — transform + mode + highlights + style |
| `ferrous_renderer/src/pipeline/gizmo.rs` | wgpu `LineList` pipeline, `depth_compare: Always`, `depth_write_enabled: false` |
| `ferrous_renderer/src/lib.rs` (`execute_gizmo_pass`) | Vertex generation — shafts, arrowheads, plane squares |
| `ferrous_app/src/context.rs` (`update_gizmo`) | Picking + drag + queue — the entire interaction API |
| `ferrous_editor/src/app.rs` (`draw_3d`) | One-liner call: `ctx.update_gizmo(sel, &mut self.gizmo)` |
````
