# frame-loop

> **Description:** Complete lifecycle of a single rendered frame — from winit event to GPU present. Covers the event loop, update phase, world sync, draw calls, and all data flowing through each stage.

---

## High-Level Frame Sequence

```mermaid
sequenceDiagram
    participant OS  as OS / winit
    participant RUN as Runner (ferrous_app)
    participant APP as impl FerrousApp (user code)
    participant CTX as AppContext
    participant CORE as ferrous_core (World)
    participant REND as ferrous_renderer (Renderer)
    participant GUI as ferrous_gui (GuiBatch)
    participant GPU as wgpu / GPU

    OS->>RUN: WindowEvent::RedrawRequested

    Note over RUN: --- Update Phase ---
    RUN->>RUN: clock.tick() → Time { delta_seconds, fps, total_seconds }
    RUN->>RUN: build AppContext { &input, time, &mut world, Some(&mut renderer), viewport }
    RUN->>APP: app.update(&mut ctx)
    APP->>CTX: ctx.world.spawn_cube("Box", pos) → Handle(u64)
    APP->>CTX: ctx.world.get_element_mut(h).transform.position = ...
    APP->>CTX: ctx.renderer.handle_input(&input, dt)
    APP-->>RUN: (returns, world is mutated)

    Note over RUN: --- Sync Phase (automatic) ---
    RUN->>REND: renderer.sync_world(&world)
    REND->>REND: Phase 1 — remove stale RenderObjects (id not in World)
    REND->>REND: Phase 2 — insert new RenderObjects (id in World, not in objects map)
    REND->>REND:   ElementKind::Cube → primitives::cube(device) → Mesh
    REND->>GPU:   ModelBuffer.write(queue, slot, &element.transform.matrix())
    REND->>REND:   RenderObject::new(id, mesh, matrix, aabb, slot)
    REND->>REND: Phase 3 — update matrices (element.transform changed)
    REND->>GPU:   ModelBuffer.write(queue, slot, &new_matrix)

    Note over RUN: --- 3D Draw Phase ---
    RUN->>APP: app.draw_3d(&mut renderer, &mut ctx)
    APP->>REND: renderer.spawn_object(...)  [optional extra objects]

    Note over RUN: --- UI Build Phase ---
    RUN->>APP: app.draw_ui(&mut gui_batch, &mut text_batch)
    APP->>GUI: batch.quad(rect, color)
    APP->>GUI: text_batch.text("hello", pos, font)
    RUN->>GUI: ui.draw(&mut gui_batch, &mut text_batch)

    Note over RUN: --- Render Phase ---
    RUN->>REND: renderer.begin_frame() → (SurfaceTexture, CommandEncoder)
    REND->>REND: build_base_packet() → FramePacket { draw_commands: Vec<DrawCommand> }
    REND->>REND:   frustum_cull(objects, &camera_frustum) — skip off-screen objects
    REND->>GPU: GpuCamera.sync(queue, &camera) — upload view_proj matrix
    REND->>GPU: WorldPass.execute(encoder, &packet)
    REND->>GPU:   set_pipeline(base_pipeline)
    REND->>GPU:   set_bind_group(0, &camera_bind_group)
    REND->>GPU:   per DrawCommand: set_bind_group(1, model_slot_offset), draw_indexed()
    REND->>GPU: UiPass.execute(encoder, &gui_batch, &text_batch)
    REND->>GPU:   set_pipeline(gui_pipeline)
    REND->>GPU:   upload quads, draw rects + glyphs
    REND->>GPU: queue.submit(encoder.finish())
    GPU-->>RUN: surface_texture.present()
    RUN->>OS: request_redraw()
```

---

## Data Flow Per Stage

