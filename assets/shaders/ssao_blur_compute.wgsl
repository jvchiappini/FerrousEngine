// SSAO Bilateral Blur (Compute Version)
//
// A separable 9-tap [Gaussian × bilateral] filter that smooths the raw
// half-resolution SSAO output while preventing occlusion from bleeding
// across geometric edges.  Two passes are dispatched: horizontal (dir=0)
// followed by vertical (dir=1).

struct BlurParams {
    texel_size   : vec2<f32>,
    direction    : u32,       // 0 = horizontal, 1 = vertical
    depth_thresh : f32,
};

@group(0) @binding(0)
var<uniform> params: BlurParams;

// Raw SSAO texture (Rgba8Unorm, non-filterable): use textureLoad
@group(1) @binding(0)
var ssao_tex     : texture_2d<f32>;

// Normal-depth texture (Rgba16Float, filterable): used for depth gating
@group(1) @binding(1)
var nd_tex     : texture_2d<f32>;
@group(1) @binding(2)
var nd_sampler : sampler;

// Output: Rgba8Unorm storage texture
@group(1) @binding(3)
var out_tex : texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let screen_pos = global_id.xy;
    let out_dims   = vec2<u32>(textureDimensions(out_tex));
    
    if (screen_pos.x >= out_dims.x || screen_pos.y >= out_dims.y) {
        return;
    }

    let uv = (vec2<f32>(screen_pos) + 0.5) / vec2<f32>(out_dims);

    // Centre pixel depth (from the full-res normal-depth texture)
    let centre_depth = textureSampleLevel(nd_tex, nd_sampler, uv, 0.0).a;

    // Build per-axis step in texel units of the SSAO texture
    var step = vec2<i32>(0, 0);
    if (params.direction == 0u) {
        step = vec2<i32>(1, 0);   // horizontal
    } else {
        step = vec2<i32>(0, 1);   // vertical
    }

    var result     = 0.0;
    var weight_sum = 0.0;

    let ssao_dims = vec2<i32>(textureDimensions(ssao_tex));
    let nd_dims   = vec2<i32>(textureDimensions(nd_tex));

    // 9-tap bilateral Gaussian blur (radius 4)
    for (var i: i32 = -4; i <= 4; i = i + 1) {
        let tap_pos   = vec2<i32>(screen_pos) + step * i;

        // Clamp to texture bounds
        let clamped_tap = clamp(tap_pos, vec2<i32>(0), ssao_dims - vec2<i32>(1));
        let sample_ao   = textureLoad(ssao_tex, clamped_tap, 0).r;

        // Fetch depth at this tap from the full-res normal-depth texture.
        // The two textures may be different resolutions, so convert via UV.
        let tap_uv       = (vec2<f32>(clamped_tap) + 0.5) / vec2<f32>(ssao_dims);
        let sample_depth = textureSampleLevel(nd_tex, nd_sampler, tap_uv, 0.0).a;

        let depth_diff  = abs(sample_depth - centre_depth);
        // Gaussian weight by tap distance
        let dist_weight = exp(-f32(i * i) / 8.0);
        
        // Soft bilateral gate: gracefully reduce weight as depth difference increases.
        // We scale the threshold by the center depth so surfaces further away
        // (which naturally have larger depth gradients) don't falsely reject all taps.
        let dynamic_thresh = params.depth_thresh * max(1.0, centre_depth * 0.5);
        let bilateral_w = saturate(1.0 - (depth_diff / dynamic_thresh));
        
        let w = dist_weight * bilateral_w;

        result     += sample_ao * w;
        weight_sum += w;
    }

    // Fallback to original value if all taps were rejected (extremely rare now)
    let centre_ao  = textureLoad(ssao_tex, vec2<i32>(screen_pos), 0).r;
    let final_ao   = select(centre_ao, result / weight_sum, weight_sum > 0.0001);

    textureStore(out_tex, screen_pos, vec4<f32>(final_ao, 0.0, 0.0, 0.0));
}
