<!--
Step-by-step guide to adding a compute shader pass to ferrous_renderer.
Covers ComputePipeline construction, WGSL shader authoring, ComputePass
registration, and ping-pong buffer patterns.
-->

# Compute pipeline — step-by-step guide

This guide walks you through wiring a compute shader into the render
graph using `ComputePipeline` and `ComputePass`.  The example implements
a **particle simulation** that updates `N` particle positions on the GPU
every frame before the world pass reads them.

## Prerequisites

- Read `../architecture.md` to understand the prepare/execute split and
  why compute passes run on the same `CommandEncoder` as raster passes.
- Read `../render_pass.md` for the `RenderPass` trait contract and
  ordering rules.

## When to use a compute pass

| Use-case | Notes |
|----------|-------|
| Particle simulation | Update positions/velocities each frame in a storage buffer |
| Raymarching / SDF | Write pixels directly to a `storage` texture |
| Voxel data generation | Build a density field, hand buffer to a mesh extraction pass |
| Post-process effects | Read the resolved colour texture, write to a second texture |

## How `ComputePipeline` and `ComputePass` relate

```
ComputePipeline  ─── wgpu::ComputePipeline (Arc-wrapped, cheap to clone)
        │
        └─► ComputePass (implements RenderPass)
                │   name: String
                │   workgroup_count: (u32, u32, u32)
                │   bind_groups: Vec<wgpu::BindGroup>
                │
                └─► RenderGraph::passes  (ordered alongside raster passes)
```

`ComputePipeline::new` compiles the WGSL shader and builds the wgpu
pipeline layout from the bind-group layouts you supply.  `ComputePass`
holds the pipeline, the bind groups, and the dispatch dimensions, and
implements the `prepare` / `execute` contract.

## Step 1 — Write the WGSL compute shader

Create `assets/shaders/particles.wgsl`:

```wgsl
// Particle update compute shader.
// Each invocation handles one particle.

struct Particle {
    position: vec3<f32>,
    _pad0:    f32,
    velocity: vec3<f32>,
    _pad1:    f32,
};

struct Params {
    delta_time: f32,
    count:      u32,
};

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<uniform>             params:    Params;

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= params.count { return; }

    // Simple Euler integration
    particles[i].position += particles[i].velocity * params.delta_time;

    // Wrap around a [-10, 10] box
    particles[i].position = fract(
        (particles[i].position + vec3<f32>(10.0)) / 20.0
    ) * 20.0 - vec3<f32>(10.0);
}
```

**Workgroup size:** `@workgroup_size(64, 1, 1)` means each workgroup
processes 64 particles.  For `N` particles dispatch
`ceil(N / 64)` workgroups in X.

## Step 2 — Create the bind group layout and pipeline

```rust
use ferrous_renderer::pipeline::ComputePipeline;

fn build_particle_pipeline(
    device: &wgpu::Device,
) -> (ComputePipeline, wgpu::BindGroupLayout) {
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("particle_bgl"),
        entries: &[
            // binding 0 — particle storage buffer
            wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            },
            // binding 1 — params uniform
            wgpu::BindGroupLayoutEntry {
                binding:    1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            },
        ],
    });

    let shader_src = std::fs::read_to_string("assets/shaders/particles.wgsl")
        .expect("particles.wgsl not found");

    let pipeline = ComputePipeline::new(
        device,
        &shader_src,
        &[&bgl],
        "main",
        Some("particle_pipeline"),
    );

    (pipeline, bgl)
}
```

## Step 3 — Allocate GPU buffers and build the bind group

```rust
use wgpu::util::DeviceExt;

const N: u32 = 1_000_000;

// Storage buffer — one Particle per entry (32 bytes each)
let particle_buf = device.create_buffer(&wgpu::BufferDescriptor {
    label:              Some("particle_buf"),
    size:               (N as u64) * 32,
    usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
});

// Params uniform — delta_time (f32) + count (u32) + 8 bytes padding = 16 bytes
let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label:    Some("particle_params"),
    contents: bytemuck::cast_slice(&[0.0_f32, N as f32, 0.0_f32, 0.0_f32]),
    usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
});

let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    label:  Some("particle_bg"),
    layout: &bgl,
    entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: particle_buf.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 1, resource: params_buf.as_entire_binding()   },
    ],
});
```

## Step 4 — Construct `ComputePass` and register it

```rust
use ferrous_renderer::passes::ComputePass;

let workgroups_x = (N + 63) / 64;   // ceil(N / workgroup_size)

let compute_pass = ComputePass::new(
    "particle_update",
    pipeline,
    (workgroups_x, 1, 1),
    vec![bind_group],
);

// Register BEFORE WorldPass so the updated positions are ready
// when the raster pass reads the vertex buffer.
renderer.clear_passes();
renderer.add_pass(Box::new(compute_pass));
renderer.add_pass(Box::new(WorldPass::new(/* … */)));
renderer.add_pass(Box::new(UiPass::new(/* … */)));
```

## Step 5 — Update params each frame

In your `FerrousApp::update` (or `draw_3d`) write the new `delta_time`
to the params buffer before the frame is submitted:

```rust
fn update(&mut self, ctx: &mut AppContext) {
    let dt = ctx.time.delta;
    let params: [f32; 4] = [dt, N as f32, 0.0, 0.0];
    ctx.renderer().unwrap()
        .context.queue
        .write_buffer(&self.params_buf, 0, bytemuck::cast_slice(&params));
}
```

## Dynamic bind group swapping (ping-pong)

For algorithms that alternate between two buffers each frame, call
`ComputePass::set_bind_groups` inside `prepare` by implementing a
wrapper pass:

```rust
pub struct PingPongPass {
    inner:   ComputePass,
    bg_a:    wgpu::BindGroup,
    bg_b:    wgpu::BindGroup,
    frame:   u64,
}

impl RenderPass for PingPongPass {
    fn name(&self) -> &str { self.inner.name() }

    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) {
        // Swap which buffer is "read" and which is "write"
        let bg = if self.frame % 2 == 0 { &self.bg_a } else { &self.bg_b };
        self.inner.set_bind_groups(vec![bg.clone() /* … */]);
        self.frame += 1;
    }

    fn execute(&mut self, device, queue, encoder, color_view, resolve, depth, packet) {
        self.inner.execute(device, queue, encoder, color_view, resolve, depth, packet);
    }

    fn as_any(&self)         -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
```

## Ordering cheatsheet

| Goal | Registration order |
|------|--------------------|
| Compute writes buffer read by world pass | Compute → World → UI |
| Post-process reads resolved colour texture | World → UI → Compute |
| Two compute passes with a data dependency | Compute A → Compute B → World → UI |

The `CommandEncoder` is shared across all passes, so barriers between a
compute write and a subsequent raster read are handled automatically by
`wgpu`'s resource tracking.
