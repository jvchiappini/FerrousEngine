// Simple skybox shader.  Vertex shader draws a unit cube with a
// position-only vertex format (Vertex::layout() supplies position).
// The vertex shader transforms positions by the camera's view-proj matrix
// but strips translation so the cube follows the camera.

struct Camera {
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: Camera;

// environment sampler / cubemap delivered in group1 (same as PBR)
// binding 3 = prefiltered environment cubemap (1024x1024 with full mip chain).
// binding 2 = irradiance (32x32 blurry), which is NOT what we want for skybox.
@group(1) @binding(1)
var env_sampler: sampler;
@group(1) @binding(3)
var tex_env: texture_cube<f32>;

struct VertexInput {
    @location(0) position : vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) world_dir : vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // remove translation from view-proj
    let view_no_trans = mat4x4<f32>(
        camera.view_proj[0],
        camera.view_proj[1],
        camera.view_proj[2],
        vec4<f32>(0.0,0.0,0.0,1.0)
    );
    out.clip_pos = view_no_trans * vec4<f32>(in.position, 1.0);
    out.world_dir = in.position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample mip 0 explicitly to get the sharpest version of the environment.
    // The prefiltered cube's mip 0 was written with roughness=0, which is the
    // original undistorted environment — exactly right for the skybox background.
    let color = textureSampleLevel(tex_env, env_sampler, normalize(in.world_dir), 0.0);
    return vec4<f32>(color.rgb, 1.0);
}
