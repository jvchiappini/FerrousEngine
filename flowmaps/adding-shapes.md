# adding-shapes

> **Description:** Step-by-step guide for adding a new primitive shape to FerrousEngine. Covers every file that must be touched, in the correct order, with no duplication of state.

---

## Overview — Where Each Concern Lives

```mermaid
graph LR
    CORE["ferrous_core\nDefines WHAT a shape IS\n(logical identity + parameters)"]
    REND["ferrous_renderer\nDefines HOW a shape is drawn\n(GPU mesh + sync mapping)"]
    APP["ferrous_app / ferrous_editor\nDefines WHEN a shape is spawned\n(user-facing API call)"]

    CORE -->|"ElementKind variant"| REND
    REND -->|"world_sync match arm"| REND
    CORE -->|"spawn_* helper"| APP
```

---

## Complete 6-Step Checklist

```mermaid
flowchart TD
    S1["Step 1 — Add ElementKind variant
    ──────────────────────────────────
    File: ferrous_core/src/scene/world.rs

    pub enum ElementKind {
        Cube { half_extent: f32 },
        Sphere { radius: f32 },       ← ADD THIS
        Mesh { path: String },
        PointLight { intensity: f32 },
        Empty,
    }"]

    S2["Step 2 — Add spawn helper on World
    ──────────────────────────────────────
    File: ferrous_core/src/scene/world.rs

    impl World {
        pub fn spawn_sphere(
            &mut self,
            name: impl Into&lt;String&gt;,
            position: Vec3,
            radius: f32,
        ) → Handle {
            let id = self.next_id();
            let element = Element {
                id,
                name: name.into(),
                transform: Transform::from_position(position),
                color: Color::WHITE,
                kind: ElementKind::Sphere { radius },
                visible: true,
                tags: vec![],
            };
            self.entities.insert(id, element);
            Handle(id)
        }
    }"]

    S3["Step 3 — Create GPU primitive
    ──────────────────────────────────
    File: ferrous_renderer/src/geometry/primitives/sphere.rs

    pub fn sphere(device: &wgpu::Device, subdivisions: u32) → Mesh {
        let (vertices, indices) = build_uv_sphere(subdivisions);
        Mesh::new(device, &vertices, &indices)
    }

    fn build_uv_sphere(subdivisions: u32) → (Vec&lt;Vertex&gt;, Vec&lt;u32&gt;) {
        // latitude / longitude loops
        // vertex: position [f32;3] + color [f32;3]
        // indices: triangle strips → indexed triangles
    }"]

    S4["Step 4 — Export the primitive
    ──────────────────────────────────
    File: ferrous_renderer/src/geometry/primitives/mod.rs

    pub mod cube;
    pub mod sphere;      ← ADD THIS
    pub use cube::cube;
    pub use sphere::sphere;  ← ADD THIS"]

    S5["Step 5 — Add sync arm in world_sync
    ──────────────────────────────────────
    File: ferrous_renderer/src/scene/world_sync.rs

    In the match block inside Phase 2 (insert new):

    ElementKind::Cube { half_extent } => {
        primitives::cube(device)
    },
    ElementKind::Sphere { radius } => {    ← ADD THIS
        primitives::sphere(device, 16)     // 16 subdivisions
    },
    ElementKind::Mesh { path } => {
        load_or_cache_mesh(device, path)
    },
    ElementKind::PointLight { .. } | ElementKind::Empty => {
        continue; // no RenderObject
    }"]

    S6["Step 6 — Use in application code
    ──────────────────────────────────────
    File: ferrous_editor/src/main.rs (or any impl FerrousApp)

    fn setup(&mut self, ctx: &mut AppContext) {
        ctx.world.spawn_sphere('Ball', Vec3::ZERO, 0.5);
    }
    // → sync_world() picks it up automatically next frame
    // → sphere mesh is created once and inserted into objects map"]

    S1 --> S2 --> S3 --> S4 --> S5 --> S6
```

