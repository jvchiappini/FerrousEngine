# Detailed Function Extraction Mapping

## Complete Function List with Line Numbers

### 1. Camera Accessors (Lines 253-267)
```rust
impl Renderer {
    pub fn camera(&self) -> &Camera { ... }           // Lines 253-255
    pub fn camera_mut(&mut self) -> &mut Camera { ... } // Lines 259-261
    pub fn orbit_mut(&mut self) -> &mut OrbitState { ... } // Lines 265-267
}
```

### 2. Core Initialization (Lines 274-489)
```rust
pub fn new(
    context: context::EngineContext,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sample_count: u32,
    hdri_path: Option<&std::path::Path>,
) -> Self { ... }
```
**Extract to**: `renderer_core.rs`
**Dependencies**: All Renderer fields

### 3. Frame API (Lines 494-500)
```rust
pub fn begin_frame(&self) -> wgpu::CommandEncoder { ... }
```
**Extract to**: `renderer_core.rs`

### 4. Texture Management (Lines 508-637)
```rust
pub fn register_texture(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle { ... } // Lines 508-526
pub fn register_texture_linear(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle { ... } // Lines 532-550
pub fn free_texture(&mut self, handle: TextureHandle) { ... } // Lines 557-559
pub fn update_texture_data(&mut self, handle: TextureHandle, width: u32, height: u32, data: &[u8]) { ... } // Lines 623-637
```
**Extract to**: `renderer_resource.rs`

### 5. Material Management (Lines 563-617)
```rust
pub fn create_material(&mut self, desc: &MaterialDescriptor) -> MaterialHandle { ... } // Lines 563-576
pub fn free_material(&mut self, handle: MaterialHandle) { ... } // Lines 601-608
pub fn update_material_params(&mut self, handle: MaterialHandle, desc: &MaterialDescriptor) { ... } // Lines 614-617
```
**Extract to**: `renderer_resource.rs`

### 6. Mesh Management (Lines 584-594) - Conditional on `assets` feature
```rust
#[cfg(feature = "assets")]
pub fn register_mesh(&mut self, key: &str, mesh: geometry::Mesh) { ... } // Lines 584-586

#[cfg(feature = "assets")]
pub fn free_mesh(&mut self, key: &str) { ... } // Lines 592-594
```
**Extract to**: `renderer_resource.rs`

### 7. Light Management (Lines 644-738)
```rust
pub fn set_directional_light(&mut self, direction: [f32; 3], color: [f32; 3], intensity: f32) { ... } // Lines 644-656
pub fn set_point_lights(&mut self, lights: &[crate::resources::PointLightUniform]) { ... } // Lines 735-738
```
**Extract to**: `renderer_api.rs`

### 8. Render Style Management (Lines 666-729)
```rust
pub fn set_render_style(&mut self, style: RenderStyle) { ... }
```
**Extract to**: `renderer_api.rs`
**Dependencies**: Creates style passes, updates material/instance buffers

### 9. Mode Switching (Lines 747-761)
```rust
pub fn set_mode(&mut self, mode: RendererMode) { ... }
```
**Extract to**: `renderer_api.rs`

### 10. Rendering Methods (Lines 764-781) - Conditional on `gui` feature
```rust
#[cfg(feature = "gui")]
pub fn render_to_target(&mut self, encoder: &mut wgpu::CommandEncoder, ui_batch: Option<GuiBatch>) { ... } // Lines 764-770

#[cfg(feature = "gui")]
pub fn render_to_view(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, ui_batch: Option<GuiBatch>) { ... } // Lines 774-781
```
**Extract to**: `renderer_core.rs` (calls do_render)

### 11. GPU-Driven Culling (Lines 795-818) - Conditional on `gpu-driven` feature
```rust
#[cfg(feature = "gpu-driven")]
pub fn enable_gpu_culling(&mut self, enabled: bool) { ... } // Lines 795-803

#[cfg(feature = "gpu-driven")]
pub fn cull_visible_counts(&self) -> Vec<u32> { ... } // Lines 812-818
```
**Extract to**: `renderer_api.rs`

### 12. Scene Synchronization (Lines 830-1083)
```rust
pub fn set_scene(&mut self, scene: &SceneData) { ... } // Lines 830-855
pub fn sync_world(&mut self, world: &ferrous_core::scene::World) { ... } // Lines 861-1083
```
**Extract to**: `renderer_api.rs`
**Dependencies**: Complex ECS synchronization, GPU culling data upload

### 13. Gizmo Management (Lines 1094-1100)
```rust
pub fn queue_gizmo(&mut self, gizmo: scene::GizmoDraw) { ... }
```
**Extract to**: `renderer_api.rs`

### 14. Pass Management (Lines 1104-1117)
```rust
pub fn add_pass<P: RenderPass>(&mut self, mut pass: P) { ... } // Lines 1104-1112
pub fn clear_extra_passes(&mut self) { ... } // Lines 1115-1117
```
**Extract to**: `renderer_passes.rs`

