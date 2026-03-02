// Shader básico que recibe un buffer de vértices con posición y color,
// además de un uniforme de cámara que proporciona la matriz view-proj.

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) color : vec3<f32>,
    @location(1) uv : vec2<f32>,
};

struct Camera {
    view_proj : mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera : Camera;
// per-object model matrix (dynamic uniform in pipeline)
struct Model {
    model : mat4x4<f32>,
};
@group(1) @binding(0)
var<uniform> model : Model;
// ---- material definitions moved to global scope ------------------------
struct Material {
    base_color : vec4<f32>,
    use_texture : u32,
};

@group(2) @binding(0)
var<uniform> material : Material;
@group(2) @binding(1)
var texture_sampler : sampler;
@group(2) @binding(2)
var texture : texture_2d<f32>;

// vertex input layout
struct VsIn {
    @location(0) position : vec3<f32>,
    @location(1) color : vec3<f32>,
    @location(2) uv : vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out : VsOut;
    // apply model transform before camera
    let world_pos = model.model * vec4<f32>(in.position, 1.0);
    out.clip_pos = camera.view_proj * world_pos;
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // material uniforms and bindings are declared at global scope above

    var color : vec4<f32> = material.base_color * vec4<f32>(in.color, 1.0);
    if (material.use_texture == 1u) {
        let texel = textureSample(texture, texture_sampler, in.uv);
        color = color * texel;
    }
    return color;
}
