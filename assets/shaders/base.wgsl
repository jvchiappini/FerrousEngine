// Shader básico que recibe un buffer de vértices con posición y color,
// además de un uniforme de cámara que proporciona la matriz view-proj.

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) color : vec3<f32>,
};

struct Camera {
    view_proj : mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera : Camera;

struct VsIn {
    @location(0) position : vec3<f32>,
    @location(1) color : vec3<f32>,
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
    return vec4<f32>(in.color, 1.0);
}
