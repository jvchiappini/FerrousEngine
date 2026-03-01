# crate-responsibilities

> **Description:** What each crate owns, what it must never do, and the precise boundary rules that keep the engine maintainable. Each crate has a single primary concern.

---

## ferrous_core — CPU Logic Layer

```mermaid
graph LR
    subgraph "ferrous_core  ✅ OWNS"
        C1["World\n  HashMap&lt;u64, Element&gt;\n  next_id: AtomicU64\n  spawn_* helpers"]
        C2["Element\n  id: u64\n  name: String\n  transform: Transform\n  color: Color\n  kind: ElementKind\n  visible: bool\n  tags: Vec&lt;String&gt;"]
        C3["ElementKind (enum)\n  Cube { half_extent: f32 }\n  Mesh { path: String }\n  PointLight { intensity: f32 }\n  Empty"]
        C4["Transform\n  position: Vec3\n  rotation: Quat\n  scale: Vec3\n  fn matrix() → Mat4"]
        C5["Camera\n  eye, target, up: Vec3\n  fovy, aspect: f32\n  znear, zfar: f32"]
        C6["CameraController\n  WASD + mouse orbit\n  fn process_input(&input, dt) → bool"]
        C7["CameraUniform\n  view_proj: [[f32;4];4]\n  fn update_view_proj(&camera)"]
        C8["EngineContext\n  device: Arc&lt;Device&gt;\n  queue: Arc&lt;Queue&gt;\n  (shared wgpu handle only)"]
        C9["Utilities\n  Color, Time, InputState\n  metrics (CPU%, RAM)\n  Viewport { width, height }"]
    end

    subgraph "ferrous_core  ❌ MUST NEVER"
        X1["❌ Import wgpu types directly\n(except through EngineContext Arc)"]
        X2["❌ Create GPU buffers or bind groups"]
        X3["❌ Know about Mesh, Vertex, RenderObject"]
        X4["❌ Own render pass logic"]
        X5["❌ Duplicate spatial data\n(no separate 'position' outside Transform)"]
    end
```

---

## ferrous_renderer — GPU Backend

```mermaid
graph LR
    subgraph "ferrous_renderer  ✅ OWNS"
        R1["Mesh\n  vertices: Arc&lt;wgpu::Buffer&gt;\n  indices: Arc&lt;wgpu::Buffer&gt;\n  index_count: u32\n  (shared via Arc — no copies per instance)"]
        R2["Vertex\n  position: [f32;3]\n  color: [f32;3]"]
        R3["RenderObject\n  id: u64\n  mesh: Mesh\n  matrix: Mat4 (snapshot, derived from Transform)\n  local_aabb: Aabb\n  slot: usize (ModelBuffer slot)"]
        R4["GpuCamera\n  buffer: wgpu::Buffer (64 bytes)\n  bind_group: BindGroup\n  fn sync(queue, &Camera)"]
        R5["ModelBuffer\n  buffer: wgpu::Buffer (dynamic uniform)\n  capacity: usize (slots)\n  fn write(queue, slot, &Mat4)"]
        R6["world_sync\n  Three-phase O(n) reconciliation:\n  1. remove stale\n  2. insert new → create Mesh\n  3. update matrices"]
        R7["Passes\n  WorldPass (3D geometry)\n  UiPass (2D overlay)"]
        R8["Culling\n  Aabb { min, max }\n  Frustum { planes: [Plane;6] }\n  frustum_cull() → visible subset"]
        R9["Primitives\n  cube(device) → Mesh\n  sphere(device, subdivisions) → Mesh"]
        R10["FramePacket\n  draw_commands: Vec&lt;DrawCommand&gt;\n  (built after cull, consumed by WorldPass)"]
    end

    subgraph "ferrous_renderer  ❌ MUST NEVER"
        Y1["❌ Store game state\n(no flags like 'is_selected', 'hp')"]
        Y2["❌ Own Transform or modify positions\n(reads World, never writes it back)"]
        Y3["❌ Know about UI widgets\n(GuiBatch passed in from outside)"]
        Y4["❌ Duplicate position data\n(position = matrix.w_axis, derived on demand)"]
        Y5["❌ Create its own Camera logic\n(Camera CPU state lives in ferrous_core)"]
    end
```

---

## ferrous_app — Event Loop Orchestrator

