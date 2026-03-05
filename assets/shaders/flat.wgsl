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
    @location(2) uv       : vec2<f32>,
    @location(3) tangent  : vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0)       world_pos : vec3<f32>,
    @location(1)       uv        : vec2<f32>,
};

// ── Vertex shader ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(
    vert: VertexInput,
    @builtin(instance_index) idx: u32,
) -> VertexOutput {
    let model     = instances[idx];
    let world_pos = model * vec4<f32>(vert.position, 1.0);

    var out: VertexOutput;
    out.clip_pos  = camera.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    out.uv        = vert.uv;
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Derive a face-flat normal from screen-space derivatives of world position.
    // `dpdx` / `dpdy` give the rate of change of `world_pos` across a 2×2 pixel
    // quad; the cross product of these two tangents is the face normal.
    let dx = dpdx(in.world_pos);
    let dy = dpdy(in.world_pos);
    let face_normal = normalize(cross(dx, dy));

    // sample base colour
    let albedo_tex = textureSample(tex_albedo, mat_sampler, in.uv);
    let base = material.base_color * albedo_tex;

    // alpha discard (FLAG_ALPHA_MASK = 1)
    if (material.flags & 1u) != 0u {
        if base.a < material.alpha_cutoff { discard; }
    }

    // Lambertian diffuse
    let l       = normalize(-dir_light.direction);
    let n_dot_l = max(dot(face_normal, l), 0.0);
    let ambient = dir_light.color * 0.15;
    let diffuse = dir_light.color * dir_light.intensity * n_dot_l;

    let lit = base.rgb * (ambient + diffuse);

    // emissive additive
    let emissive = material.emissive.rgb * material.emissive.w;

    return vec4<f32>(lit + emissive, base.a);
}
