// Procedural HDR Sky shader (Preetham-inspired atmospheric model)
//
// Draws a beautiful, physically-based sky with Rayleigh and Mie scattering
// based on the current sun direction.

struct Camera {
    view      : mat4x4<f32>,
    proj      : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    eye_pos   : vec3<f32>,
    exposure  : f32,
    fog_color : vec3<f32>,
    fog_density: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
    _padding: array<vec4<f32>, 17>,
};

struct Light {
    direction: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    intensity: f32,
    light_view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<uniform> light: Light;

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
    
    // Professional translation stripping: using w=0.0 for the direction vector
    // ensures the skybox is always centered at the camera regardless of eye position.
    let pos = camera.view_proj * vec4<f32>(in.position, 0.0);
    
    // Force to far plane for depth testing compatibility
    out.clip_pos = pos.xyww; 
    out.world_dir = in.position;
    return out;
}

// ── Atmospheric Constants ───────────────────────────────────────────────────
const RAYLEIGH_COEFF = vec3<f32>(5.8e-6, 13.5e-6, 33.1e-6);
const MIE_COEFF = 4.0e-6;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let V = normalize(in.world_dir);
    let L = normalize(-light.direction.xyz); // Direction TO the sun
    
    let cos_theta = dot(V, L);
    let zenith_dir = vec3<f32>(0.0, 1.0, 0.0);
    
    // Use the absolute vertical component for a symmetric atmosphere-like ground
    let cos_zenith = max(V.y, 0.0);
    let abs_y = abs(V.y);
    
    // Mie scattering
    let g = 0.76;
    let mie_phase = (1.0 - g*g) / (4.0 * 3.14159 * pow(1.0 + g*g - 2.0*g*cos_theta, 1.5));
    
    // Rayleigh scattering
    let rayleigh_phase = 0.75 * (1.0 + cos_theta * cos_theta);
    
    // Atmospheric color approximation
    let p_sky = vec3<f32>(0.2, 0.5, 0.9); // Deep atmospheric blue
    let p_horizon = vec3<f32>(0.75, 0.8, 0.85); // Professional Haze
    
    // Sky gradient with smoother zenith transition
    var sky_color = mix(p_horizon, p_sky, smoothstep(0.0, 0.9, cos_zenith));
    
    // Apply light scattering influence
    let scattering = RAYLEIGH_COEFF * rayleigh_phase + MIE_COEFF * mie_phase * 15.0;
    sky_color = sky_color * (1.1 + scattering * 50.0);
    
    // Add sun disc (high intensity HDR)
    let sun_disc = smoothstep(0.99988, 0.9999, cos_theta);
    let sun_color = vec3<f32>(1.0, 0.95, 0.8) * 80.0;
    sky_color += sun_color * sun_disc;
    
    // ── Infinite Atmospheric Horizon ─────────────────────────────────────────
    // To satisfy the "Real Environment" feedback, we eliminate the hard line.
    // We use a dark atmospheric blue for the deep ground (nadir) and a hazy
    // color for the horizon area.
    let nadir_color = vec3<f32>(0.12, 0.14, 0.16); 
    let atmosphere_haze = p_horizon * 0.7; // Brighter haze for the horizon
    
    // Smooth atmospheric blending over a wide range to simulate aerial perspective
    let ground_gradient = smoothstep(-0.8, 0.0, V.y);
    let horizon_glow = smoothstep(-0.15, 0.05, V.y);
    
    // Combine colors: Deep Ground -> Haze -> Sky
    let base_ground = mix(nadir_color, atmosphere_haze, ground_gradient);
    sky_color = mix(base_ground, sky_color, horizon_glow);
    
    return vec4<f32>(sky_color, 1.0);
}