### 15. Resize/Viewport (Lines 1123-1190)
```rust
pub fn resize(&mut self, new_width: u32, new_height: u32) { ... } // Lines 1123-1183
pub fn set_viewport(&mut self, vp: Viewport) { ... } // Lines 1186-1190
```
**Extract to**: `renderer_api.rs`

### 16. Configuration Helpers (Lines 1196-1207)
```rust
pub fn set_clear_color(&mut self, color: wgpu::Color) { ... } // Lines 1196-1198
pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) { ... } // Lines 1202-1207
```
**Extract to**: `renderer_api.rs`

### 17. Input Handling (Lines 1212-1214)
```rust
pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) { ... }
```
**Extract to**: `renderer_api.rs`

### 18. Private Helpers (Lines 1219-1243)
```rust
fn sync_style_material_table(&mut self) { ... } // Lines 1219-1230
fn sync_style_instance_buffer(&mut self, bg: Arc<wgpu::BindGroup>) { ... } // Lines 1233-1243
```
**Extract to**: `renderer_resource.rs`

### 19. Main Render Pipeline (Lines 1246-1517)
```rust
#[cfg(feature = "gui")]
fn do_render(&mut self, encoder: &mut wgpu::CommandEncoder, dest: RenderDest<'_>, ui_batch: Option<GuiBatch>) { ... }
```
**Extract to**: `renderer_core.rs` (main) and `renderer_passes.rs` (pass-specific logic)

## Sub-Module Extraction Plan

### `renderer_core.rs` (Lines to extract)
- `Renderer::new()` (274-489)
- `Renderer::begin_frame()` (494-500)
- `Renderer::render_to_target()` (764-770) - if gui
- `Renderer::render_to_view()` (774-781) - if gui  
- `Renderer::do_render()` (1246-1517) - if gui

### `renderer_api.rs` (Lines to extract)
- `Renderer::camera()` (253-255)
- `Renderer::camera_mut()` (259-261)
- `Renderer::orbit_mut()` (265-267)
- `Renderer::set_directional_light()` (644-656)
- `Renderer::set_render_style()` (666-729)
- `Renderer::set_point_lights()` (735-738)
- `Renderer::set_mode()` (747-761)
- `Renderer::enable_gpu_culling()` (795-803) - if gpu-driven
- `Renderer::cull_visible_counts()` (812-818) - if gpu-driven
- `Renderer::set_scene()` (830-855)
- `Renderer::sync_world()` (861-1083)
- `Renderer::queue_gizmo()` (1094-1100)
- `Renderer::resize()` (1123-1183)
- `Renderer::set_viewport()` (1186-1190)
- `Renderer::set_clear_color()` (1196-1198)
- `Renderer::set_font_atlas()` (1202-1207) - if gui
- `Renderer::handle_input()` (1212-1214)

### `renderer_resource.rs` (Lines to extract)
- `Renderer::register_texture()` (508-526)
- `Renderer::register_texture_linear()` (532-550)
- `Renderer::free_texture()` (557-559)
- `Renderer::create_material()` (563-576)
- `Renderer::register_mesh()` (584-586) - if assets
- `Renderer::free_mesh()` (592-594) - if assets
- `Renderer::free_material()` (601-608)
- `Renderer::update_material_params()` (614-617)
- `Renderer::update_texture_data()` (623-637)
- `Renderer::sync_style_material_table()` (1219-1230)
- `Renderer::sync_style_instance_buffer()` (1233-1243)

### `renderer_passes.rs` (Lines to extract)
- `Renderer::add_pass()` (1104-1112)
- `Renderer::clear_extra_passes()` (1115-1117)
- Pass coordination logic from `do_render()` (partial)

## Dependencies Analysis

### Cross-Module Dependencies
1. **`renderer_core.rs`** depends on:
   - `renderer_passes.rs` for pass execution
   - `renderer_api.rs` for camera/viewport access

2. **`renderer_api.rs`** depends on:
   - `renderer_resource.rs` for material/texture management
   - `renderer_passes.rs` for pass creation (render style)
   - `renderer_core.rs` for render pipeline

3. **`renderer_resource.rs`** depends on:
   - `renderer_passes.rs` for material table updates

4. **`renderer_passes.rs`** depends on:
   - `renderer_resource.rs` for material/instance buffer access

## Feature Matrix

| Feature | Affected Functions | Sub-Module |
|---------|-------------------|------------|
| `gui` | render_to_target, render_to_view, set_font_atlas, do_render | renderer_core, renderer_api |
| `gpu-driven` | enable_gpu_culling, cull_visible_counts | renderer_api |
| `assets` | register_mesh, free_mesh | renderer_resource |

## Recommended Implementation Order

1. **Phase 1**: Create `renderer_resource.rs` (least dependencies)
2. **Phase 2**: Create `renderer_api.rs` (depends on resource)
3. **Phase 3**: Create `renderer_passes.rs` (depends on both)
4. **Phase 4**: Create `renderer_core.rs` (depends on all)
5. **Phase 5**: Update `lib.rs` to import and re-export