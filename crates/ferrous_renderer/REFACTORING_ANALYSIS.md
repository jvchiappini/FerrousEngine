# Ferrous Renderer Module Refactoring Analysis

## Overview
This document analyzes the `ferrous_renderer/src/lib.rs` file (1518 lines) and provides a complete mapping of all functions to logical sub-modules for extraction.

## Current Structure
The `Renderer` struct contains:
- **Core fields**: context, render_target, built-in passes, camera_system
- **Scene state**: world_material_descs, instance_buf, instance_layout, gizmo_system, frame_builder  
- **Pipeline state**: format, sample_count, viewport, width, height, mode
- **Render stats and SSAO**: render_stats, prepass, ssao_pass, ssao_blur_pass, ssao_resources, ssao_enabled
- **Render style state**: render_style, cel_pass, outline_pass, flat_pass, pipeline_layouts, current_dir_light
- **GPU-driven features**: gpu_culling_enabled, cull_pass

## Recommended Sub-Module Extraction

### 1. `renderer_core.rs` - Core Renderer Implementation
**Purpose**: Core renderer lifecycle, initialization, and fundamental operations

**Lines to extract**: 270-490 (Renderer::new), 494-500 (begin_frame), 1245-1517 (do_render)

**Functions**:
- `Renderer::new()` (lines 274-489) - Complete initialization logic
- `Renderer::begin_frame()` (lines 494-500) - Command encoder creation
- `Renderer::do_render()` (lines 1246-1517) - Main render pipeline execution

**Field dependencies**:
- `context`: EngineContext
- `render_target`: RenderTarget
- `world_pass`: WorldPass
- `ui_pass`: UiPass (if gui feature)
- `post_process_pass`: PostProcessPass
- `extra_passes`: Vec<Box<dyn RenderPass>>
- `camera_system`: CameraSystem
- `viewport`: Viewport
- `width`, `height`: u32
- `mode`: RendererMode
- `render_stats`: RenderStats
- `prepass`: PrePass
- `ssao_pass`: SsaoPass
- `ssao_blur_pass`: SsaoBlurPass
- `ssao_resources`: SsaoResources
- `ssao_enabled`: bool
- `render_style`: RenderStyle
- `cel_pass`: Option<CelShadedPass>
- `outline_pass`: Option<OutlinePass>
- `flat_pass`: Option<FlatShadedPass>
- `pipeline_layouts`: PipelineLayouts
- `current_dir_light`: DirectionalLightUniform
- `gpu_culling_enabled`: bool (if gpu-driven)
- `cull_pass`: Option<CullPass> (if gpu-driven)

### 2. `renderer_api.rs` - Public API Surface
**Purpose**: Public API methods that should remain in lib.rs or be re-exported

**Functions to extract**:
- `Renderer::camera()` (lines 253-255) - Camera accessor
- `Renderer::camera_mut()` (lines 259-261) - Mutable camera accessor  
- `Renderer::orbit_mut()` (lines 265-267) - Orbit controller accessor
- `Renderer::register_texture()` (lines 508-526) - Texture registration
- `Renderer::register_texture_linear()` (lines 532-550) - Linear texture registration
- `Renderer::free_texture()` (lines 557-559) - Texture deallocation
- `Renderer::create_material()` (lines 563-576) - Material creation
- `Renderer::register_mesh()` (lines 584-586) - Mesh registration (if assets feature)
- `Renderer::free_mesh()` (lines 592-594) - Mesh deallocation (if assets feature)
- `Renderer::free_material()` (lines 601-608) - Material deallocation
- `Renderer::update_material_params()` (lines 614-617) - Material parameter update
- `Renderer::update_texture_data()` (lines 623-637) - Texture data update
- `Renderer::set_directional_light()` (lines 644-656) - Directional light setup
- `Renderer::set_render_style()` (lines 666-729) - Render style switching
- `Renderer::set_point_lights()` (lines 735-738) - Point light upload
- `Renderer::set_mode()` (lines 747-761) - Renderer mode switching
- `Renderer::render_to_target()` (lines 764-770) - Render to target (if gui)
- `Renderer::render_to_view()` (lines 774-781) - Render to view (if gui)
- `Renderer::enable_gpu_culling()` (lines 795-803) - GPU culling enable (if gpu-driven)
- `Renderer::cull_visible_counts()` (lines 812-818) - Cull statistics (if gpu-driven)
- `Renderer::set_scene()` (lines 830-855) - Scene data upload
- `Renderer::sync_world()` (lines 861-1083) - ECS world synchronization
- `Renderer::queue_gizmo()` (lines 1094-1100) - Gizmo queuing
- `Renderer::add_pass()` (lines 1104-1112) - Custom pass addition
- `Renderer::clear_extra_passes()` (lines 1115-1117) - Clear custom passes
- `Renderer::resize()` (lines 1123-1183) - Window resize handling
- `Renderer::set_viewport()` (lines 1186-1190) - Viewport configuration
- `Renderer::set_clear_color()` (lines 1196-1198) - Background color
- `Renderer::set_font_atlas()` (lines 1202-1207) - Font atlas upload (if gui)
- `Renderer::handle_input()` (lines 1212-1214) - Input handling

