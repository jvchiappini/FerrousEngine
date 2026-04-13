// SSAO — Screen Space Ambient Occlusion (Compute Version)
//
// Professional-grade SSAO implementation using hemisphere kernel sampling
// in view space. Designed for high-fidelity results with minimal artifacts.

struct SsaoParams {
    noise_scale  : vec2<f32>,
    radius       : f32,
    bias         : f32,
    intensity    : f32,
    power        : f32,
    screen_size  : vec2<f32>,
    proj         : mat4x4<f32>,
    inv_proj     : mat4x4<f32>,
    kernel_size  : u32,
    _pad         : u32,
    _pad1        : u32,
    _pad2        : u32,
};

@group(0) @binding(0)
var<uniform> params: SsaoParams;

struct KernelSample {
    direction : vec4<f32>,
};
struct SsaoKernel {
    samples : array<KernelSample, 64>,
};
@group(0) @binding(1)
var<uniform> kernel: SsaoKernel;

@group(1) @binding(0)
var normal_depth_tex : texture_2d<f32>;
@group(1) @binding(1)
var normal_depth_sampler : sampler;

// Noise texture: sampled via textureLoad (no sampler needed)
@group(1) @binding(2)
var noise_tex     : texture_2d<f32>;

// Output: R32Float storage texture (half-res)
@group(1) @binding(3)
var out_tex : texture_storage_2d<r32float, write>;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Reconstruct view-space position from UV + linear depth.
fn reconstruct_view_pos(uv: vec2<f32>, linear_depth: f32) -> vec3<f32> {
    // Convert UV → NDC (WebGPU NDC z is [0, 1])
    let ndc_x =  uv.x * 2.0 - 1.0;
    let ndc_y = (1.0 - uv.y) * 2.0 - 1.0;
    let ndc = vec4<f32>(ndc_x, ndc_y, 0.5, 1.0);
    
    // Unproject to an arbitrary point on the view ray
    let view_ray = params.inv_proj * ndc;
    let ray_dir = view_ray.xyz / view_ray.w;
    
    // Scale the ray so its Z-coordinate equals -linear_depth
    return ray_dir * (-linear_depth / ray_dir.z);
}

fn tbn_from_normal(normal: vec3<f32>, rvec: vec3<f32>) -> mat3x3<f32> {
    let tangent   = normalize(rvec - normal * dot(rvec, normal));
    let bitangent = cross(normal, tangent);
    return mat3x3<f32>(tangent, bitangent, normal);
}

// ── Compute Shader ───────────────────────────────────────────────────────────

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let screen_pos = global_id.xy;
    let dims = vec2<f32>(textureDimensions(out_tex));
    
    if (f32(screen_pos.x) >= dims.x || f32(screen_pos.y) >= dims.y) {
        return;
    }

    // UV coords in [0,1] for this SSAO pixel (half-res)
    let uv = (vec2<f32>(screen_pos) + 0.5) / dims;

    // ── 1. Sample normal-depth at full-res ────────────────────────────────────
    // The normal-depth texture is full-res, SSAO is half-res.
    // uv maps correctly since both cover the same normalized viewport.
    let nd = textureSampleLevel(normal_depth_tex, normal_depth_sampler, uv, 0.0);
    let linear_d = nd.a;

    // Skip sky / empty pixels (depth == 0 means nothing was rendered there)
    if (linear_d < 0.001) {
        textureStore(out_tex, screen_pos, vec4<f32>(1.0, 0.0, 0.0, 0.0));
        return;
    }

    // Unpack view-space normal from [0,1] → [-1,1]
    let view_normal = normalize(nd.xyz * 2.0 - vec3<f32>(1.0));
    // Reconstruct view-space fragment position
    let frag_pos = reconstruct_view_pos(uv, linear_d);

    // ── 2. Random rotation vector from tiled noise texture ────────────────────
    // Tile the 4x4 noise texture perfectly across the screen using integer modulo
    let noise_coord = vec2<i32>(screen_pos) % vec2<i32>(4);
    let noise_sample = textureLoad(noise_tex, noise_coord, 0);
    let rvec = normalize(noise_sample.xyz * 2.0 - vec3<f32>(1.0));
    let tbn = tbn_from_normal(view_normal, rvec);

    // ── 3. Hemisphere sampling ────────────────────────────────────────────────
    var occlusion = 0.0;
    let n_samples = min(params.kernel_size, 64u);

    for (var i: u32 = 0u; i < n_samples; i = i + 1u) {
        // Kernel sample in tangent space → view space
        let s = kernel.samples[i].direction.xyz;
        let sample_pos = frag_pos + tbn * s * params.radius;

        // Project sample to screen space
        var clip = params.proj * vec4<f32>(sample_pos, 1.0);
        clip = clip / clip.w;

        // Clip-space to UV [0,1]; flip Y for wgpu/Vulkan convention
        let sample_uv = vec2<f32>(clip.x * 0.5 + 0.5, -clip.y * 0.5 + 0.5);

        // Skip samples that project outside the screen
        if (sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0) {
            continue;
        }

        // Fetch the scene depth at the sample's projected position
        let scene_nd = textureSampleLevel(normal_depth_tex, normal_depth_sampler, sample_uv, 0.0);
        let scene_depth = scene_nd.a;

        // Skip empty pixels in the depth buffer
        if (scene_depth < 0.001) {
            continue;
        }

        // The expected depth of our sample (positive, = -sample_pos.z in view space)
        let expected_depth = -sample_pos.z;

        // Self-occlusion bias: slightly larger for distant geometry
        let depth_bias = params.bias * max(1.0, expected_depth * 0.1);

        // Range check: fade out occlusion contribution for geometry that is
        // far apart in depth (prevents halos on object silhouettes).
        // Uses smoothstep to naturally fall off up to the radius * 2.
        let depth_diff = abs(linear_d - scene_depth);
        let range_check = 1.0 - smoothstep(params.radius * 0.1, params.radius * 2.0, depth_diff);

        // Occlusion test:
        // If the scene_depth is `<` our hemisphere sample's depth, it means the
        // solid geometry of the scene is CLOSER to the camera than our sample.
        // Therefore, the geometry blocks (occludes) our sample!
        if (scene_depth <= expected_depth - depth_bias) {
            occlusion += range_check;
        }
    }

    let occ_factor = occlusion / f32(n_samples);

    // Distance fade: gracefully disable SSAO for very distant geometry
    // to prevent noise at far clip distances. (Fade between 20 and 50 units)
    let dist_fade = 1.0 - smoothstep(20.0, 50.0, linear_d);

    // Final AO: 1.0 = fully lit, 0.0 = fully occluded
    // We saturate(1.0 - occlusion) so 0 occlusion means 1.0 light.
    var ao = saturate(1.0 - occ_factor * params.intensity);
    ao = pow(ao, params.power);
    
    // Apply distance fade as a lerp towards 1.0 (no occlusion) based on distance
    ao = mix(1.0, ao, dist_fade);

    textureStore(out_tex, screen_pos, vec4<f32>(ao, 0.0, 0.0, 0.0));
}
