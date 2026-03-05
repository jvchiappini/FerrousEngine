// Inverted-hull outline pass.
//
// Renders each mesh a second time with:
//   - back-face culling DISABLED (so inner faces are drawn)
//   - front-face culling ENABLED  (outer faces are culled → only the "hull" shell)
//   - vertices extruded along their normals by `outline_width` world units
//   - a solid colour (the `outline_color` stored in the CelParams block)
//
// Bind groups:
//   group(0) camera uniform
//   group(1) instance storage buffer
//   group(3) CelParams (binding 10) — carries outline_width + outline_color
//
// Groups 2 (material) is NOT needed — outline always renders as a flat colour.
// We still declare group(2) binding(0) with a dummy struct so the pipeline
// layout matches the shared layout; the actual buffer data is unused.

// ── Bind groups ───────────────────────────────────────────────────────────────

struct Camera {
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<storage, read> instances: array<mat4x4<f32>>;

// Minimal material placeholder — binding(0) must exist to satisfy the layout.
struct MaterialUniform {
    base_color          : vec4<f32>,
    emissive            : vec4<f32>,
    metallic_roughness  : vec4<f32>,
    normal_ao           : vec4<f32>,
    flags               : u32,
    alpha_cutoff        : f32,
    _pad                : vec2<u32>,
};
@group(2) @binding(0)
var<uniform> material: MaterialUniform;

struct DirectionalLight {
    direction       : vec3<f32>,
    _pad0           : f32,
    color           : vec3<f32>,
    intensity       : f32,
    light_view_proj : mat4x4<f32>,
};
@group(3) @binding(0)
var<uniform> dir_light: DirectionalLight;

struct CelParams {
    toon_levels   : u32,
    outline_width : f32,
    _pad0         : u32,
    _pad1         : u32,
};
@group(3) @binding(10)
var<uniform> cel_params: CelParams;

// Outline colour (4×f32 RGBA) at binding 11.
struct OutlineColor {
    color : vec4<f32>,
};
@group(3) @binding(11)
var<uniform> outline_color: OutlineColor;

// ── Vertex / Fragment IO ──────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
    @location(3) tangent  : vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos : vec4<f32>,
};

// ── Vertex shader ─────────────────────────────────────────────────────────────
//
// Extrudes vertex along its world-space normal before projecting.
// The extrusion amount is `outline_width` world units, which creates a
// constant-thickness outline in world space (not screen space).
// For a screen-space outline use the clip-space normal extrusion trick instead.

@vertex
fn vs_main(
    vert: VertexInput,
    @builtin(instance_index) idx: u32,
) -> VertexOutput {
    let model  = instances[idx];
    let w_pos  = (model * vec4<f32>(vert.position, 1.0)).xyz;
    let w_nrm  = normalize((model * vec4<f32>(vert.normal, 0.0)).xyz);

    // push vertex outward along the normal
    let extruded = w_pos + w_nrm * cel_params.outline_width;

    var out: VertexOutput;
    out.clip_pos = camera.view_proj * vec4<f32>(extruded, 1.0);
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return outline_color.color;
}
