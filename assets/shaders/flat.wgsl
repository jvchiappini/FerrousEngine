// Flat-shaded instanced geometry.
//
// Like the PBR shader but replaces smooth per-vertex normals with face normals
// derived from `dpdx` / `dpdy` of the world-space position.  The result is the
// low-poly / faceted look where every triangle has a single uniform shade.
//
// Lighting: simple Lambertian diffuse from a single directional light.
// No IBL, no PBR specular.  The ambient term is a constant fraction of the
// light colour to prevent fully-dark faces.
//
// Bind groups mirror instanced.wgsl:
//   group(0) camera
//   group(1) instance storage buffer
//   group(2) material uniform + albedo texture
//   group(3) directional light uniform

const PI: f32 = 3.14159265359;

// ── Bind groups ───────────────────────────────────────────────────────────────

struct Camera {
    view      : mat4x4<f32>,
    proj      : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    eye_pos   : vec3<f32>,
    exposure  : f32,
    fog_color : vec3<f32>,
    fog_density: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
    _padding: array<vec4<f32>, 17>,
};
@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<storage, read> instances: array<mat4x4<f32>>;

struct MaterialUniform {
    base_color          : vec4<f32>,
    emissive            : vec4<f32>,   // w = strength
    metallic_roughness  : vec4<f32>,
    normal_ao           : vec4<f32>,
    flags               : u32,
    alpha_cutoff        : f32,
    _pad                : vec2<u32>,
};
@group(2) @binding(0)
var<uniform> material: MaterialUniform;
@group(2) @binding(1)
var mat_sampler: sampler;
@group(2) @binding(2)
var tex_albedo: texture_2d<f32>;

struct DirectionalLight {
    direction       : vec3<f32>,
    _pad0           : f32,
    color           : vec3<f32>,
    intensity       : f32,
    light_view_proj : mat4x4<f32>,
};
@group(3) @binding(0)
var<uniform> dir_light: DirectionalLight;

// ── Vertex / Fragment IO ──────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) tangent  : vec4<f32>,
    @location(3) color    : vec4<f32>,
    @location(4) uv       : vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0)       world_pos : vec3<f32>,
    @location(1)       uv        : vec2<f32>,
    @location(2)       color     : vec4<f32>,
};

// ── Vertex shader ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(
    vert: VertexInput,
    @builtin(instance_index) idx: u32,
) -> VertexOutput {
    var model = instances[idx];
    let is_screen_space = abs(model[0][3]) > 0.5;
    if (is_screen_space) {
        model[0][3] = 0.0;
    }
    
    let world_pos = model * vec4<f32>(vert.position, 1.0);

    var out: VertexOutput;
    if (is_screen_space) {
        out.clip_pos = world_pos;
    } else {
        out.clip_pos  = camera.view_proj * world_pos;
    }
    out.world_pos = world_pos.xyz;
    out.uv        = vert.uv;
    out.color     = vert.color;
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Standard albedo + emissive for unlit look
    let base = material.base_color * in.color;
    let emissive = material.emissive.rgb * material.emissive.w;
    
    // Final output (ignoring lighting and derivatives to ensure 2D consistency)
    return vec4<f32>(base.rgb + emissive, base.a);
}
