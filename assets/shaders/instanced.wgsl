// Shader de instanced rendering.
//
// Todas las matrices de modelo viven en un array<mat4x4<f32>> en un
// storage buffer.  El índice de la instancia activa se obtiene de
// @builtin(instance_index), que wgpu proporciona gratuitamente cuando
// se emite draw_indexed(..., first_instance..first_instance+count).

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) color : vec3<f32>,
};

struct Camera {
    view_proj : mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera : Camera;

// Array de matrices de modelo — una por instancia.
@group(1) @binding(0)
var<storage, read> instances : array<mat4x4<f32>>;

struct VsIn {
    @location(0) position : vec3<f32>,
    @location(1) color    : vec3<f32>,
};

@vertex
fn vs_main(
    in            : VsIn,
    @builtin(instance_index) inst_idx : u32,
) -> VsOut {
    var out : VsOut;
    // inst_idx is the absolute instance index (includes first_instance offset
    // from draw_indexed).  It indexes directly into the instances storage buffer
    // where matrices are packed contiguously starting at slot `first_instance`.
    let model     = instances[inst_idx];
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_pos  = camera.view_proj * world_pos;
    out.color     = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
