# FerrousEngine — Mapa de Flujo de Crates

> **Concepto central:** `ferrous_core` actúa como la **API pública** (tipo Spigot):
> define todos los tipos lógicos del juego sin tocar GPU.
> `ferrous_renderer` es el **backend GPU** exclusivo: habla con wgpu, genera buffers y ejecuta los render passes.
> `ferrous_app` es el **orquestador** que conecta ambos en el loop principal.

---

## Diagrama de dependencias entre crates

```mermaid
graph TD
    subgraph "Dependencias de crates"
        CORE["ferrous_core\n───────────────\nTransform · Color · Time\nInputState · World · Element\nElementKind · Handle\nCamera · CameraUniform\nController · EngineContext\nmetrics"]

        ASSETS["ferrous_assets\n───────────────\nFont · FontParser\nGlyphAtlas"]

        GUI["ferrous_gui\n───────────────\nGuiRenderer · GuiBatch\nTextBatch · GuiQuad\nUi · Widget · Button\nSlider · TextInput\nViewportWidget"]

        RENDERER["ferrous_renderer\n───────────────\nRenderer · RenderTarget\nMesh · Vertex\nRenderObject · Aabb · Frustum\nGpuCamera · ModelBuffer\nWorldPass · UiPass\nFramePacket · DrawCommand\nworld_sync"]

        APP["ferrous_app\n───────────────\nApp · AppConfig\nAppContext · Runner\nFerrousApp trait\nGraphicsState"]

        EDITOR["ferrous_editor\n───────────────\nEditorApp\nimpl FerrousApp"]

        CORE --> GUI
        CORE --> RENDERER
        CORE --> APP
        ASSETS --> APP
        ASSETS --> GUI
        GUI --> RENDERER
        RENDERER --> APP
        APP --> EDITOR
    end
```

---

## Flujo completo de un frame — "Crear un cubo"

```mermaid
sequenceDiagram
    participant ED as ferrous_editor
    participant APP as ferrous_app (Runner)
    participant CORE as ferrous_core (World)
    participant REND as ferrous_renderer
    participant GPU as wgpu / GPU

    Note over ED,GPU: Llamada de usuario: ctx.world.spawn_cube("Cube", pos)

    ED->>APP: draw_3d(&mut renderer, &mut ctx)
    APP->>CORE: ctx.world.spawn_cube("Cube", Vec3)
    CORE->>CORE: next_id() → u64
    CORE->>CORE: Element { id, name, Transform{pos}, ElementKind::Cube{half_extent} }
    CORE->>CORE: entities.insert(id, element)
    CORE-->>APP: Handle(u64)

    Note over APP,REND: Auto-sync al final del update()

    APP->>REND: renderer.sync_world(&world)
    REND->>REND: world_sync::sync_world(...)
    REND->>REND: Phase 1 — retiene sólo IDs que siguen en World
    REND->>REND: Phase 2 — element.kind == Cube → create_cube(device) → Mesh
    REND->>REND: RenderObject::new(id, mesh, matrix, slot)
    REND->>GPU: ModelBuffer.write(queue, slot, &matrix)
    REND->>REND: objects.insert(id, RenderObject)

    Note over APP,GPU: Render del frame

    APP->>REND: renderer.begin_frame() → CommandEncoder
    REND->>REND: build_base_packet() — frustrumcull → DrawCommands
    REND->>GPU: GpuCamera.sync(queue, &camera) — upload view_proj
    REND->>GPU: WorldPass.execute(encoder, &packet)
    REND->>GPU: UiPass.execute(encoder, &gui_batch)
    REND->>GPU: queue.submit(encoder.finish())
    GPU-->>APP: frame.present()
```

---

## Responsabilidades por crate (lo que DEBE y NO DEBE vivir en cada una)