```mermaid
graph LR
    subgraph "ferrous_app  ✅ OWNS"
        A1["Runner\n  winit EventLoop\n  owns GraphicsState\n  calls FerrousApp hooks in order"]
        A2["GraphicsState\n  wgpu Surface + SurfaceConfig\n  owns Renderer instance\n  handles resize events"]
        A3["AppContext&lt;'a&gt;\n  &InputState\n  Time { delta, fps, total }\n  &mut World\n  Option&lt;&mut Renderer&gt;\n  Viewport { width, height }"]
        A4["FerrousApp trait\n  setup() · update()\n  draw_3d() · draw_ui()"]
        A5["TimeClock\n  Instant-based\n  drives Time every frame"]
        A6["AppConfig\n  title: String\n  width, height: u32\n  vsync: bool\n  msaa: u32"]
    end

    subgraph "ferrous_app  ❌ MUST NEVER"
        Z1["❌ Contain game logic\n(no entity manipulation, no physics)"]
        Z2["❌ Know about specific ElementKinds\n(just passes World through)"]
        Z3["❌ Render directly\n(delegates to Renderer methods only)"]
    end
```

---

## ferrous_gui — 2D UI Layer

```mermaid
graph LR
    subgraph "ferrous_gui  ✅ OWNS"
        G1["GuiRenderer\n  wgpu pipeline for 2D quads\n  wgpu pipeline for text glyphs\n  vertex + index buffers (dynamic)"]
        G2["GuiBatch\n  Vec&lt;GuiQuad&gt; — colored rectangles\n  built by user each frame\n  consumed by UiPass"]
        G3["TextBatch\n  Vec&lt;GlyphQuad&gt; — per-character quads\n  references GlyphAtlas UV coords"]
        G4["Widget system\n  Ui (layout root)\n  Button, Slider, TextInput\n  ColorPicker, Canvas\n  ViewportWidget (3D viewport embed)"]
        G5["Layout\n  LayoutNode (flexbox-inspired)\n  Container (nesting)"]
    end

    subgraph "ferrous_gui  ❌ MUST NEVER"
        W1["❌ Import World or Element\n(knows nothing about the 3D scene)"]
        W2["❌ Hold GPU resources directly\n(GuiRenderer wraps them, widgets use batches)"]
        W3["❌ Perform 3D math\n(no Mat4, no frustum, no Vec3 in layout)"]
    end
```

---

## ferrous_assets — Resource Loading

```mermaid
graph LR
    subgraph "ferrous_assets  ✅ OWNS"
        AS1["Font\n  raw TTF/OTF bytes\n  parsed glyph outlines via ttf-parser"]
        AS2["GlyphAtlas\n  CPU bitmap (u8 array)\n  GPU wgpu::Texture + TextureView\n  GlyphMetrics map (char → UV rect)"]
        AS3["FontParser\n  build_atlas(font_data) → GlyphAtlas\n  rasterizes glyphs at given px size"]
    end

    subgraph "ferrous_assets  ❌ MUST NEVER"
        V1["❌ Know about World or scene logic"]
        V2["❌ Own render pipelines\n(textures passed to GuiRenderer)"]
        V3["❌ Duplicate glyph bitmaps\n(one GlyphAtlas per font+size pair)"]
    end
```

---

## ferrous_editor — Concrete App

```mermaid
graph LR
    subgraph "ferrous_editor  ✅ OWNS"
        ED1["EditorApp\n  impl FerrousApp\n  owns app-level state"]
        ED2["setup()\n  load fonts via ferrous_assets\n  spawn initial scene objects"]
        ED3["update()\n  WASD / orbit camera\n  UI input handling"]
        ED4["draw_ui()\n  inspector panel\n  hierarchy panel\n  performance overlay"]
        ED5["draw_3d()\n  gizmos (move handles)\n  selection highlight\n  grid overlay"]
    end

    subgraph "ferrous_editor  ❌ MUST NEVER"
        E1["❌ Directly call wgpu APIs\n(only through Renderer and GuiRenderer)"]
        E2["❌ Implement its own sync logic\n(sync_world() is automatic in Runner)"]
    end
```

---

## Boundary Enforcement Summary

```mermaid
graph TD
    B1["ferrous_core\nno Cargo.toml dep on wgpu"]
    B2["ferrous_renderer\nCargo dep: wgpu, glam, ferrous_core"]
    B3["ferrous_gui\nCargo dep: wgpu, ferrous_core, ferrous_assets"]
    B4["ferrous_app\nCargo dep: ferrous_core, ferrous_renderer, ferrous_gui, ferrous_assets"]
    B5["ferrous_editor\nCargo dep: ferrous_app (all transitive)"]

    B1 -->|"no GPU"| B2
    B1 -->|"no GPU"| B3
    B2 -->|"GPU only"| B4
    B3 -->|"2D GPU"| B4
    B4 -->|"full engine"| B5
```
