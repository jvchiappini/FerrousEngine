/// Extremely Perfect 2D Sprite Pipeline
/// Instanced Rendering with internal Vertex Generation.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct InstanceInput {
    @location(0) transform_c0: vec4<f32>,
    @location(1) transform_c1: vec4<f32>,
    @location(2) transform_c2: vec4<f32>,
    @location(3) transform_c3: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) uv_rect: vec4<f32>,     // x, y, w, h
    @location(6) properties: vec4<f32>,  // x=flip_x, y=flip_y, z=is_lit, w=reserved
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// Generates simple quad in XY range [0.0 ... 1.0] with UVs
var<private> QUAD_POS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0)
);

@vertex
fn vs_main(
    @builtin(vertex_index) v_idx: u32,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    // Build model matrix from instance locations
    let model = mat4x4<f32>(
        instance.transform_c0,
        instance.transform_c1,
        instance.transform_c2,
        instance.transform_c3
    );

    // Quad mapping by Triangle Strip 
    let base_pos = QUAD_POS[v_idx];
    
    // UV logic including flip
    var final_uv = base_pos;
    if instance.properties.x > 0.0 { final_uv.x = 1.0 - final_uv.x; } // flip_x
    if instance.properties.y > 0.0 { final_uv.y = 1.0 - final_uv.y; } // flip_y
    
    // UV Rect mapping (Sprite sheet logic)
    out.uv = instance.uv_rect.xy + (final_uv * instance.uv_rect.zw);
    out.color = instance.color;

    // Shift to origin [-0.5, -0.5] if anchor is center
    let local_pos = vec4<f32>(base_pos.x - 0.5, base_pos.y - 0.5, 0.0, 1.0);
    let world_pos = model * local_pos;

    out.clip_position = camera.view_proj * world_pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(sprite_texture, sprite_sampler, in.uv);
    let out_color = tex_color * in.color;
    
    // Discard transparent pixels to preserve Z-buffer ordering properly!
    if out_color.a < 0.05 {
        discard;
    }

    return out_color;
}
