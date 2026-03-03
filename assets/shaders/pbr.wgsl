// Physically based rendering shader in WGSL
// Phase 5: full PBR from scratch

// constants
const PI: f32 = 3.14159265359;

// bind groups and structs

struct Camera {
    view_proj : mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct Model {
    model : mat4x4<f32>,
    normal_mat : mat4x4<f32>,
};

@group(1) @binding(0)
var<uniform> model: Model;

struct MaterialUniform {
    base_color : vec4<f32>,
    emissive : vec4<f32>, // w == strength
    metallic_roughness : vec4<f32>, // x=met, y=rough, z=ao_strength
    normal_ao : vec4<f32>, // x = normal_scale
    flags : u32,
    alpha_cutoff: f32,
    // pad the remaining three dwords to maintain 80‑byte total size
    _pad: vec2<u32>,
};

@group(2) @binding(0)
var<uniform> material: MaterialUniform;

@group(2) @binding(1)
var mat_sampler: sampler;

@group(2) @binding(2)
var tex_albedo: texture_2d<f32>;

@group(2) @binding(3)
var tex_normal: texture_2d<f32>;

@group(2) @binding(4)
var tex_met_rough: texture_2d<f32>;

@group(2) @binding(5)
var tex_emissive: texture_2d<f32>;

@group(2) @binding(6)
var tex_ao: texture_2d<f32>;

struct DirectionalLight {
    direction : vec3<f32>,
    _pad0 : f32,
    color : vec3<f32>,
    intensity : f32,
};

@group(3) @binding(0)
var<uniform> dir_light: DirectionalLight;


// vertex input / output
struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(2) tangent : vec4<f32>, // w = handedness
    @location(3) color : vec3<f32>,
    @location(4) uv : vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) world_pos : vec3<f32>,
    @location(1) world_normal : vec3<f32>,
    @location(2) world_tangent : vec3<f32>,
    @location(3) world_bitangent : vec3<f32>,
    @location(4) uv : vec2<f32>,
    @location(5) color : vec3<f32>,
};

// vertex shader
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos4 = model.model * vec4<f32>(in.position, 1.0);
    out.clip_pos = camera.view_proj * world_pos4;
    out.world_pos = world_pos4.xyz;

    // transform normals and tangents
    out.world_normal = (model.normal_mat * vec4<f32>(in.normal, 0.0)).xyz;
    out.world_tangent = (model.normal_mat * vec4<f32>(in.tangent.xyz, 0.0)).xyz;
    let n = normalize(out.world_normal);
    let t = normalize(out.world_tangent);
    let b = normalize(cross(n, t) * in.tangent.w);
    out.world_bitangent = b;

    out.uv = in.uv;
    out.color = in.color;
    return out;
}

// PBR utility functions
fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = (NdotH * NdotH) * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom + 0.0001);
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = (roughness + 1.0);
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k + 0.0001);
}

fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let ggx1 = geometry_schlick_ggx(NdotV, roughness);
    let ggx2 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(1.0 - cosTheta, 5.0);
}

// fragment input / output
struct FragmentInput {
    @location(0) world_pos : vec3<f32>,
    @location(1) world_normal : vec3<f32>,
    @location(2) world_tangent : vec3<f32>,
    @location(3) world_bitangent : vec3<f32>,
    @location(4) uv : vec2<f32>,
    @location(5) color : vec3<f32>,
};

struct FragmentOutput {
    @location(0) frag_color : vec4<f32>,
};

// fragment shader
@fragment
fn fs_main(in: FragmentInput) -> FragmentOutput {
    var albedo = material.base_color.xyz * in.color;
    // track alpha separately; start with material base alpha
    var out_alpha = material.base_color.w;
    if ((material.flags & 1u) != 0u) {
        let sample = textureSample(tex_albedo, mat_sampler, in.uv);
        albedo *= sample.xyz;
        // modulate alpha by texture's alpha channel as well
        out_alpha *= sample.a;
    }
    var ao_factor = 1.0;
    if ((material.flags & 16u) != 0u) {
        ao_factor = textureSample(tex_ao, mat_sampler, in.uv).x * material.metallic_roughness.z;
    }

    // normal mapping
    var N = normalize(in.world_normal);
    if ((material.flags & 2u) != 0u) {
        var normal_sample = textureSample(tex_normal, mat_sampler, in.uv).xyz * 2.0 - vec3<f32>(1.0);
        // WGSL forbids writing to swizzles; expand manually.
        normal_sample.x = normal_sample.x * material.normal_ao.x;
        normal_sample.y = normal_sample.y * material.normal_ao.x;
        let T = normalize(in.world_tangent);
        let B = normalize(in.world_bitangent);
        let TBN = mat3x3<f32>(T, B, N);
        N = normalize(TBN * normal_sample);
    }

    // metallic / roughness
    var metallic = material.metallic_roughness.x;
    var roughness = material.metallic_roughness.y;
    if ((material.flags & 4u) != 0u) {
        let mr = textureSample(tex_met_rough, mat_sampler, in.uv).xyz;
        roughness *= mr.y;
        metallic *= mr.z;
    }
    roughness = clamp(roughness, 0.04, 1.0);

    // View vector: we don't yet have world-space camera position available
    // so approximate by treating the camera as if at origin.
    let Vdir = normalize(-in.world_pos);

    let Ldir = normalize(-dir_light.direction);
    let H = normalize(Vdir + Ldir);
    let NdotV = max(dot(N, Vdir), 0.0);
    let NdotL = max(dot(N, Ldir), 0.0);
    let NdotH = max(dot(N, H), 0.0);
    let VdotH = max(dot(Vdir, H), 0.0);

    // compute F0
    let F0 = mix(vec3<f32>(0.04), albedo, metallic);

    // Cook-Torrance BRDF
    let D = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);
    let numerator = D * G * F;
    let denominator = 4.0 * NdotV * NdotL + 0.0001;
    let specular = numerator / denominator;

    let kS = F;
    // diffuse component: (1 - F0) scaled by (1 - metallic)
    let kD = (vec3<f32>(1.0) - kS) * (1.0 - metallic);

    let radiance = dir_light.color * dir_light.intensity;
    let Lo = (kD * albedo / PI + specular) * radiance * NdotL;

    // ambient
    let ambient = vec3<f32>(0.03) * albedo * ao_factor;

    var color = ambient + Lo;

    // emissive
    if ((material.flags & 8u) != 0u) {
        let emiss = material.emissive.xyz * material.emissive.w;
        let sample = textureSample(tex_emissive, mat_sampler, in.uv);
        color += emiss * sample.xyz;
    }

    // tone mapping and gamma
    color = color / (color + vec3<f32>(1.0));
    color = pow(color, vec3<f32>(1.0 / 2.2));

    var out: FragmentOutput;
    out.frag_color = vec4<f32>(color, out_alpha);
    // alpha masking: discard below cutoff when flag set
    if ((material.flags & 32u) != 0u) {
        if (out.frag_color.a < material.alpha_cutoff) {
            discard;
        }
    }
    return out;
}
