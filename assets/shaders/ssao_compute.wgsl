// SSAO — Screen Space Ambient Occlusion (Compute Version)
//
// Optimized via Compute Shader to allow for shared memory optimizations
// and better GPU utilization.

struct SsaoParams {
    noise_scale  : vec2<f32>,
    radius       : f32,
    bias         : f32,
    proj         : mat4x4<f32>,
    inv_proj     : mat4x4<f32>,
    screen_size  : vec2<f32>,
    kernel_size  : u32,
    _pad         : u32,
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

@group(1) @binding(2)
var noise_tex     : texture_2d<f32>;
@group(1) @binding(3)
var noise_sampler : sampler;

// Output: R32Float storage texture (half-res)
@group(1) @binding(4)
var out_tex : texture_storage_2d<r32float, write>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn reconstruct_view_pos(uv: vec2<f32>, linear_depth: f32) -> vec3<f32> {
    let ndc_x = uv.x * 2.0 - 1.0;
    let ndc_y = (1.0 - uv.y) * 2.0 - 1.0;
    let view_ray = params.inv_proj * vec4<f32>(ndc_x, ndc_y, -1.0, 1.0);
    let ray_dir = view_ray.xyz / view_ray.w;
    return ray_dir * (linear_depth / -ray_dir.z);
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

    let uv = (vec2<f32>(screen_pos) + 0.5) / dims;

    // Sample normal+depth
    let nd = textureSampleLevel(normal_depth_tex, normal_depth_sampler, uv, 0.0);
    let linear_d = nd.a;

    if (linear_d < 0.001) {
        textureStore(out_tex, screen_pos, vec4<f32>(1.0, 0.0, 0.0, 0.0));
        return;
    }

    let view_normal = normalize(nd.xyz * 2.0 - vec3<f32>(1.0));
    let frag_pos = reconstruct_view_pos(uv, linear_d);

    let noise_uv = uv * params.noise_scale;
    let rvec = textureSampleLevel(noise_tex, noise_sampler, noise_uv, 0.0).xyz * 2.0 - vec3<f32>(1.0);
    let tbn = tbn_from_normal(view_normal, rvec);

    var occlusion = 0.0;
    let n_samples = params.kernel_size;

    for (var i: u32 = 0u; i < n_samples; i = i + 1u) {
        let s = kernel.samples[i].direction.xyz;
        var sample_pos = frag_pos + tbn * s * params.radius;

        var clip = params.proj * vec4<f32>(sample_pos, 1.0);
        clip = clip / clip.w;

        let sample_uv = vec2<f32>(clip.x * 0.5 + 0.5, -clip.y * 0.5 + 0.5);

        let scene_nd = textureSampleLevel(normal_depth_tex, normal_depth_sampler, sample_uv, 0.0);
        let scene_depth = scene_nd.a;

        let in_screen = (sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0);
        let in_range = smoothstep(0.0, 1.0, params.radius / abs(linear_d - scene_depth + 0.0001));
        
        if (in_screen && scene_depth >= ((-sample_pos.z) + params.bias)) {
            occlusion += in_range;
        }
    }

    let raw_ao = 1.0 - (occlusion / f32(n_samples));
    textureStore(out_tex, screen_pos, vec4<f32>(saturate(raw_ao), 0.0, 0.0, 0.0));
}
