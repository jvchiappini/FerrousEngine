// SSAO — Screen Space Ambient Occlusion
//
// Reads the normal+depth prepass texture and the SSAO kernel/noise
// to compute a per-pixel occlusion factor in [0, 1].
// Runs at half resolution for performance.

// ── Uniforms ─────────────────────────────────────────────────────────────────

struct SsaoParams {
    // 4×4 rotation noise tile size
    noise_scale  : vec2<f32>,
    // world-space hemisphere radius (exposed to editor)
    radius       : f32,
    // depth-comparison bias (prevents self-occlusion)
    bias         : f32,
    // full-res projection matrix (to project samples to screen)
    proj         : mat4x4<f32>,
    // view-space reconstruction — we store the inverse projection so we can
    // recover view-space position from the depth value stored in the prepass.
    inv_proj     : mat4x4<f32>,
    // screen dimensions of the SSAO target (half-res)
    screen_size  : vec2<f32>,
    // number of kernel samples (up to 64)
    kernel_size  : u32,
    _pad         : u32,
};

@group(0) @binding(0)
var<uniform> params: SsaoParams;

// 64-sample hemisphere kernel, each vec3 in view space oriented toward +Z
struct KernelSample {
    direction : vec4<f32>, // xyz = sample direction, w = unused
};
struct SsaoKernel {
    samples : array<KernelSample, 64>,
};
@group(0) @binding(1)
var<uniform> kernel: SsaoKernel;

// Normal+depth texture from the prepass (full resolution)
@group(1) @binding(0)
var normal_depth_tex : texture_2d<f32>;
@group(1) @binding(1)
var normal_depth_sampler : sampler;

// 4×4 noise texture (tiled) to rotate the kernel per-pixel
@group(1) @binding(2)
var noise_tex     : texture_2d<f32>;
@group(1) @binding(3)
var noise_sampler : sampler;

// ── Helpers ──────────────────────────────────────────────────────────────────

// Reconstruct view-space position from UV + linear depth stored in prepass.
// The prepass stores -view_pos.z in the alpha channel.
fn reconstruct_view_pos(uv: vec2<f32>, linear_depth: f32) -> vec3<f32> {
    // Convert UV to NDC [-1, 1]; note V is flipped (wgpu top-left origin).
    let ndc_x = uv.x * 2.0 - 1.0;
    let ndc_y = (1.0 - uv.y) * 2.0 - 1.0;

    // Unproject using the inverse projection matrix.
    // We project the NDC point at depth = -1 (near plane) then scale by depth.
    let view_ray = params.inv_proj * vec4<f32>(ndc_x, ndc_y, -1.0, 1.0);
    // view_ray.xyz / view_ray.w gives the direction; scale by actual depth.
    let ray_dir = view_ray.xyz / view_ray.w;
    // Normalize direction and scale so Z = -linear_depth
    return ray_dir * (linear_depth / -ray_dir.z);
}

// Build a TBN matrix oriented along the input normal, rotated by rvec.
fn tbn_from_normal(normal: vec3<f32>, rvec: vec3<f32>) -> mat3x3<f32> {
    let tangent   = normalize(rvec - normal * dot(rvec, normal));
    let bitangent = cross(normal, tangent);
    return mat3x3<f32>(tangent, bitangent, normal);
}

// ── Fullscreen triangle ───────────────────────────────────────────────────────

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0)       uv  : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    // Generate a fullscreen triangle from 3 vertex indices.
    var out: VsOut;
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    out.uv  = vec2<f32>(x * 0.5, y * 0.5);
    out.pos = vec4<f32>(x - 1.0, 1.0 - y, 0.0, 1.0);
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

struct FsOut {
    @location(0) occlusion : f32,
};

@fragment
fn fs_main(in: VsOut) -> FsOut {
    let uv = in.uv;

    // Sample the prepass texture
    let nd       = textureSample(normal_depth_tex, normal_depth_sampler, uv);
    let linear_d = nd.a;

    // Skip sky/background pixels (depth == 0 means no geometry)
    if (linear_d < 0.001) {
        var sky_out: FsOut;
        sky_out.occlusion = 1.0;
        return sky_out;
    }

    // Unpack view-space normal from [0,1] → [-1,1]
    let view_normal = normalize(nd.xyz * 2.0 - vec3<f32>(1.0));

    // Reconstruct view-space position
    let frag_pos = reconstruct_view_pos(uv, linear_d);

    // Sample noise texture — tiled every 4 pixels
    let noise_uv = uv * params.noise_scale;
    let rvec     = textureSample(noise_tex, noise_sampler, noise_uv).xyz * 2.0 - vec3<f32>(1.0);

    // Build the per-pixel rotation matrix
    let tbn = tbn_from_normal(view_normal, rvec);

    // Accumulate occlusion
    var occlusion = 0.0;
    let n_samples = params.kernel_size;

    for (var i: u32 = 0u; i < n_samples; i = i + 1u) {
        // Transform hemisphere sample from tangent to view space
        let s   = kernel.samples[i].direction.xyz;
        var sample_pos = frag_pos + tbn * s * params.radius;

        // Project sample to screen space
        var clip = params.proj * vec4<f32>(sample_pos, 1.0);
        clip     = clip / clip.w;

        // Map NDC to UV (flip Y for wgpu convention)
        let sample_uv = vec2<f32>(
            clip.x * 0.5 + 0.5,
            -clip.y * 0.5 + 0.5
        );

        // Bounds check: skip samples that land outside the screen
        if (sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0) {
            continue;
        }

        // Read the actual geometry depth at the projected UV
        let scene_nd    = textureSample(normal_depth_tex, normal_depth_sampler, sample_uv);
        let scene_depth = scene_nd.a;

        // The sample is occluded if the geometry at that screen position is
        // closer to the camera than our sample (accounting for bias).
        let sample_depth = -sample_pos.z; // positive linear depth
        let in_range     = smoothstep(0.0, 1.0, params.radius / abs(linear_d - scene_depth + 0.0001));
        if (scene_depth >= (sample_depth + params.bias)) {
            occlusion += in_range;
        }
    }

    // Normalise and invert (1 = fully lit, 0 = fully occluded)
    let raw_ao = 1.0 - (occlusion / f32(n_samples));

    var out: FsOut;
    out.occlusion = saturate(raw_ao);
    return out;
}