```mermaid
graph LR
    subgraph "ferrous_core — API lógica (sin GPU)"
        direction TB
        C1["✅ World, Element, Handle"]
        C2["✅ ElementKind (Cube, Sphere, Mesh...)"]
        C3["✅ Transform (position, rotation, scale)"]
        C4["✅ Camera (eye, target, fovy, znear, zfar)"]
        C5["✅ CameraUniform (view_proj matrix CPU)"]
        C6["✅ Color, Time, InputState"]
        C7["✅ EngineContext (wgpu device+queue — compartido)"]
        C8["✅ Controller (WASD, mappings)"]
        C9["✅ metrics (CPU/RAM)"]
        C10["❌ NO buffers GPU · NO pipelines · NO shaders"]
    end

    subgraph "ferrous_renderer — Backend GPU exclusivo"
        direction TB
        R1["✅ Mesh (Arc vertex+index buffers)"]
        R2["✅ Vertex ([f32;3] pos + [f32;3] color)"]
        R3["✅ RenderObject (id, mesh, matrix, slot, AABB)"]
        R4["✅ GpuCamera (wgpu::Buffer + BindGroup)"]
        R5["✅ ModelBuffer (dynamic uniform buffer por objeto)"]
        R6["✅ WorldPass / UiPass (render passes)"]
        R7["✅ world_sync (World → RenderObject reconciliación)"]
        R8["✅ primitives/cube.rs, sphere.rs... (geometría GPU)"]
        R9["✅ Frustum culling (Aabb, AABB transform)"]
        R10["❌ NO lógica de juego · NO Transform propio · NO posición propia"]
    end

    subgraph "ferrous_app — Orquestador del loop"
        direction TB
        A1["✅ Runner (event loop, winit, frame timing)"]
        A2["✅ GraphicsState (Surface + Renderer)"]
        A3["✅ AppContext (vista unificada por frame)"]
        A4["✅ FerrousApp trait (setup/update/draw_ui/draw_3d)"]
        A5["✅ Auto sync: renderer.sync_world(&world)"]
        A6["❌ NO lógica de juego propia · sólo orquesta"]
    end

    subgraph "ferrous_gui — Widgets 2D"
        direction TB
        G1["✅ GuiRenderer (wgpu pipeline 2D)"]
        G2["✅ GuiBatch / TextBatch (comandos 2D por frame)"]
        G3["✅ Ui, Widget, Button, Slider, TextInput..."]
        G4["❌ NO conoce la escena 3D · NO conoce World"]
    end

    subgraph "ferrous_assets — Carga de recursos"
        direction TB
        AS1["✅ Font (TTF/OTF parser, GlyphAtlas, GPU textura)"]
        AS2["❌ futuro: texturas, audio, meshes desde archivo"]
    end
```

---

## Flujo de datos por capa cada frame

```mermaid
flowchart TD
    INPUT["🖱️ Input (teclado, ratón)\nwinit WindowEvent"]

    subgraph "ferrous_app — Runner.render_frame()"
        CLOCK["TimeClock.tick() → Time{delta, fps}"]
        UPDATE["app.update(&mut AppContext)\n→ modifica World, cámara, viewport"]
        SYNCW["renderer.sync_world(&world)\n→ reconcilia World ↔ RenderObject"]
        CAMINPUT["renderer.handle_input(&input, dt)\n→ mueve Camera (orbit/WASD)"]
        DRAW3D["app.draw_3d(&mut renderer, &mut ctx)\n→ spawns adicionales, efectos"]
        DRAWUI["app.draw_ui(&mut GuiBatch, &mut TextBatch)\n→ widgets, texto HUD"]
        UISYS["ui.draw() → sistema de layout\n→ llena GuiBatch desde widgets"]
    end

    subgraph "ferrous_renderer — begin_frame → render_to_view"
        ENCODE["begin_frame() → CommandEncoder"]
        CAMUPLOAD["GpuCamera.sync(queue, &camera)\n→ upload view_proj a GPU"]
        CULL["build_base_packet()\n→ frustum cull + DrawCommands"]
        WORLDPASS["WorldPass.execute(encoder)\n→ set_pipeline, bind camera,\n   per-object: bind model slot,\n   draw(index_count)"]
        UIPASS["UiPass.execute(encoder)\n→ GuiRenderer render quads+texto"]
        SUBMIT["queue.submit(encoder.finish())\nframe.present()"]
    end

    subgraph "GPU"
        VS["Vertex Shader\nbase.wgsl / gui.wgsl / text.wgsl"]
        FS["Fragment Shader"]
        FB["Framebuffer → pantalla"]
    end

    INPUT --> CLOCK
    CLOCK --> UPDATE
    UPDATE --> SYNCW
    SYNCW --> CAMINPUT
    CAMINPUT --> DRAW3D
    DRAW3D --> DRAWUI
    DRAWUI --> UISYS
    UISYS --> ENCODE
    ENCODE --> CAMUPLOAD
    CAMUPLOAD --> CULL
    CULL --> WORLDPASS
    WORLDPASS --> UIPASS
    UIPASS --> SUBMIT
    SUBMIT --> VS
    VS --> FS
    FS --> FB
```

