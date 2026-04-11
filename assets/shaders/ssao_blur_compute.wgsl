// SSAO Bilateral Blur (Compute Version)

struct BlurParams {
    texel_size   : vec2<f32>,
    direction    : u32,       // 0 = horizontal, 1 = vertical
    depth_thresh : f32,
};

@group(0) @binding(0)
var<uniform> params: BlurParams;

@group(1) @binding(0)
var ssao_tex     : texture_2d<f32>;
@group(1) @binding(1)
var ssao_sampler : sampler;

@group(1) @binding(2)
var nd_tex     : texture_2d<f32>;
@group(1) @binding(3)
var nd_sampler : sampler;

// Output: R32Float storage texture
@group(1) @binding(4)
var out_tex : texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let screen_pos = global_id.xy;
    let dims = vec2<f32>(textureDimensions(out_tex));
    
    if (f32(screen_pos.x) >= dims.x || f32(screen_pos.y) >= dims.y) {
        return;
    }

    let uv = (vec2<f32>(screen_pos) + 0.5) / dims;

    // Centres pixel depth
    let centre_depth = textureSampleLevel(nd_tex, nd_sampler, uv, 0.0).a;

    var step = vec2<f32>(0.0, 0.0);
    if (params.direction == 0u) {
        step = vec2<f32>(params.texel_size.x, 0.0);
    } else {
        step = vec2<f32>(0.0, params.texel_size.y);
    }

    var result = 0.0;
    var weight_sum = 0.0;

    for (var i: i32 = -2; i <= 2; i = i + 1) {
        let offset_uv   = uv + step * f32(i);
        let sample_ao   = textureSampleLevel(ssao_tex, ssao_sampler, offset_uv, 0.0).r;
        let sample_depth = textureSampleLevel(nd_tex, nd_sampler, offset_uv, 0.0).a;

        let depth_diff = abs(sample_depth - centre_depth);
        let w = select(0.0, 1.0, depth_diff < params.depth_thresh);

        result     += sample_ao * w;
        weight_sum += w;
    }

    let final_ao = select(
        textureSampleLevel(ssao_tex, ssao_sampler, uv, 0.0).r,
        result / weight_sum,
        weight_sum > 0.0001
    );

    textureStore(out_tex, screen_pos, vec4<f32>(final_ao, 0.0, 0.0, 0.0));
}
