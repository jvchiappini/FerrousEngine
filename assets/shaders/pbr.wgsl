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
    // extra field populated by the CPU; used by the shadow pass and
    // optionally sampled by the PBR shader to compute shadow coordinates.
    light_view_proj : mat4x4<f32>,
};

@group(3) @binding(0)
var<uniform> dir_light: DirectionalLight;

// IBL resources bound alongside the directional light.  sampler at 1,
// irradiance cube at 2, prefiltered specular cube at 3, BRDF LUT at 4.
@group(3) @binding(1)
var env_sampler: sampler;
@group(3) @binding(2)
var tex_irradiance: texture_cube<f32>;
@group(3) @binding(3)
var tex_prefilter: texture_cube<f32>;
@group(3) @binding(4)
var tex_brdf: texture_2d<f32>;

// shadow map stored in the same bind group as lights (group 3)
@group(3) @binding(6)
var shadow_sampler: sampler_comparison;
@group(3) @binding(7)
var shadow_map: texture_depth_2d;

// ── Point Lights (Storage Buffer) ────────────────────────────────────────────
// binding(5) follows the four IBL resources at bindings 1-4.
// STD430 layout: a 16-byte header (count + 12 bytes padding) followed by
// a runtime-sized array of PointLight structs (each 32 bytes / 2x vec4).
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
// Formula: saturate(1 - (d/r)^4)^2 / (d^2 + 1)
// The + 1 in the denominator prevents the singularity at d=0.
fn point_attenuation(dist: f32, radius: f32) -> f32 {
    let d_over_r = dist / radius;
    let numerator = saturate(1.0 - d_over_r * d_over_r * d_over_r * d_over_r);
    return (numerator * numerator) / (dist * dist + 1.0);
}


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
    @location(6) shadow_pos : vec4<f32>,
};

// vertex shader
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos4 = model.model * vec4<f32>(in.position, 1.0);
    out.clip_pos = camera.view_proj * world_pos4;
    out.world_pos = world_pos4.xyz;
    out.shadow_pos = dir_light.light_view_proj * world_pos4;

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
    @location(6) shadow_pos : vec4<f32>,
};

struct FragmentOutput {
    @location(0) frag_color : vec4<f32>,
};

