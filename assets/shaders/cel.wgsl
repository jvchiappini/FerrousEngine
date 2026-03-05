// Cel / Toon-shaded instanced geometry.
//
// Bind groups mirror instanced.wgsl exactly so the same pipeline layouts
// can be reused:
//   group(0) camera uniform
//   group(1) instance storage buffer
//   group(2) material uniform + textures (only base_color is sampled)
//   group(3) directional light uniform  (no IBL, no shadow map)
//
// The fragment stage quantises the N·L diffuse term into `toon_levels`
// discrete bands, producing the classic flat cartoon look.

// ── Constants ─────────────────────────────────────────────────────────────────
const PI: f32 = 3.14159265359;

// ── Bind groups ───────────────────────────────────────────────────────────────

struct Camera {
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<storage, read> instances: array<mat4x4<f32>>;

struct MaterialUniform {
    base_color          : vec4<f32>,
    emissive            : vec4<f32>,   // w = strength
    metallic_roughness  : vec4<f32>,   // x=met, y=rough, z=ao_strength
    normal_ao           : vec4<f32>,   // x = normal_scale
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

// The CPU packs { toon_levels: u32, outline_width: f32, _pad: vec2<u32> }
// into a 16-byte block at binding 10.
struct CelParams {
    toon_levels   : u32,
    outline_width : f32,
    _pad0         : u32,
    _pad1         : u32,
};
@group(3) @binding(10)
var<uniform> cel_params: CelParams;

// ── Vertex / Fragment IO ──────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
    @location(3) tangent  : vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0)       world_pos : vec3<f32>,
    @location(1)       world_nrm : vec3<f32>,
    @location(2)       uv        : vec2<f32>,
};

// ── Vertex shader ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(
    vert: VertexInput,
    @builtin(instance_index) idx: u32,
) -> VertexOutput {
    let model = instances[idx];
    // normal matrix = transpose(inverse(model)); for uniform scale this is
    // just the upper-left 3x3 of the model matrix.
    let world_pos = model * vec4<f32>(vert.position, 1.0);
    let world_nrm = normalize((model * vec4<f32>(vert.normal, 0.0)).xyz);

    var out: VertexOutput;
    out.clip_pos  = camera.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    out.world_nrm = world_nrm;
    out.uv        = vert.uv;
    return out;
}

// ── Helper: quantise a [0,1] value into `levels` discrete steps ──────────────
fn quantise(value: f32, levels: u32) -> f32 {
    let f = f32(levels);
    return floor(value * f + 0.5) / f;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // sample albedo (sRGB → linear happens in the texture view)
    let albedo_tex = textureSample(tex_albedo, mat_sampler, in.uv);
    let base = material.base_color * albedo_tex;

    // alpha discard (FLAG_ALPHA_MASK = 1)
    if (material.flags & 1u) != 0u {
        if base.a < material.alpha_cutoff { discard; }
    }

    let n = normalize(in.world_nrm);
    let l = normalize(-dir_light.direction);

    // diffuse N·L, clamped
    let n_dot_l = max(dot(n, l), 0.0);

    // toon ramp quantisation
    let levels = max(cel_params.toon_levels, 2u);
    let ramp   = quantise(n_dot_l, levels);

    // ambient term: a constant 20% of the directional light colour so shaded
    // faces aren't pitch black.
    let ambient = dir_light.color * 0.2;
    let diffuse = dir_light.color * dir_light.intensity * ramp;

    let lit = base.rgb * (ambient + diffuse);

    // emissive additive
    let emissive = material.emissive.rgb * material.emissive.w;

    return vec4<f32>(lit + emissive, base.a);
}
