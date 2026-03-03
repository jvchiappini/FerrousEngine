// Instanced PBR shader.
//
// Each instance reads its model matrix from a storage buffer using
// @builtin(instance_index).  Full PBR lighting (Cook-Torrance BRDF) is
// performed in the fragment stage using the same directional light that the
// legacy per-object path uses (group 3).

const PI: f32 = 3.14159265359;

// ── bind groups ──────────────────────────────────────────────────────────────

struct Camera {
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,   // xyz = world-space camera position, w = padding
};
@group(0) @binding(0)
var<uniform> camera: Camera;

// Array of model matrices — one per instance.
@group(1) @binding(0)
var<storage, read> instances: array<mat4x4<f32>>;

struct MaterialUniform {
    base_color          : vec4<f32>,
    emissive            : vec4<f32>,  // w == strength
    metallic_roughness  : vec4<f32>,  // x=met, y=rough, z=ao_strength
    normal_ao           : vec4<f32>,  // x = normal_scale
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
    _pad0     : f32,
    color     : vec3<f32>,
    intensity : f32,
    light_view_proj : mat4x4<f32>,
};
@group(3) @binding(0)
var<uniform> dir_light: DirectionalLight;

// shadow map bindings (part of light group 3)
@group(3) @binding(6)
var shadow_sampler: sampler_comparison;
@group(3) @binding(7)
var shadow_map: texture_depth_2d;

// ── Point Lights (Storage Buffer) ────────────────────────────────────────────
struct PointLight {
    position_radius: vec4<f32>, // xyz = world pos, w = radius
    color_intensity: vec4<f32>, // xyz = linear RGB, w = intensity
};
struct LightStorage {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    lights: array<PointLight>,
};
@group(3) @binding(5) var<storage, read> point_lights: LightStorage;

// Physical inverse-square falloff with smooth cutoff at the light radius.
fn point_attenuation(dist: f32, radius: f32) -> f32 {
    let d_over_r = dist / radius;
    let numerator = saturate(1.0 - d_over_r * d_over_r * d_over_r * d_over_r);
    return (numerator * numerator) / (dist * dist + 1.0);
}

// ── vertex ────────────────────────────────────────────────────────────────────

struct VsIn {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) tangent  : vec4<f32>,
    @location(3) color    : vec3<f32>,
    @location(4) uv       : vec2<f32>,
};

struct VsOut {
    @builtin(position) clip_pos    : vec4<f32>,
    @location(0)       world_pos   : vec3<f32>,
    @location(1)       world_normal: vec3<f32>,
    @location(2)       world_tan   : vec3<f32>,
    @location(3)       world_bitan : vec3<f32>,
    @location(4)       uv          : vec2<f32>,
    @location(5)       color       : vec3<f32>,
    @location(6)       shadow_pos  : vec4<f32>,
};

@vertex
fn vs_main(in: VsIn, @builtin(instance_index) inst_idx: u32) -> VsOut {
    var out: VsOut;
    let model     = instances[inst_idx];
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_pos    = camera.view_proj * world_pos;
    out.world_pos   = world_pos.xyz;

    // Build the normal matrix as transpose(inverse(M3)) using cofactors.
    // This is correct even for non-uniform scale, unlike a plain mat3 extract.
    let m3 = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    // cofactor matrix (== adjugate transposed) — cheaper than full inverse
    let c00 = m3[1][1] * m3[2][2] - m3[2][1] * m3[1][2];
    let c01 = m3[2][1] * m3[0][2] - m3[0][1] * m3[2][2];
    let c02 = m3[0][1] * m3[1][2] - m3[1][1] * m3[0][2];
    let c10 = m3[2][0] * m3[1][2] - m3[1][0] * m3[2][2];
    let c11 = m3[0][0] * m3[2][2] - m3[2][0] * m3[0][2];
    let c12 = m3[1][0] * m3[0][2] - m3[0][0] * m3[1][2];
    let c20 = m3[1][0] * m3[2][1] - m3[2][0] * m3[1][1];
    let c21 = m3[2][0] * m3[0][1] - m3[0][0] * m3[2][1];
    let c22 = m3[0][0] * m3[1][1] - m3[1][0] * m3[0][1];
    // adjugate = transpose of cofactor matrix = normal matrix (direction preserved)
    let normal_mat = mat3x3<f32>(
        vec3<f32>(c00, c10, c20),
        vec3<f32>(c01, c11, c21),
        vec3<f32>(c02, c12, c22),
    );
    out.world_normal = normalize(normal_mat * in.normal);
    out.world_tan    = normalize(normal_mat * in.tangent.xyz);
    let n = normalize(out.world_normal);
    let t = normalize(out.world_tan);
    out.world_bitan  = normalize(cross(n, t) * in.tangent.w);

    out.uv    = in.uv;
    out.color = in.color;
    out.shadow_pos = dir_light.light_view_proj * world_pos;
    return out;
}

