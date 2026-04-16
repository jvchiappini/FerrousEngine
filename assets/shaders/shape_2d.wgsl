struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
};

struct InstanceInput {
    @location(0) transform_c0: vec4<f32>,
    @location(1) transform_c1: vec4<f32>,
    @location(2) transform_c2: vec4<f32>,
    @location(3) transform_c3: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) params: vec4<f32>, // x=border_thickness, y=corner_radius, z=smoothing, w=is_filled
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>, // [-0.5, 0.5]
    @location(2) params: vec4<f32>,
    @location(3) scale: vec2<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let transform = mat4x4<f32>(
        instance.transform_c0,
        instance.transform_c1,
        instance.transform_c2,
        instance.transform_c3,
    );

    // Extract scale from transform (assuming orthogonal/uniform-ish)
    let scale_x = length(vec3<f32>(transform[0].xyz));
    let scale_y = length(vec3<f32>(transform[1].xyz));
    let scale = vec2<f32>(scale_x, scale_y);

    // Standard quad vertices
    let positions = array<vec2<f32>, 4>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>( 0.5,  0.5),
    );

    let pos = positions[model.vertex_index];
    
    // Anti-aliasing gradient padding
    // We expand the geometry quad by 0.5 world units on all sides to guarantee there 
    // is enough fragment space for the SDF smoothstep gradient to fully resolve to 0.0 alpha.
    // Failing to do this causes the triangle boundary to clip the gradient, 
    // resulting in hard aliased jagged edges for thin objects or rotated lines.
    let padding = 0.5;
    let expand_x = select(padding / scale.x, 0.0, scale.x < 0.0001);
    let expand_y = select(padding / scale.y, 0.0, scale.y < 0.0001);
    let expanded_pos = pos + sign(pos) * vec2<f32>(expand_x, expand_y);

    let world_pos = transform * vec4<f32>(expanded_pos, 0.0, 1.0);
    
    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_pos;
    out.color = instance.color;
    out.local_pos = expanded_pos;
    out.params = instance.params;
    out.scale = scale;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.local_pos * in.scale;
    let half_extents = in.scale * 0.5;

    // Exact Signed Distance Field for a Rounded Box
    // x = border_thickness, y = corner_radius, z = smoothing multiplier
    let r = in.params.y;
    let q = abs(p) - half_extents + vec2<f32>(r);
    
    // length(max) gives external distance, min(max) gives exact internal distance
    // This exact Euclidean distance is MANDATORY for fwidth() to work correctly,
    // otherwise the interior is flat (0.0) and fwidth becomes 0.
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;

    // Anti-aliasing using fwidth for pixel-perfect smoothing.
    // fwidth(d) is the exact variation across 1 physical pixel in clip-space.
    // Multiplying this by the smoothing factor (e.g. 1.5) produces a guaranteed
    // pixel-perfect anti-aliased edge that scales flawlessly with resolution and camera zoom.
    let aa_width = fwidth(d) * in.params.z;
    let alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

    // Border handling
    let border = in.params.x;
    var final_color = in.color;
    
    if (border > 0.0) {
        let interior_alpha = 1.0 - smoothstep(-aa_width, aa_width, d + border);
        // If not filled, subtract interior
        if (in.params.w == 0.0) {
            final_color.a *= (alpha - interior_alpha);
        }
    } else {
        final_color.a *= alpha;
    }

    // Clip low alpha pixels
    if (final_color.a < 0.001) {
        discard;
    }

    return final_color;
}