```mermaid
flowchart TD
    subgraph "Input Stage"
        EV["winit WindowEvent\n(keyboard, mouse, resize)"]
        IS["InputState\n  keys_held: HashSet\n  mouse_delta: Vec2\n  scroll: f32"]
        EV -->|"Runner collects into"| IS
    end

    subgraph "Update Stage — app.update(ctx)"
        W["World\n  entities: HashMap&lt;u64, Element&gt;\n  next_id: AtomicU64"]
        TF["Element.transform\n  position: Vec3\n  rotation: Quat\n  scale: Vec3"]
        EL["Element\n  id: u64\n  name: String\n  color: Color\n  kind: ElementKind\n  visible: bool\n  tags: Vec&lt;String&gt;"]
        IS --> W
        W --> EL
        EL --> TF
    end

    subgraph "Sync Stage — sync_world(&world)"
        SS["world_sync::sync_world\n  removed: ids in objects but not world\n  added: ids in world but not objects\n  updated: ids in both, matrix changed"]
        MB["ModelBuffer\n  wgpu::Buffer (dynamic uniform)\n  slot → byte_offset = slot * 256\n  write(queue, slot, &mat4)"]
        RO["RenderObject\n  id: u64\n  mesh: Mesh (Arc buffers)\n  matrix: Mat4 (current snapshot)\n  local_aabb: Aabb\n  slot: usize"]
        TF -->|"transform.matrix() → Mat4"| SS
        SS --> RO
        SS --> MB
    end

    subgraph "Render Stage — begin_frame → present"
        FP["FramePacket\n  draw_commands: Vec&lt;DrawCommand&gt;\n  (frustum culled)"]
        DC["DrawCommand\n  slot: usize → dynamic offset\n  index_count: u32\n  base_vertex: i32"]
        CAM["GpuCamera\n  buffer: wgpu::Buffer (64 bytes)\n  bind_group: BindGroup\n  view_proj: Mat4"]
        WP["WorldPass\n  pipeline: RenderPipeline\n  per cmd: bind model slot → draw"]
        UP["UiPass\n  pipeline: RenderPipeline\n  quads + glyphs"]
        SC["SurfaceTexture\n  → present()"]
        RO --> FP
        FP --> DC
        DC --> WP
        CAM --> WP
        WP --> UP
        UP --> SC
    end
```

---

## Timing and Frame Budget

```mermaid
flowchart LR
    subgraph "Per Frame Cost (approximate)"
        T1["clock.tick()\n~1 µs"]
        T2["app.update()\nuser-defined\ntypically < 1 ms"]
        T3["sync_world()\nO(n) — n = dirty objects\ntypically < 0.5 ms"]
        T4["begin_frame()\nget swapchain texture\n~0.1 ms"]
        T5["build_base_packet()\nfrustum cull O(n)\n~0.1 ms per 1000 objects"]
        T6["GpuCamera.sync()\n1 queue.write_buffer\n~1 µs"]
        T7["WorldPass + UiPass\nGPU-side\n~1–5 ms at 60fps"]
        T8["queue.submit + present\n~0.1 ms CPU"]
        T1 --> T2 --> T3 --> T4 --> T5 --> T6 --> T7 --> T8
    end
```

---

## Window Resize Handling

```mermaid
sequenceDiagram
    participant OS  as winit
    participant RUN as Runner
    participant GFX as GraphicsState
    participant REND as Renderer
    participant CAM  as GpuCamera

    OS->>RUN: WindowEvent::Resized(new_size)
    RUN->>GFX: graphics_state.resize(new_size)
    GFX->>GFX: surface.configure(device, &new_config)
    GFX->>REND: renderer.resize(new_size)
    REND->>CAM: GpuCamera recreates projection (new aspect ratio)
    RUN->>RUN: viewport.width / height updated in AppContext
```

---

## FerrousApp Trait — Hook Points

```mermaid
graph TD
    TR["FerrousApp trait"]
    S["setup(&mut self, ctx: &mut AppContext)\nCalled once at startup\n→ load assets, spawn initial scene"]
    U["update(&mut self, ctx: &mut AppContext)\nCalled every frame\n→ handle input, modify World"]
    D3["draw_3d(&mut self, renderer: &mut Renderer, ctx: &mut AppContext)\nCalled every frame after sync\n→ add runtime render objects, gizmos"]
    DU["draw_ui(&mut self, gui: &mut GuiBatch, text: &mut TextBatch)\nCalled every frame before render\n→ build UI quads and text"]

    TR --> S
    TR --> U
    TR --> D3
    TR --> DU
```