// fragment shader
@fragment
fn fs_main(frag_in: FragmentInput) -> FragmentOutput {
    var albedo = material.base_color.xyz * frag_in.color;
    // track alpha separately; start with material base alpha
    var out_alpha = material.base_color.w;
    if ((material.flags & 1u) != 0u) {
        let sample = textureSample(tex_albedo, mat_sampler, frag_in.uv);
        albedo *= sample.xyz;
        // modulate alpha by texture's alpha channel as well
        out_alpha *= sample.a;
    }
    var ao_factor = material.metallic_roughness.z;  // ao_strength
    if ((material.flags & 16u) != 0u) {
        ao_factor = textureSample(tex_ao, mat_sampler, frag_in.uv).x * ao_factor;
    }

    // normal mapping
    var N = normalize(frag_in.world_normal);
    if ((material.flags & 2u) != 0u) {
        var normal_sample = textureSample(tex_normal, mat_sampler, frag_in.uv).xyz * 2.0 - vec3<f32>(1.0);
        // WGSL forbids writing to swizzles; expand manually.
        normal_sample.x = normal_sample.x * material.normal_ao.x;
        normal_sample.y = normal_sample.y * material.normal_ao.x;
        let T = normalize(frag_in.world_tangent);
        let B = normalize(frag_in.world_bitangent);
        let TBN = mat3x3<f32>(T, B, N);
        N = normalize(TBN * normal_sample);
    }

    // metallic / roughness
    var metallic = material.metallic_roughness.x;
    var roughness = material.metallic_roughness.y;
    if ((material.flags & 4u) != 0u) {
        let mr = textureSample(tex_met_rough, mat_sampler, frag_in.uv).xyz;
        roughness *= mr.y;
        metallic *= mr.z;
    }
    metallic  = clamp(metallic,  0.0,  1.0);
    // 0.001 minimum: avoids GGX singularity while still allowing near-mirror surfaces.
    // For a perfect mirror set metallic=1, roughness=0 in the inspector.
    roughness = clamp(roughness, 0.001, 1.0);

    // View vector: world-space direction from fragment to camera eye.
    let Vdir = normalize(camera.eye_pos.xyz - frag_in.world_pos);

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
    // shadow computation using the depth texture rendered earlier from the
    // light's point of view.  We project the world position and compare the
    // stored depth against the fragment's depth to get a visibility factor.
    var shadow: f32 = 1.0;
    let proj = frag_in.shadow_pos / frag_in.shadow_pos.w;
    // NDC: X and Y in [-1,1] with Y+ = up; UV: X in [0,1] with V+ = down.
    // Negate Y so that the shadow map is sampled right-side-up.
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, -proj.y * 0.5 + 0.5);
    // only sample when inside the shadow map bounds; outside means the
    // fragment is outside the light frustum and we conservatively treat it
    // as lit.
    if (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 && proj.z >= 0.0 && proj.z <= 1.0) {
        // PCF 3×3: sample a 3×3 grid of shadow map texels and average the
        // comparison results.  This softens the hard aliased shadow edges
        // at the cost of 9 texture fetches instead of 1.
        let texel = 1.0 / 2048.0;
        var pcf: f32 = 0.0;
        for (var sy: i32 = -1; sy <= 1; sy = sy + 1) {
            for (var sx: i32 = -1; sx <= 1; sx = sx + 1) {
                let offset = vec2<f32>(f32(sx), f32(sy)) * texel;
                pcf += textureSampleCompare(shadow_map, shadow_sampler, uv + offset, proj.z - 0.005);
            }
        }
        shadow = pcf / 9.0;
    }
    var Lo = (kD * albedo / PI + specular) * radiance * NdotL * shadow;

    // ── Point lights ─────────────────────────────────────────────────────────
    let light_count = min(point_lights.count, 1024u);
    for (var i: u32 = 0u; i < light_count; i = i + 1u) {
        let pl = point_lights.lights[i];
        let light_pos   = pl.position_radius.xyz;
        let pl_radius   = pl.position_radius.w;
        let light_color = pl.color_intensity.xyz;
        let pl_intensity = pl.color_intensity.w;

        let to_light = light_pos - frag_in.world_pos;
        let pl_dist  = length(to_light);

        // Early-out: fragment is beyond the light's influence radius.
        if (pl_dist > pl_radius) {
            continue;
        }

        let Lpl   = to_light / pl_dist; // normalised light direction
        let Hpl   = normalize(Vdir + Lpl);
        let NdotL_pl = max(dot(N, Lpl),    0.0);
        let NdotH_pl = max(dot(N, Hpl),    0.0);
        let VdotH_pl = max(dot(Vdir, Hpl), 0.0);

        let D_pl = distribution_ggx(NdotH_pl, roughness);
        let G_pl = geometry_smith(NdotV, NdotL_pl, roughness);
        let F_pl = fresnel_schlick(VdotH_pl, F0);
        let specular_pl = (D_pl * G_pl * F_pl) / (4.0 * NdotV * NdotL_pl + 0.0001);

        let kD_pl = (vec3<f32>(1.0) - F_pl) * (1.0 - metallic);

        let atten    = point_attenuation(pl_dist, pl_radius);
        let radiance_pl = light_color * pl_intensity * atten;
        Lo += (kD_pl * albedo / PI + specular_pl) * radiance_pl * NdotL_pl;
    }

    // Ambient: Fake Hemisphere / IBL Environment
    let F_ambient = fresnel_schlick(max(dot(N, Vdir), 0.0), F0);
    let kD_ambient = (vec3<f32>(1.0) - F_ambient) * (1.0 - metallic);
    
    // Sample IBL textures rather than fake hemisphere colors.
    // irradiance map stores diffuse lighting.
    let irr = textureSample(tex_irradiance, mat_sampler, N).xyz;
    let diffuse_ambient = kD_ambient * albedo * irr;

    // specular prefiltered environment map: choose mip based on roughness
    let R = reflect(-Vdir, N);
    // roughness -> mip level (0 = crisp, max = rough)
    // `textureNumMipLevels` is not available on all backends; use the
    // WGSL built-in `textureNumLevels` which returns the total number of
    // mipmap levels for a sampled texture.
    let maxMip = textureNumLevels(tex_prefilter);
    let mip = roughness * f32(maxMip - 1);
    let prefiltered = textureSampleLevel(tex_prefilter, mat_sampler, R, mip).xyz;
    let brdf = textureSample(tex_brdf, mat_sampler, vec2<f32>(max(dot(N, Vdir),0.0), roughness)).xy;
    let specular_ambient = prefiltered * (F_ambient * brdf.x + brdf.y);

    let ambient = (diffuse_ambient + specular_ambient) * ao_factor * 0.8; // intensidad global del ambiente

    var color = ambient + Lo;

    // emissive
    if ((material.flags & 8u) != 0u) {
        let emiss = material.emissive.xyz * material.emissive.w;
        let sample = textureSample(tex_emissive, mat_sampler, frag_in.uv);
        color += emiss * sample.xyz;
    }

    // ── No tone mapping or gamma correction here ──────────────────────────
    // This pass writes to a Rgba16Float HDR texture.  Values may exceed 1.0
    // — that is intentional and correct.  All colour grading (ACES tone
    // mapping + sRGB gamma correction) is performed by the post-process pass
    // that reads this texture and writes to the final swapchain surface.

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