### 3. `renderer_resource.rs` - Resource Management
**Purpose**: Resource lifecycle, texture/material management, instance buffers

**Functions to extract**:
- Texture registration/management methods
- Material creation/management methods  
- Mesh registration/management methods
- Instance buffer management
- Material table synchronization

**Field dependencies**:
- `material_registry`: MaterialRegistry
- `world_material_descs`: HashMap<u64, MaterialDescriptor>
- `instance_buf`: InstanceBuffer
- `shadow_instance_buf`: InstanceBuffer
- `instance_layout`: Arc<wgpu::BindGroupLayout>
- `world_pass`: WorldPass (for material table updates)

### 4. `renderer_passes.rs` - Pass Management
**Purpose**: Render pass coordination and execution pipeline

**Functions to extract**:
- Pass execution pipeline (do_render)
- SSAO pass coordination
- Render style pass coordination
- Gizmo pass execution
- Post-process pass execution
- UI pass execution
- Extra passes execution

**Field dependencies**:
- `world_pass`: WorldPass
- `ui_pass`: UiPass (if gui)
- `post_process_pass`: PostProcessPass
- `extra_passes`: Vec<Box<dyn RenderPass>>
- `prepass`: PrePass
- `ssao_pass`: SsaoPass
- `ssao_blur_pass`: SsaoBlurPass
- `cel_pass`: Option<CelShadedPass>
- `outline_pass`: Option<OutlinePass>
- `flat_pass`: Option<FlatShadedPass>
- `gizmo_system`: GizmoSystem
- `frame_builder`: FrameBuilder

## Conditional Compilation Features

### `gui` feature:
- `ui_pass`: UiPass field
- `render_to_target()`, `render_to_view()` methods
- `set_font_atlas()` method
- GUI-related code in `do_render()`

### `gpu-driven` feature:
- `gpu_culling_enabled`: bool field
- `cull_pass`: Option<CullPass> field
- `enable_gpu_culling()` method
- `cull_visible_counts()` method
- GPU culling logic in `sync_world()` and `do_render()`

### `assets` feature:
- `register_mesh()` method
- `free_mesh()` method

## Public API Surface to Remain in lib.rs

The following should remain as re-exports or public API in lib.rs:
1. `RendererMode` enum
2. `RenderDest` enum (if needed)
3. All public re-exports from sub-modules
4. Feature-gated conditional exports

## Recommended Refactoring Steps

1. **Create `renderer_core.rs`** with core initialization and render pipeline
2. **Create `renderer_api.rs`** with public API methods
3. **Create `renderer_resource.rs`** with resource management
4. **Create `renderer_passes.rs`** with pass coordination
5. **Update lib.rs** to:
   - Import and re-export from sub-modules
   - Maintain backward compatibility
   - Keep essential public API surface

## Implementation Notes

- The `do_render()` method is complex and should be carefully split between `renderer_core.rs` (main pipeline) and `renderer_passes.rs` (pass-specific logic)
- SSAO coordination logic should stay with pass management
- Resource synchronization (material tables, instance buffers) should be in resource management
- Conditional compilation should be preserved in appropriate sub-modules