---

## Flujo de Transform — única fuente de verdad

```mermaid
flowchart LR
    subgraph "ferrous_core (CPU — fuente de verdad)"
        T["Element.transform\nTransform {\n  position: Vec3,\n  rotation: Quat,\n  scale: Vec3\n}"]
    end

    subgraph "ferrous_renderer (CPU — espejo derivado)"
        SYNC["world_sync.rs\nelement.transform.matrix()\n→ Mat4 (TRS)"]
        OBJ["RenderObject.matrix: Mat4\n(derivado del Transform,\n NO almacena position propia)"]
        MB["ModelBuffer slot N\n(wgpu::Buffer dynamic uniform)"]
    end

    subgraph "GPU"
        SHADER["base.wgsl\nuniform Model { model: mat4x4 }\n→ world_pos = model * vertex_pos"]
    end

    T -->|"transform.matrix()"| SYNC
    SYNC --> OBJ
    OBJ -->|"ModelBuffer.write(queue, slot, &matrix)"| MB
    MB -->|"@group(1) dynamic offset"| SHADER
```

---

## Cómo agregar un nuevo Shape (ej: Sphere)

```mermaid
flowchart TD
    S1["1️⃣ ferrous_core/src/scene/world.rs\nAgregar variante:\nElementKind::Sphere { radius: f32 }"]
    S2["2️⃣ ferrous_core/src/scene/world.rs\nAgregar helper:\nWorld::spawn_sphere(name, pos, radius)"]
    S3["3️⃣ ferrous_renderer/src/geometry/primitives/sphere.rs\nCrear fn sphere(device, subdivisions) → Mesh\n(vértices+índices GPU)"]
    S4["4️⃣ ferrous_renderer/src/geometry/primitives/mod.rs\npub mod sphere; pub use sphere::sphere;"]
    S5["5️⃣ ferrous_renderer/src/scene/world_sync.rs\nEn el match:\nElementKind::Sphere{radius} => create_sphere(device, *radius)"]
    S6["✅ ferrous_editor / cualquier app\nctx.world.spawn_sphere('Ball', pos, 0.5)\n→ se renderiza automáticamente"]

    S1 --> S2 --> S3 --> S4 --> S5 --> S6
```

---

## Problemas de duplicación resueltos / pendientes

```mermaid
graph TD
    subgraph "✅ RESUELTOS"
        FIX1["RenderObject.position: Vec3\nEliminado — se deriva de matrix.w_axis\non demand en get_object_position()"]
        FIX2["ferrous_core/elements/cube.rs\nEliminado — era un Cube{name,id,position}\nduplicando Element + Transform"]
    end

    subgraph "⚠️ ATENCIÓN — diseño correcto pero a vigilar"
        WARN1["Camera en ferrous_core\n+ Camera en ferrous_renderer\n✅ Correcto: core = lógica CPU,\nrenderer sólo re-exporta la misma struct"]
        WARN2["ModelBuffer almacena Mat4 por objeto\n+ RenderObject.matrix: Mat4\nAmbos existen en CPU — el RenderObject\nes el cache local, ModelBuffer\nes el upload buffer a GPU\n✅ Correcto por arquitectura wgpu"]
        WARN3["Renderer.camera (Camera struct)\n+ GpuCamera (buffer+bindgroup)\nNo es duplicación: Camera = estado CPU,\nGpuCamera = recursos wgpu\n✅ Correcto"]
    end

    subgraph "🔮 FUTURO — posibles duplicaciones a evitar"
        TODO1["Si se añade Sphere/Mesh con\npropiedades físicas (radio, masa),\nque vivan en ElementKind,\nNO en un struct separado"]
        TODO2["Si se añade AssetManager,\nlos Mesh deben compartirse por Arc\n(ya lo hace Mesh con Arc<wgpu::Buffer>)\nNo duplicar geometría GPU por instancia"]
    end
```
