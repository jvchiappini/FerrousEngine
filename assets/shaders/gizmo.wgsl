// Minimal shader for gizmo lines.  Identical to `base.wgsl` except there
// is no `model` uniform; all positions are expected in world space already.

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) color : vec4<f32>,
};

struct Camera {
    view      : mat4x4<f32>,
    proj      : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    eye_pos   : vec3<f32>,
    exposure  : f32,
    fog_color : vec4<f32>,
    fog_density: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
    _padding: array<vec4<f32>, 17>,
};

@group(0) @binding(0)
var<uniform> camera : Camera;

struct VsIn {
    @location(0) position : vec3<f32>,
    @location(3) color : vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out : VsOut;
    out.clip_pos = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}