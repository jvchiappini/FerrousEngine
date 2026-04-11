// GPU Particle Render Shader

struct Particle {
    position : vec3<f32>,
    life     : f32,
    velocity : vec3<f32>,
    size     : f32,
};

struct SceneUniforms {
    view_proj : mat4x4<f32>,
    eye       : vec3<f32>,
    _pad      : f32,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

@group(1) @binding(0)
var<storage, read> particles: array<Particle>;

struct VsOut {
    @builtin(position) pos   : vec4<f32>,
    @location(0)       uv    : vec2<f32>,
    @location(1)       color : vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) v_idx: u32,
    @builtin(instance_index) i_idx: u32,
) -> VsOut {
    let p = particles[i_idx];
    
    if (p.life <= 0.0) {
        // Discard by moving off-screen
        return VsOut(vec4<f32>(0.0, 0.0, 0.0, 0.0), vec2<f32>(0.0), vec4<f32>(0.0));
    }

    // Quad vertices: (-0.5, -0.5) to (0.5, 0.5)
    let quad = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.5,  0.5)
    );

    let uv_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0)
    );

    let offset = quad[v_idx] * 0.1; // Base size
    
    // Cylindrical Billboarding
    let right = vec3<f32>(scene.view_proj[0][0], scene.view_proj[1][0], scene.view_proj[2][0]);
    let up    = vec3<f32>(scene.view_proj[0][1], scene.view_proj[1][1], scene.view_proj[2][1]);
    
    let world_pos = p.position + (right * offset.x + up * offset.y);
    
    var out: VsOut;
    out.pos = scene.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv  = uv_coords[v_idx];
    
    // Color fade over life
    let alpha = saturate(p.life);
    out.color = vec4<f32>(1.0, 0.5, 0.2, alpha);
    
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Soft circular particle
    let dist = length(in.uv - vec2<f32>(0.5));
    if (dist > 0.5) {
        discard;
    }
    
    let strength = 1.0 - smoothstep(0.0, 0.5, dist);
    return in.color * strength;
}
