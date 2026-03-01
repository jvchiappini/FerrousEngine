# crate-overview

> **Description:** Complete dependency graph for all FerrousEngine crates — what each crate owns, how they connect, and why the layering exists.

---

## Crate Dependency Graph

```mermaid
graph TD
    subgraph "ferrous_core — CPU-only Logic Layer"
        CORE["ferrous_core
        ─────────────────────────────
        Scene Graph:
          World · Element · Handle
          ElementKind (Cube, Mesh, PointLight, Empty)
        Spatial:
          Transform (position: Vec3, rotation: Quat, scale: Vec3)
          transform.matrix() → Mat4
        Camera:
          Camera (eye, target, up, fovy, aspect, znear, zfar)
          CameraUniform (view_proj: [[f32;4];4])
          CameraController (WASD / orbit)
        Utilities:
          Color · Time · InputState
          EngineContext (wgpu Device+Queue shared handle)
          metrics (CPU%, RAM bytes)"]
    end

    subgraph "ferrous_assets — Resource Loading"
        ASSETS["ferrous_assets
        ─────────────────────────────
        Font:
          FontParser (TTF/OTF via ttf-parser)
          GlyphAtlas (CPU bitmap → GPU texture)
          GlyphMetrics (advance, bearing, uv rect)
        Future:
          textures · audio · meshes from disk"]
    end

    subgraph "ferrous_gui — 2D UI Layer"
        GUI["ferrous_gui
        ─────────────────────────────
        Renderer:
          GuiRenderer (wgpu 2D pipeline)
          GuiBatch (quad draw commands)
          TextBatch (glyph draw commands)
        Widgets:
          Ui (layout root)
          Widget · Button · Slider
          TextInput · ColorPicker
          ViewportWidget · Canvas
        Layout:
          LayoutNode (flexbox-style)
          Container"]
    end

    subgraph "ferrous_renderer — GPU Backend"
        RENDERER["ferrous_renderer
        ─────────────────────────────
        Scene:
          Renderer (top-level handle)
          RenderObject (id, mesh, matrix, aabb, slot)
          world_sync (World → RenderObjects, O(n))
        Geometry:
          Mesh (Arc vertex + Arc index buffers)
          Vertex ([f32;3] pos + [f32;3] color)
          primitives: cube / sphere / ...
        Culling:
          Aabb · Frustum · frustum_cull()
        Camera GPU:
          GpuCamera (wgpu::Buffer + BindGroup)
        Buffers:
          ModelBuffer (dynamic uniform, one slot/object)
        Passes:
          WorldPass (3D geometry pass)
          UiPass (2D overlay pass)
        Graph:
          FramePacket · DrawCommand
          RenderGraph (pass ordering)
        Targets:
          RenderTarget · SwapchainTarget"]
    end

    subgraph "ferrous_app — Event Loop Orchestrator"
        APP["ferrous_app
        ─────────────────────────────
        Entry:
          Runner (winit event loop)
          FerrousApp trait (setup/update/draw_ui/draw_3d)
        Per-frame:
          AppContext (InputState, Time, World, Renderer, Viewport)
          GraphicsState (wgpu Surface + Renderer)
        Auto-wired:
          renderer.sync_world(&world) — called automatically
          TimeClock.tick() — drives Time
        Config:
          AppConfig (window title, size, vsync, msaa)"]
    end

    subgraph "ferrous_editor — Concrete Application"
        EDITOR["ferrous_editor
        ─────────────────────────────
        EditorApp
          impl FerrousApp
          setup() → spawns scene objects
          update() → camera/input logic
          draw_ui() → editor panels
          draw_3d() → gizmos, overlays"]
    end

    CORE    -->|"ElementKind, Transform, Camera\nused by renderer for sync"| RENDERER
    CORE    -->|"World, InputState, Time\nexposed through AppContext"| APP
    CORE    -->|"Color, metrics\nused by widgets"| GUI
    ASSETS  -->|"GlyphAtlas, Font\nloaded at startup"| APP
    ASSETS  -->|"GlyphAtlas → texture upload\nused by GuiRenderer"| GUI
    GUI     -->|"GuiRenderer, GuiBatch, TextBatch\ndrawn in UiPass"| RENDERER
    RENDERER-->|"Renderer, RenderTarget\norchestrated per-frame"| APP
    APP     -->|"Runner, FerrousApp trait\nentry point for the editor"| EDITOR
```

---

## Layering Rules

```mermaid
graph LR
    L1["Layer 0\nferrous_core\nNo GPU deps at all\nPure Rust + glam"] 
    L2["Layer 1\nferrous_assets\nCan do CPU texture packing\nNo game logic"]
    L3["Layer 2\nferrous_gui\nKnows Color, Font\nDoes NOT know World or 3D"]
    L4["Layer 2\nferrous_renderer\nKnows Transform via World sync\nDoes NOT own game state"]
    L5["Layer 3\nferrous_app\nTies everything together\nNo game logic itself"]
    L6["Layer 4\nferrous_editor\nOnly concrete app logic\nCalls public APIs only"]

    L1 --> L2
    L1 --> L3
    L1 --> L4
    L2 --> L3
    L2 --> L5
    L3 --> L4
    L4 --> L5
    L5 --> L6
```

---

## Key Architectural Invariants

```mermaid
graph TD
    INV1["❌ ferrous_core must NEVER import wgpu\nAll GPU types live in ferrous_renderer"]
    INV2["❌ ferrous_renderer must NEVER own game state\nWorld lives exclusively in ferrous_core"]
    INV3["❌ ferrous_gui must NEVER know about 3D scene\nNo World or RenderObject imports in gui"]
    INV4["❌ ferrous_app must NEVER contain game logic\nAll behaviour lives in impl FerrousApp"]
    INV5["✅ Transform is the ONLY position source\nDerive matrix on-demand, never store separately"]
    INV6["✅ Mesh buffers are Arc-wrapped\nShared geometry = zero GPU copies per instance"]
    INV7["✅ world_sync is O(n) per frame\nInsert/remove/update separated into 3 phases"]
```
