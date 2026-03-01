# transform-pipeline

> **Description:** How spatial data travels from user code to the GPU — the complete Transform → Mat4 → ModelBuffer → shader pipeline. This file also documents why there is exactly one position, one rotation, and one scale per scene object.

---

## Single Source of Truth

```mermaid
flowchart LR
    subgraph "ferrous_core — CPU Source of Truth"
        TF["Element.transform : Transform
        ─────────────────────────
        position : Vec3   (3× f32, 12 bytes)
        rotation : Quat   (4× f32, 16 bytes)
        scale    : Vec3   (3× f32, 12 bytes)
        ─────────────────────────
        Total: 40 bytes per element"]
    end

    subgraph "ferrous_renderer — Derived CPU Snapshot"
        SYNC["world_sync.rs
        element.transform.matrix()
        → Mat4 (TRS decomposed)"]
        RO["RenderObject.matrix : Mat4
        ─────────────────────────
        16× f32 = 64 bytes per object
        Derived snapshot only —
        NOT authoritative"]
        MB["ModelBuffer slot N
        ─────────────────────────
        wgpu::Buffer (dynamic uniform)
        byte_offset = slot × 256
        write(queue, slot, &mat4)"]
    end

    subgraph "GPU — Shader Input"
        SH["base.wgsl
        @group(1) @binding(0)
        var&lt;uniform&gt; model: ModelUniform;
        struct ModelUniform { mat: mat4x4&lt;f32&gt; }
        ──────────────────────
        let world_pos = model.mat * vec4(pos, 1.0);"]
    end

    TF -->|"fn matrix() builds\nMat4::from_scale_rotation_translation\nno allocation"| SYNC
    SYNC --> RO
    RO -->|"ModelBuffer.write(queue, slot, &mat)"| MB
    MB -->|"@group(1) dynamic offset\nbind per draw call"| SH
```

---

## matrix() Decomposition

```mermaid
flowchart TD
    P["position: Vec3\n(tx, ty, tz)"]
    R["rotation: Quat\n(qx, qy, qz, qw)"]
    S["scale: Vec3\n(sx, sy, sz)"]
    MAT["Mat4 = T × R × S
    ─────────────────────────────────────────
    | sx·r00  sy·r01  sz·r02  tx |
    | sx·r10  sy·r11  sz·r12  ty |
    | sx·r20  sy·r21  sz·r22  tz |
    |   0       0       0      1 |
    ─────────────────────────────────────────
    glam: Mat4::from_scale_rotation_translation(s, r, p)"]

    P --> MAT
    R --> MAT
    S --> MAT
```

---

## Position Derivation (No Redundant Storage)

```mermaid
flowchart LR
    RO["RenderObject\n  matrix: Mat4"]
    WX["matrix.w_axis\n  x = tx (world X)\n  y = ty (world Y)\n  z = tz (world Z)\n  w = 1.0"]
    POS["Vec3 { x: w.x, y: w.y, z: w.z }\nDerived on demand\nNever stored separately"]

    RO -->|"get_object_position(id)"| WX
    WX --> POS
```

> **Rule:** `RenderObject` does **not** have a `position: Vec3` field.  
> Position is extracted from `matrix.w_axis.xyz` when needed.  
> This eliminates 12 bytes/object of redundant storage and a potential desync bug where `position != matrix.translation`.

---

## Three-Phase Sync in Detail

```mermaid
flowchart TD
    subgraph "Phase 1 — Remove Stale"
        P1A["Iterate objects map keys"]
        P1B["If id NOT in world.entities → remove RenderObject\nfree ModelBuffer slot"]
        P1A --> P1B
    end

    subgraph "Phase 2 — Insert New"
        P2A["Iterate world.entities"]
        P2B["If id NOT in objects map:"]
        P2C["match element.kind:
        Cube { half_extent } → primitives::cube(device)
        Mesh { path } → load/cache mesh
        PointLight → no RenderObject (light-only)
        Empty → no RenderObject"]
        P2D["alloc_slot() → usize"]
        P2E["RenderObject::new(id, mesh, matrix, aabb, slot)"]
        P2F["ModelBuffer.write(queue, slot, &matrix)"]
        P2G["objects.insert(id, render_object)"]
        P2A --> P2B --> P2C --> P2D --> P2E --> P2F --> P2G
    end

    subgraph "Phase 3 — Update Matrices"
        P3A["Iterate objects map"]
        P3B["Fetch element from world by id"]
        P3C["new_matrix = element.transform.matrix()"]
        P3D["If new_matrix != render_object.matrix:"]
        P3E["render_object.matrix = new_matrix"]
        P3F["ModelBuffer.write(queue, slot, &new_matrix)"]
        P3A --> P3B --> P3C --> P3D --> P3E --> P3F
    end

    Phase1 --> Phase2 --> Phase3
```

---

## ModelBuffer Memory Layout

```mermaid
flowchart LR
    subgraph "wgpu::Buffer (dynamic uniform)"
        S0["Slot 0 | bytes 0–63\nMat4 for object #0\n+ padding to 256"]
        S1["Slot 1 | bytes 256–319\nMat4 for object #1\n+ padding to 256"]
        S2["Slot 2 | bytes 512–575\nMat4 for object #2\n+ padding to 256"]
        SN["Slot N | bytes N×256 …\nMat4 for object #N"]
    end

    DC["DrawCommand { slot: usize }\n→ dynamic_offset = slot × 256\nset_bind_group(1, &bg, &[dynamic_offset])"]

    S0 & S1 & S2 & SN --> DC
```

> **Why 256-byte alignment?** `wgpu` requires dynamic uniform offsets to be aligned to `min_uniform_buffer_offset_alignment`, which is 256 bytes on most devices.

---

## Transform Modification Flow (User Perspective)

```mermaid
sequenceDiagram
    participant APP as User Code (impl FerrousApp)
    participant W   as World (ferrous_core)
    participant E   as Element.transform
    participant SS  as world_sync (ferrous_renderer)
    participant MB  as ModelBuffer
    participant GPU as GPU

    APP->>W: world.get_element_mut(handle)
    W-->>APP: &mut Element
    APP->>E: element.transform.position = Vec3::new(1, 2, 3)
    Note over E: transform is dirty (no explicit flag — sync compares matrices)

    Note over APP,GPU: Next frame — automatic sync

    APP->>SS: (Runner calls) renderer.sync_world(&world)
    SS->>E: element.transform.matrix() → new_mat
    SS->>SS: new_mat != render_object.matrix → update
    SS->>MB: ModelBuffer.write(queue, slot, &new_mat)
    MB->>GPU: wgpu::Queue.write_buffer(offset)
    Note over GPU: GPU now uses updated world matrix
```

---

## Scale and Rotation Examples

```mermaid
graph TD
    EX1["Uniform scale 2×\n  transform.scale = Vec3::splat(2.0)\n  → mat4 diagonal = [2,2,2,1]"]
    EX2["90° Y rotation\n  transform.rotation = Quat::from_rotation_y(PI/2)\n  → right column swapped"]
    EX3["Billboard (always faces camera)\n  rotation = Quat::from_rotation_arc(forward, cam_dir)\n  Computed in update(), stored in transform.rotation"]
    EX4["Non-uniform scale + rotation\n  transform.scale = Vec3::new(1, 2, 0.5)\n  transform.rotation = Quat::from_euler(...)"]
```
