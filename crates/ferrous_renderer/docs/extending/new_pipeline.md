<!--
Guide to adding a new wgpu render pipeline to ferrous_renderer.
Covers pipeline layout conventions, shader requirements, and where to
place the new code in the module tree.
-->

# Adding a new render pipeline

This guide explains how to add a second (or third) render pipeline
alongside the built-in `WorldPipeline`.  Common reasons to add a new
pipeline are:

- Different shading models (PBR, unlit, wireframe, …)
- Fullscreen post-process effects
- Specialised geometry formats (skinned meshes, instanced quads, …)

## Conventions you must follow

### Bind-group layout slots

The crate reserves two global slots for all 3-D pipelines:

| Group | Contents |
|-------|----------|
| 0 | Camera uniform (`CameraUniform { view_proj: mat4x4<f32> }`) |
| 1 | Model uniform (`mat4x4<f32>` transform) |

If your pipeline also uses these groups, share the `Arc<BindGroupLayout>`
from `PipelineLayouts` so bind-groups created for one pipeline can be
used with another.  If your pipeline needs additional resources (textures,
extra uniforms), add them at group 2 and above.

### Vertex format

Use `Vertex::layout()` unless you are intentionally introducing a new
vertex format.  If you need a different layout, document it clearly and
create a new layout descriptor in `geometry/vertex.rs` or alongside
your pipeline file.

### Texture format and sample count

Always query these from `renderer.render_target()`:

```rust
let format  = renderer.render_target().color_format();
let samples = renderer.render_target().sample_count();
```

Hardcoding these values will cause validation errors when MSAA is
changed or when rendering to textures with a different format.

## File placement

Create a new file in `pipeline/`:

```
pipeline/
├── mod.rs
├── layout.rs      // shared PipelineLayouts — do not modify
├── world.rs       // existing WorldPipeline
└── my_pipeline.rs // your new pipeline
```

Declare it in `pipeline/mod.rs`:

```rust
pub mod my_pipeline;
pub use my_pipeline::MyPipeline;
```

## Minimal pipeline implementation

```rust
// pipeline/my_pipeline.rs
use std::sync::Arc;
use crate::geometry::Vertex;
use super::layout::PipelineLayouts;

#[derive(Clone)]
pub struct MyPipeline {
    pub inner:   Arc<wgpu::RenderPipeline>,
    pub layouts: PipelineLayouts,
}

impl MyPipeline {
    pub fn new(
        device:  &wgpu::Device,
        layouts: PipelineLayouts,
        format:  wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let src = std::fs::read_to_string("assets/shaders/my_shader.wgsl")
            .expect("my_shader.wgsl not found");

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("my_shader"),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("my_pipeline_layout"),
            bind_group_layouts:   &[&layouts.camera, &layouts.model],
            push_constant_ranges: &[],
        });

        let inner = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("my_pipeline"),
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
                    blend:      Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology:  wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
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
        });

        Self { inner: Arc::new(inner), layouts }
    }
}
```

## Using the new pipeline in a pass

Construct your pipeline in `Renderer::new` (or lazily on first use) and
pass it to a custom `RenderPass`:

```rust
// In Renderer::new or your application setup:
let my_pipeline = MyPipeline::new(
    &ctx.device,
    pipeline_layouts.clone(),
    render_target.color_format(),
    render_target.sample_count(),
);
renderer.add_pass(Box::new(MyPass::new(my_pipeline)));
```

Inside `MyPass::execute` bind group 0 from `packet.camera.bind_group`
and group 1 from each `DrawCommand::model_bind_group`, then call
`set_pipeline(&my_pipeline.inner)` before issuing draw calls.

## Shader WGSL template

Place the shader at `assets/shaders/my_shader.wgsl`:

```wgsl
struct CameraUniform { view_proj: mat4x4<f32> };
struct ModelUniform  { model:     mat4x4<f32> };

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model:  ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       color:    vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = model.model * vec4<f32>(in.position, 1.0);
    out.clip_pos = camera.view_proj * world;
    out.color    = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
```

## Checklist

- [ ] File created under `pipeline/my_pipeline.rs`
- [ ] Declared and re-exported in `pipeline/mod.rs`
- [ ] Queries `format` and `samples` from `RenderTarget` at construction time
- [ ] Uses `PipelineLayouts` slots (group 0 = camera, group 1 = model)
- [ ] Shader located under `assets/shaders/`
- [ ] Associated `RenderPass` impl registered with `Renderer::add_pass`
- [ ] `cargo check --workspace` passes
