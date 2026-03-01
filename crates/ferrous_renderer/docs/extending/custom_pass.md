<!--
Step-by-step guide to implementing a custom RenderPass.
Covers pipeline construction, shader authoring, and registration.
-->

# Custom render pass — step-by-step guide

This guide walks you through creating a fully functional custom
`RenderPass` from scratch.  The example adds a **toon-outline pass**
that draws a slightly expanded, back-face-culled solid in a single
colour to produce a cartoon outline effect.

## Prerequisites

- Read `../render_pass.md` for the trait contract and ordering rules.
- Read `../architecture.md` to understand `FramePacket` and the
  prepare/execute split.

## Step 1 — Write the WGSL shader

Create `assets/shaders/outline.wgsl`:

```wgsl
// Outline pass — expands vertices along their normals and renders
// back faces in a flat colour.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
struct ModelUniform {
    model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model:  ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,   // unused; keeps layout compatible
};

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    let world_pos = model.model * vec4<f32>(in.position, 1.0);
    let expanded  = world_pos.xyz + normalize(world_pos.xyz) * 0.03;
    return camera.view_proj * vec4<f32>(expanded, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);   // black outline
}
```

The shader reuses the same bind-group layout as the world shader
(group 0 = camera, group 1 = model), so `PipelineLayouts` can be
shared directly.

## Step 2 — Build the pipeline

```rust
use std::sync::Arc;
use ferrous_renderer::{
    pipeline::layout::PipelineLayouts,
    geometry::Vertex,
};

pub fn build_outline_pipeline(
    device:  &wgpu::Device,
    layouts: &PipelineLayouts,
    format:  wgpu::TextureFormat,
    samples: u32,
) -> wgpu::RenderPipeline {
    let src = std::fs::read_to_string("assets/shaders/outline.wgsl")
        .expect("outline.wgsl not found");

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:  Some("outline_shader"),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some("outline_layout"),
        bind_group_layouts:   &[&layouts.camera, &layouts.model],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:  Some("outline_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module:      &module,
            entry_point: Some("vs_main"),
            buffers:     &[Vertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module:      &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology:   wgpu::PrimitiveTopology::TriangleList,
            cull_mode:  Some(wgpu::Face::Front), // cull front faces = draw back
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format:              wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare:       wgpu::CompareFunction::Less,
            stencil:             Default::default(),
            bias:                Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview: None,
        cache:     None,
    })
}
```

## Step 3 — Implement the pass

```rust
use ferrous_renderer::{RenderPass, FramePacket};
use std::any::Any;
use std::sync::Arc;

pub struct OutlinePass {
    pipeline: Arc<wgpu::RenderPipeline>,
}

impl OutlinePass {
    pub fn new(
        device:  &wgpu::Device,
        layouts: &ferrous_renderer::pipeline::layout::PipelineLayouts,
        format:  wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        Self {
            pipeline: Arc::new(build_outline_pipeline(device, layouts, format, samples)),
        }
    }
}

impl RenderPass for OutlinePass {
    fn name(&self) -> &str { "outline_pass" }

    fn as_any(&self)         -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn prepare(
        &mut self,
        _device: &wgpu::Device,
        _queue:  &wgpu::Queue,
        _packet: &FramePacket,
    ) {
        // nothing to upload: camera and model uniforms are managed
        // by WorldPass and sync_world respectively
    }

    fn execute(
        &self,
        _device:        &wgpu::Device,
        _queue:         &wgpu::Queue,
        encoder:        &mut wgpu::CommandEncoder,
        color_view:     &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        depth_view:     &wgpu::TextureView,
        packet:         &FramePacket,
    ) {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("outline_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view:           color_view,
                resolve_target,
                ops: wgpu::Operations {
                    load:  wgpu::LoadOp::Load,   // preserve WorldPass output
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load:  wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &packet.camera.bind_group, &[]);

        for cmd in &packet.scene_objects {
            rp.set_bind_group(1, &cmd.model_bind_group, &[]);
            rp.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
            rp.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
            rp.draw_indexed(0..cmd.index_count, 0, 0..1);
        }
    }
}
```

## Step 4 — Register the pass

Retrieve the format and sample count from the renderer, then register
the pass **between** `WorldPass` and `UiPass`:

```rust
// 1. Clear the default pass list
renderer.clear_passes();

// 2. Re-add WorldPass manually (it was removed by clear_passes)
let world_pass = ferrous_renderer::passes::world_pass::WorldPass::new(
    &ctx.device,
    &pipeline_layouts,
    &gpu_camera,
    wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
);
renderer.add_pass(Box::new(world_pass));

// 3. Add the outline pass
let outline = OutlinePass::new(
    &ctx.device,
    &pipeline_layouts,
    renderer.render_target().color_format(),
    renderer.render_target().sample_count(),
);
renderer.add_pass(Box::new(outline));

// 4. Re-add UiPass
let ui_pass = ferrous_renderer::passes::ui_pass::UiPass::new(&ctx.device, &ctx.queue);
renderer.add_pass(Box::new(ui_pass));
```

> **Tip** — if you only want to append a pass *after* the existing list
> (e.g. a post-process effect after the UI), simply call
> `renderer.add_pass(Box::new(my_pass))` without calling `clear_passes`.

## Step 5 — Verify

Run `cargo check -p ferrous_editor` (or your application crate) to
confirm there are no compile errors, then run the application to see the
outline effect.

## Common mistakes

| Mistake | Symptom | Fix |
|---------|---------|-----|
| Using `LoadOp::Clear` in a non-first pass | Overwrites WorldPass output | Use `LoadOp::Load` |
| Forgetting `resolve_target` in MSAA | Black screen or validation error | Always forward `resolve_target` |
| Accessing `queue` inside `execute` | Panic / borrow conflict | Move all uploads to `prepare` |
| Wrong cull mode for outline effect | No outline visible | Front faces must be culled |
