// Post-process pass: reads the HDR Rgba16Float intermediate texture,
// applies ACES Filmic Tone Mapping and sRGB gamma correction, and writes
// the result to the final swapchain surface.

// ── Bindings ─────────────────────────────────────────────────────────────────

@group(0) @binding(0) var t_hdr: texture_2d<f32>;
@group(0) @binding(1) var s_hdr: sampler;
@group(0) @binding(2) var t_bloom: texture_2d<f32>;
@group(0) @binding(3) var s_bloom: sampler;

// ── Vertex shader ─────────────────────────────────────────────────────────────
// Generates a fullscreen triangle from the vertex index — no vertex buffer needed.
// Indices 0-2 cover the entire NDC square through the classic clip-space trick.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VsOut {
    // Produces UV coords: (0,0), (2,0), (0,2)
    let raw_uv = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VsOut;
    // wgpu/Vulkan: V=0 is at the top, so we flip Y to match texture convention.
    out.uv  = vec2<f32>(raw_uv.x, 1.0 - raw_uv.y);
    out.pos = vec4<f32>(raw_uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

// ── Tone mapping ─────────────────────────────────────────────────────────────
// Narkowicz 2015 ACES approximation.
// Input:  linear HDR colour (can exceed 1.0).
// Output: SDR colour clamped to [0, 1].
fn aces_tone_mapping(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // 1. Sample the raw HDR colour from the intermediate texture.
    let hdr_color = textureSample(t_hdr, s_hdr, in.uv).rgb;
    // sample the bloom contribution produced by the bloom pass.  we expect
    // level-0 of the bloom chain to already contain the accumulated result.
    let bloom_color = textureSample(t_bloom, s_bloom, in.uv).rgb;

    // 2. Exposure — multiply before tone mapping to control overall brightness.
    //    Outdoor HDRIs are typically very bright (sky > 1.0), so an exposure
    //    value < 1.0 brings the scene into a nicer range for ACES.
    let exposure = 0.5;
    let exposed = (hdr_color + bloom_color * 0.15) * exposure;

    // 3. ACES filmic tone mapping — compresses the HDR range to [0, 1].
    let mapped = aces_tone_mapping(exposed);

    // 4. Write the linear tonemapped value directly to the swapchain surface.
    //    The surface is Bgra8UnormSrgb, so the hardware automatically applies
    //    the linear→sRGB conversion when storing the pixel.  We must NOT do a
    //    manual pow(x, 1/2.2) here or gamma would be applied twice.
    return vec4<f32>(mapped, 1.0);
}