---

## File Touch Map

```mermaid
graph TD
    subgraph "ferrous_core"
        FC1["world.rs\n+ ElementKind::Sphere { radius }\n+ World::spawn_sphere()"]
    end

    subgraph "ferrous_renderer"
        FR1["primitives/sphere.rs\n(new file)\npub fn sphere(device, subdivisions) → Mesh"]
        FR2["primitives/mod.rs\npub mod sphere;\npub use sphere::sphere;"]
        FR3["scene/world_sync.rs\n+ match arm for ElementKind::Sphere"]
    end

    subgraph "ferrous_editor (optional)"
        FE1["main.rs\nctx.world.spawn_sphere(...)"]
    end

    FC1 -->|"used by"| FR3
    FR1 -->|"exported via"| FR2
    FR2 -->|"called in"| FR3
    FR3 -->|"RenderObject created"| FE1
```

---

## AABB for New Shapes

```mermaid
flowchart LR
    subgraph "Aabb Computation (per shape)"
        CUBE["Cube { half_extent: f32 }
        local_aabb = Aabb {
            min: Vec3::splat(-half_extent),
            max: Vec3::splat(half_extent),
        }"]
        SPHERE["Sphere { radius: f32 }
        local_aabb = Aabb {
            min: Vec3::splat(-radius),
            max: Vec3::splat(radius),
        }"]
        MESH["Mesh { path }
        local_aabb = compute from vertex bounds
        (min/max of all vertex positions)"]
    end

    CUBE & SPHERE & MESH -->|"stored in RenderObject.local_aabb"| CULL["Frustum culling
    transform_aabb(local_aabb, matrix)
    → world-space AABB
    test against 6 frustum planes"]
```

---

## Primitive Vertex Format

```mermaid
graph LR
    V["Vertex
    position: [f32; 3]   ← XYZ world-space
    color:    [f32; 3]   ← RGB (0.0–1.0)
    ─────────────────────
    Size: 24 bytes each
    VertexBufferLayout stride: 24"]

    WD["wgpu::VertexBufferLayout
    array_stride: 24
    step_mode: Vertex
    attributes:
      [0] Float32x3 offset 0  → position
      [1] Float32x3 offset 12 → color"]

    V --> WD
```

---

## Shape Does Not Exist in Both Layers

```mermaid
graph TD
    WRONG["❌ WRONG PATTERN
    ─────────────────────────────────────
    struct Sphere {
        radius: f32,        ← duplicate
        position: Vec3,     ← duplicate (lives in Transform)
        id: u32,            ← duplicate (lives in Element)
        mesh: Mesh,         ← belongs in RenderObject
    }
    This duplicates Element + Transform + RenderObject.
    Adding this as a standalone struct is what was done
    with the now-deleted ferrous_core/elements/cube.rs.
    DO NOT repeat this pattern."]

    CORRECT["✅ CORRECT PATTERN
    ─────────────────────────────────────
    ferrous_core: ElementKind::Sphere { radius: f32 }
      → only the shape parameter, nothing else

    ferrous_renderer: RenderObject { mesh: Mesh, matrix, slot }
      → only GPU resources, no logical state

    ferrous_core: Element { id, name, transform, color, kind }
      → single canonical record, one place to update"]

    WRONG -->|"replaced by"| CORRECT
```

---

## Testing New Shapes

```mermaid
flowchart TD
    T1["Unit test in ferrous_renderer:
    #[test]
    fn sphere_primitive_generates_valid_mesh() {
        let device = mock_device();
        let mesh = primitives::sphere(&device, 4);
        assert!(mesh.index_count > 0);
        // verify indices are within vertex bounds
    }"]

    T2["Integration test in ferrous_editor:
    spawn a sphere at (0,0,0)
    call sync_world()
    assert objects map contains the new id
    assert render_object.mesh.index_count > 0"]

    T1 --> T2
```
