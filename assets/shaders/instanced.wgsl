// Instanced PBR shader — Full Physically Based Rendering
const PI: f32 = 3.14159265359;

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
@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var<storage, read> instances: array<mat4x4<f32>>;

struct MaterialUniform {
    base_color : vec4<f32>,
    emissive : vec4<f32>,           // w == strength
    metallic_roughness : vec4<f32>, // x=met, y=rough, z=ao_strength, w=opacity
    extra_params : vec4<f32>,       // x=normal_scale, y=clearcoat, z=clearcoat_rough
    flags : u32,
    alpha_cutoff: f32,
    _pad: vec2<u32>,
    _pad1: vec4<u32>,
};
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var mat_sampler: sampler;
@group(2) @binding(2) var tex_albedo: texture_2d<f32>;
@group(2) @binding(3) var tex_normal: texture_2d<f32>;
@group(2) @binding(4) var tex_met_rough: texture_2d<f32>;
@group(2) @binding(5) var tex_emissive: texture_2d<f32>;
@group(2) @binding(6) var tex_ao: texture_2d<f32>;

struct DirectionalLight {
    direction: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    intensity: f32,
    light_view_proj : mat4x4<f32>,
};
@group(3) @binding(0) var<uniform> dir_light: DirectionalLight;
@group(3) @binding(1) var env_sampler: sampler;
@group(3) @binding(2) var tex_irradiance: texture_cube<f32>;
@group(3) @binding(3) var tex_prefilter: texture_cube<f32>;
@group(3) @binding(4) var tex_brdf: texture_2d<f32>;

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
@group(3) @binding(6) var shadow_sampler: sampler_comparison;
@group(3) @binding(7) var shadow_map: texture_depth_2d;
@group(3) @binding(8) var ssao_tex: texture_2d<f32>;
@group(3) @binding(9) var ssao_sampler: sampler;

// -- PBR UTILS --
fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = (NdotH * NdotH) * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom + 0.0001);
}
fn geometry_schlick_ggx_direct(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k + 0.0001);
}
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let ggx1 = geometry_schlick_ggx_direct(NdotV, roughness);
    let ggx2 = geometry_schlick_ggx_direct(NdotL, roughness);
    return ggx1 * ggx2;
}
fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}
fn point_attenuation(dist: f32, radius: f32) -> f32 {
    let d_over_r = dist / radius;
    let numerator = saturate(1.0 - d_over_r * d_over_r * d_over_r * d_over_r);
    return (numerator * numerator) / (dist * dist + 1.0);
}