// ── PBR utilities ─────────────────────────────────────────────────────────────

fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a  = roughness * roughness;
    let a2 = a * a;
    let d  = (NdotH * NdotH) * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d + 0.0001);
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k + 0.0001);
}

fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(NdotV, roughness)
         * geometry_schlick_ggx(NdotL, roughness);
}

// Geometry term for IBL/ambient: k = roughness² / 2
fn geometry_schlick_ggx_ibl(NdotV: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let k = a / 2.0;
    return NdotV / (NdotV * (1.0 - k) + k + 0.0001);
}

// Smith geometry for ambient/IBL
fn geometry_smith_ibl(NdotV: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx_ibl(NdotV, roughness);
}

fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// ── fragment ──────────────────────────────────────────────────────────────────

@fragment
fn fs_main(frag_in: VsOut) -> @location(0) vec4<f32> {
    // albedo
    var albedo    = material.base_color.xyz * frag_in.color;
    var out_alpha = material.base_color.w;
    if ((material.flags & 1u) != 0u) {
        let s  = textureSample(tex_albedo, mat_sampler, frag_in.uv);
        albedo    *= s.xyz;
        out_alpha *= s.a;
    }

    // AO
    var ao_factor = material.metallic_roughness.z;  // ao_strength
    if ((material.flags & 16u) != 0u) {
        ao_factor = textureSample(tex_ao, mat_sampler, frag_in.uv).x * ao_factor;
    }

    // normal
    var N = normalize(frag_in.world_normal);
    if ((material.flags & 2u) != 0u) {
        var ns = textureSample(tex_normal, mat_sampler, frag_in.uv).xyz * 2.0 - 1.0;
        ns.x *= material.normal_ao.x;
        ns.y *= material.normal_ao.x;
        let T   = normalize(frag_in.world_tan);
        let B   = normalize(frag_in.world_bitan);
        let TBN = mat3x3<f32>(T, B, N);
        N = normalize(TBN * ns);
    }

    // metallic / roughness
    var metallic  = material.metallic_roughness.x;
    var roughness = material.metallic_roughness.y;
    if ((material.flags & 4u) != 0u) {
        let mr  = textureSample(tex_met_rough, mat_sampler, frag_in.uv).xyz;
        roughness *= mr.y;
        metallic  *= mr.z;
    }
    metallic  = clamp(metallic,  0.0,  1.0);
    // 0.001 minimum: avoids GGX singularity while still allowing near-mirror surfaces.
    // For a perfect mirror set metallic=1, roughness=0 in the inspector.
    roughness = clamp(roughness, 0.001, 1.0);

    let V    = normalize(camera.eye_pos.xyz - frag_in.world_pos);
    let Ldir = normalize(-dir_light.direction);
    let H    = normalize(V + Ldir);
    // clamp NdotV to a small epsilon to avoid divide-by-zero in geometry term
    let NdotV = max(dot(N, V),    0.0001);
    let NdotL = max(dot(N, Ldir), 0.0);
    let NdotH = max(dot(N, H),    0.0);
    let VdotH = max(dot(V, H),    0.0);

    // F0: dielectrics use 0.04, metals use albedo
    let F0      = mix(vec3<f32>(0.04), albedo, metallic);
    let D       = distribution_ggx(NdotH, roughness);
    let G       = geometry_smith(NdotV, NdotL, roughness);
    let F       = fresnel_schlick(VdotH, F0);
    let spec    = (D * G * F) / (4.0 * NdotV * NdotL + 0.0001);
    // energy conservation: kD = 0 for metals
    let kD      = (vec3<f32>(1.0) - F) * (1.0 - metallic);
    let radiance = dir_light.color * dir_light.intensity;
    var shadow: f32 = 1.0;
    let proj = frag_in.shadow_pos / frag_in.shadow_pos.w;
    // NDC: X and Y in [-1,1] with Y+ = up; UV: X in [0,1] with V+ = down.
    // Negate Y so that the shadow map is sampled right-side-up.
    let uv   = vec2<f32>(proj.x * 0.5 + 0.5, -proj.y * 0.5 + 0.5);
    if (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 && proj.z >= 0.0 && proj.z <= 1.0) {
        shadow = textureSampleCompare(shadow_map, shadow_sampler, uv, proj.z);
    }
    var Lo      = (kD * albedo / PI + spec) * radiance * NdotL * shadow;

    // ── Point lights ─────────────────────────────────────────────────────────
    let light_count = min(point_lights.count, 1024u);
    for (var i: u32 = 0u; i < light_count; i = i + 1u) {
        let pl          = point_lights.lights[i];
        let light_pos   = pl.position_radius.xyz;
        let pl_radius   = pl.position_radius.w;
        let light_color = pl.color_intensity.xyz;
        let pl_intensity = pl.color_intensity.w;

        let to_light = light_pos - frag_in.world_pos;
        let pl_dist  = length(to_light);
        if (pl_dist > pl_radius) { continue; }

        let Lpl      = to_light / pl_dist;
        let Hpl      = normalize(V + Lpl);
        let NdotL_pl = max(dot(N, Lpl),    0.0);
        let NdotH_pl = max(dot(N, Hpl),    0.0);
        let VdotH_pl = max(dot(V, Hpl),    0.0);

        let D_pl = distribution_ggx(NdotH_pl, roughness);
        let G_pl = geometry_smith(NdotV, NdotL_pl, roughness);
        let F_pl = fresnel_schlick(VdotH_pl, F0);
        let spec_pl  = (D_pl * G_pl * F_pl) / (4.0 * NdotV * NdotL_pl + 0.0001);
        let kD_pl    = (vec3<f32>(1.0) - F_pl) * (1.0 - metallic);
        let atten    = point_attenuation(pl_dist, pl_radius);
        let radiance_pl = light_color * pl_intensity * atten;
        Lo += (kD_pl * albedo / PI + spec_pl) * radiance_pl * NdotL_pl;
    }

    // Ambient: Fake Hemisphere / IBL Environment
    let F_ambient  = fresnel_schlick(max(dot(N, V), 0.0), F0);
    let kD_ambient = (vec3<f32>(1.0) - F_ambient) * (1.0 - metallic);
    
    let sky_color = vec3<f32>(0.5, 0.7, 1.0) * 1.2;
    let ground_color = vec3<f32>(0.2, 0.15, 0.1);
    
    // Diffuse ambient (Irradiance) based on surface normal
    let irradiance = mix(ground_color, sky_color, N.y * 0.5 + 0.5);
    let diffuse_ambient = kD_ambient * albedo * irradiance;

    // Specular ambient (Radiance) based on reflection vector
    let R = reflect(-V, N);
    let R_blend = mix(N.y, R.y, 1.0 - roughness); // blur for rough materials
    let specular_radiance = mix(ground_color, sky_color, R_blend * 0.5 + 0.5);

    let G_ambient  = geometry_smith_ibl(max(dot(N, V), 0.0001), roughness);
    let specular_ambient = F_ambient * G_ambient * specular_radiance;
    
    let ambient    = (diffuse_ambient + specular_ambient) * ao_factor * 0.8; // intensidad global
    var color   = ambient + Lo;

    // emissive
    if ((material.flags & 8u) != 0u) {
        let emiss = material.emissive.xyz * material.emissive.w;
        let es    = textureSample(tex_emissive, mat_sampler, frag_in.uv);
        color += emiss * es.xyz;
    }

    // tone mapping
    color = color / (color + vec3<f32>(1.0));
    // Hardware applies sRGB gamma correction for us

    var result = vec4<f32>(color, out_alpha);
    // alpha mask
    if ((material.flags & 32u) != 0u) {
        if (result.a < material.alpha_cutoff) { discard; }
    }
    return result;
}
