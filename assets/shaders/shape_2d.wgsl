struct CameraUniform {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    padding: vec2<f32>,   // Explicitly matching the 80-byte Rust struct
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) transform_c0: vec4<f32>,
    @location(1) transform_c1: vec4<f32>,
    @location(2) transform_c2: vec4<f32>,
    @location(3) transform_c3: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) params: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>,
    @location(3) world_pos: vec2<f32>,
    @location(4) local_size: vec2<f32>, 
    @location(5) world_pixel_size: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let transform = mat4x4<f32>(
        in.transform_c0,
        in.transform_c1,
        in.transform_c2,
        in.transform_c3
    );

    let local_size = vec2<f32>(
        length(transform[0].xyz),
        length(transform[1].xyz)
    );

    let pos_in = vec2<f32>(
        f32(in.vertex_index & 1u),
        f32(in.vertex_index >> 1u)
    ) - 0.5;

    // --- High-Precision Pixel Size Calculation ---
    // view_proj[0][0] is (2.0 / width_in_world_units) in Ortho
    let res = max(camera.resolution, vec2<f32>(1.0, 1.0));
    let world_pixel_size = 2.0 / (abs(camera.view_proj[0][0]) * res.x);
    let padding = world_pixel_size * 4.0;

    let expansion_factor = 1.0 + (padding * 2.0 / local_size);
    let expanded_local_pos = pos_in * expansion_factor;
    
    let world_pos_4 = transform * vec4<f32>(expanded_local_pos, 0.0, 1.0);
    
    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_pos_4;
    out.local_pos = expanded_local_pos; 
    out.color = in.color;
    out.params = in.params;
    out.world_pos = world_pos_4.xy;
    out.local_size = local_size;
    out.world_pixel_size = world_pixel_size;
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let local_size = in.local_size;
    let p = in.local_pos * local_size;
    
    // Exact Euclidean Distance to rounded rectangle
    let radius = in.params.y;
    let q = abs(p) - (local_size * 0.5) + vec2<f32>(radius);
    let d = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;

    // --- Anti-Aliasing (Stable Pre-calculated) ---
    // We use the pixel size calculated in VS for stability, 
    // especially on diagonals where dpdx fluctuates.
    let grad_len = in.world_pixel_size;
    
    // Pixel-scale stabilization:
    // Ensure the line is always at least 1.5 pixels thick.
    let target_pixel_width = 1.8; // Slightly thicker for better stability on diagonals
    let min_world_width = grad_len * target_pixel_width;
    let actual_width = in.params.x; 
    
    var final_d = d;
    var alpha_multiplier = 1.0;
    
    if (actual_width < min_world_width) {
        let expansion = (min_world_width - actual_width) * 0.5;
        final_d -= expansion;
        alpha_multiplier = clamp(actual_width / min_world_width, 0.2, 1.0);
    }

    // Smoothing range (usually 1-2 pixels)
    let smoothing = in.params.z; 
    let filter_width = grad_len * smoothing;
    
    // Main shape alpha
    var alpha = 1.0 - smoothstep(-filter_width, filter_width, final_d);

    let final_alpha = alpha * alpha_multiplier;

    if (final_alpha < 0.01) {
        discard;
    }

    return vec4<f32>(in.color.rgb * 2.0, in.color.a * final_alpha);
}