struct VsIn {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) tangent  : vec4<f32>,
    @location(3) color    : vec4<f32>,
    @location(4) uv       : vec2<f32>,
    @builtin(instance_index) instance_idx: u32,
};

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) world_pos : vec3<f32>,
    @location(1) world_normal : vec3<f32>,
    @location(2) world_tangent : vec3<f32>,
    @location(3) world_bitangent : vec3<f32>,
    @location(4) uv : vec2<f32>,
    @location(5) shadow_pos : vec4<f32>,
    @location(6) color : vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    let model = instances[input.instance_idx];
    let world_pos4 = model * vec4<f32>(input.position, 1.0);
    var out: VsOut;
    out.clip_pos = camera.view_proj * world_pos4;
    out.world_pos = world_pos4.xyz;
    out.shadow_pos = dir_light.light_view_proj * world_pos4;
    out.world_normal = normalize((model * vec4<f32>(input.normal, 0.0)).xyz);
    out.world_tangent = normalize((model * vec4<f32>(input.tangent.xyz, 0.0)).xyz);
    out.world_bitangent = normalize(cross(out.world_normal, out.world_tangent) * input.tangent.w);
    out.uv = vec2<f32>(input.uv.x, 1.0 - input.uv.y); // Flip V for WebGPU
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(frag_in: VsOut) -> @location(0) vec4<f32> {
    var albedo = material.base_color.xyz * frag_in.color.xyz;
    var out_alpha = material.base_color.w * material.metallic_roughness.w * frag_in.color.w;
    
    if ((material.flags & 1u) != 0u) {
        let sample = textureSampleLevel(tex_albedo, mat_sampler, frag_in.uv, 0.0);
        albedo *= sample.xyz;
        out_alpha *= sample.a;
    }

    var N = normalize(frag_in.world_normal);
    if ((material.flags & 2u) != 0u) {
        var normal_sample = textureSampleLevel(tex_normal, mat_sampler, frag_in.uv, 0.0).xyz * 2.0 - 1.0;
        normal_sample.y *= -1.0; // Flip Y for normal maps if needed
        let TBN = mat3x3<f32>(normalize(frag_in.world_tangent), normalize(frag_in.world_bitangent), N);
        N = normalize(TBN * (normal_sample * material.extra_params.x));
    }

    var metallic = material.metallic_roughness.x;
    var roughness = material.metallic_roughness.y;
    if ((material.flags & 4u) != 0u) {
        let mr = textureSampleLevel(tex_met_rough, mat_sampler, frag_in.uv, 0.0).xyz;
        roughness *= mr.y;
        metallic *= mr.z;
    }
    metallic  = clamp(metallic, 0.0, 1.0);
    roughness = clamp(roughness, 0.001, 1.0);

    let Vdir = normalize(camera.eye_pos - frag_in.world_pos);
    let Ldir = normalize(-dir_light.direction);
    let H    = normalize(Vdir + Ldir);
    let NdotV = max(dot(N, Vdir), 0.0001);
    let NdotL = max(dot(N, Ldir), 0.0);
    let NdotH = max(dot(N, H),    0.0);
    let VdotH = max(dot(Vdir, H), 0.0);

    let F0 = mix(vec3<f32>(0.04), albedo, metallic);
    let D  = distribution_ggx(NdotH, roughness);
    let G  = geometry_smith(NdotV, NdotL, roughness);
    let F  = fresnel_schlick(VdotH, F0);
    let specular = (D * G * F) / (4.0 * NdotV * NdotL + 0.0001);
    let kD = (vec3<f32>(1.0) - F) * (1.0 - metallic);
    
    // Shadows: textureSampleCompare must be outside non-uniform control flow
    let proj = frag_in.shadow_pos / frag_in.shadow_pos.w;
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, -proj.y * 0.5 + 0.5);
    let in_frustum = (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 && proj.z >= 0.0 && proj.z <= 1.0);
    
    let shadow_depth = textureSampleCompare(shadow_map, shadow_sampler, uv, proj.z - 0.005);
    let shadow = select(1.0, shadow_depth, in_frustum);
    
    var Lo = (kD * albedo / PI + specular) * dir_light.color * dir_light.intensity * NdotL * shadow;

    // Point lights
    let light_count = min(point_lights.count, 128u);
    for (var i: u32 = 0u; i < light_count; i = i + 1u) {
        let pl = point_lights.lights[i];
        let to_light = pl.position_radius.xyz - frag_in.world_pos;
        let pl_dist = length(to_light);
        if (pl_dist > pl.position_radius.w) { continue; }
        
        let Lpl = to_light / pl_dist;
        let Hpl = normalize(Vdir + Lpl);
        let NdotL_pl = max(dot(N, Lpl), 0.0);
        let F_pl = fresnel_schlick(max(dot(Vdir, Hpl), 0.0), F0);
        let spec_pl = (distribution_ggx(max(dot(N, Hpl), 0.0), roughness) * geometry_smith(NdotV, NdotL_pl, roughness) * F_pl) / (4.0 * NdotV * NdotL_pl + 0.0001);
        let kD_pl = (1.0 - F_pl) * (1.0 - metallic);
        Lo += (kD_pl * albedo / PI + spec_pl) * pl.color_intensity.xyz * pl.color_intensity.w * point_attenuation(pl_dist, pl.position_radius.w) * NdotL_pl;
    }

    // IBL
    let irr = textureSampleLevel(tex_irradiance, env_sampler, N, 0.0).xyz;
    let diffuse_ambient = (1.0 - fresnel_schlick(NdotV, F0)) * (1.0 - metallic) * albedo * irr;
    
    let R = reflect(-Vdir, N);
    let maxMip = f32(textureNumLevels(tex_prefilter) - 1u);
    let prefiltered = textureSampleLevel(tex_prefilter, env_sampler, R, roughness * maxMip).xyz;
    let brdf = textureSampleLevel(tex_brdf, env_sampler, vec2<f32>(NdotV, roughness), 0.0).xy;
    let specular_ambient = prefiltered * (fresnel_schlick(NdotV, F0) * brdf.x + brdf.y);

    // Sample the blurred SSAO texture using the fragment's NDC position.
    let clip_ssao = camera.view_proj * vec4<f32>(frag_in.world_pos, 1.0);
    let ndc_ssao  = clip_ssao.xyz / clip_ssao.w;
    let ssao_uv   = vec2<f32>(ndc_ssao.x * 0.5 + 0.5, -ndc_ssao.y * 0.5 + 0.5);
    let ssao_factor = textureSampleLevel(ssao_tex, ssao_sampler, ssao_uv, 0.0).r;

    let global_ambient = camera.ambient_color * camera.ambient_intensity * albedo;
    let total_ambient = (diffuse_ambient + specular_ambient + global_ambient) * material.metallic_roughness.z * ssao_factor;
    let ambient = total_ambient * 0.9 + global_ambient * 0.1;

    var color = ambient + Lo;
    
    // Emissive
    if ((material.flags & 8u) != 0u) {
        color += material.emissive.xyz * material.emissive.w;
    }

    // Fog
    let dist = length(camera.eye_pos - frag_in.world_pos);
    let fog_factor = clamp(1.0 - exp(-dist * camera.fog_density), 0.0, 1.0);
    color = mix(color, camera.fog_color, fog_factor);

    return vec4<f32>(color, out_alpha);
}
