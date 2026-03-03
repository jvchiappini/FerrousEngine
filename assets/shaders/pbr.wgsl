// Physically based rendering shader in WGSL
// Phase 5: full PBR from scratch

// constants
const PI: f32 = 3.14159265359;

// bind groups and structs

struct Camera {
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,   // xyz = world-space camera position, w = padding
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

    // glTF stores UVs with V=0 at the bottom (OpenGL convention).
    // wgpu/Vulkan expect V=0 at the top, so we flip the V axis here.
    out.uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
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

// Geometry term for direct lighting: k = (roughness+1)² / 8
fn geometry_schlick_ggx_direct(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k + 0.0001);
}

// Geometry term for IBL/ambient: k = roughness² / 2
fn geometry_schlick_ggx_ibl(NdotV: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let k = a / 2.0;
    return NdotV / (NdotV * (1.0 - k) + k + 0.0001);
}

// Smith geometry for direct lighting
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let ggx1 = geometry_schlick_ggx_direct(NdotV, roughness);
    let ggx2 = geometry_schlick_ggx_direct(NdotL, roughness);
    return ggx1 * ggx2;
}

// Smith geometry for ambient/IBL
fn geometry_smith_ibl(NdotV: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx_ibl(NdotV, roughness);
}

fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
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
    var ao_factor = material.metallic_roughness.z;  // ao_strength
    if ((material.flags & 16u) != 0u) {
        ao_factor = textureSample(tex_ao, mat_sampler, in.uv).x * ao_factor;
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
    metallic  = clamp(metallic,  0.0,  1.0);
    // 0.001 minimum: avoids GGX singularity while still allowing near-mirror surfaces.
    // For a perfect mirror set metallic=1, roughness=0 in the inspector.
    roughness = clamp(roughness, 0.001, 1.0);

    // View vector: world-space direction from fragment to camera eye.
    let Vdir = normalize(camera.eye_pos.xyz - in.world_pos);

    let Ldir = normalize(-dir_light.direction);
    let H    = normalize(Vdir + Ldir);
    // clamp NdotV to a small epsilon to avoid divide-by-zero in geometry term
    let NdotV = max(dot(N, Vdir), 0.0001);
    let NdotL = max(dot(N, Ldir), 0.0);
    let NdotH = max(dot(N, H),    0.0);
    let VdotH = max(dot(Vdir, H), 0.0);

    // F0: dielectrics use 0.04, metals use albedo
    let F0 = mix(vec3<f32>(0.04), albedo, metallic);

    // Cook-Torrance specular BRDF
    let D       = distribution_ggx(NdotH, roughness);
    let G       = geometry_smith(NdotV, NdotL, roughness);
    let F       = fresnel_schlick(VdotH, F0);
    let specular = (D * G * F) / (4.0 * NdotV * NdotL + 0.0001);

    // energy conservation: kD = 0 for metals (all energy goes to specular)
    let kD = (vec3<f32>(1.0) - F) * (1.0 - metallic);

    let radiance = dir_light.color * dir_light.intensity;
    let Lo = (kD * albedo / PI + specular) * radiance * NdotL;

    // Ambient: Fake Hemisphere / IBL Environment
    let F_ambient = fresnel_schlick(max(dot(N, Vdir), 0.0), F0);
    let kD_ambient = (vec3<f32>(1.0) - F_ambient) * (1.0 - metallic);
    
    let sky_color = vec3<f32>(0.5, 0.7, 1.0) * 1.2;
    let ground_color = vec3<f32>(0.2, 0.15, 0.1);
    
    // Diffuse ambient (Irradiance) based on surface normal
    let irradiance = mix(ground_color, sky_color, N.y * 0.5 + 0.5);
    let diffuse_ambient = kD_ambient * albedo * irradiance;
    
    // Specular ambient (Radiance) based on reflection vector
    let R = reflect(-Vdir, N);
    let R_blend = mix(N.y, R.y, 1.0 - roughness); // difuminar hacia la normal si es rugoso
    let specular_radiance = mix(ground_color, sky_color, R_blend * 0.5 + 0.5);
    
    let G_ambient = geometry_smith_ibl(max(dot(N, Vdir), 0.0001), roughness);
    let specular_ambient = F_ambient * G_ambient * specular_radiance;

    let ambient = (diffuse_ambient + specular_ambient) * ao_factor * 0.8; // intensidad global del ambiente

    var color = ambient + Lo;

    // emissive
    if ((material.flags & 8u) != 0u) {
        let emiss = material.emissive.xyz * material.emissive.w;
        let sample = textureSample(tex_emissive, mat_sampler, in.uv);
        color += emiss * sample.xyz;
    }

    // tone mapping 
    color = color / (color + vec3<f32>(1.0));
    // The surface format is sRGB, so the hardware handles gamma correction.

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
