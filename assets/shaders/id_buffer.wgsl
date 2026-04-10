// ─── GPU ID Buffer Shader ───────────────────────────────────────────────────────
//
// Renderiza quads pero escribe únicamente su `node_id` en una textura uint32
// para hit-testing pixel-perfect en la UI (radios de los bordes).

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) radii:     vec4<f32>,
    @location(1) uv0:       vec2<f32>, // Para offsets de sombra / blur extra
    @location(2) size:      vec2<f32>,
    @location(3) @interpolate(flat) node_id:   u32,
    @location(4) @interpolate(flat) flags:     u32,
    @location(5) raw_uv:    vec2<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var radius: f32;
    if p.x <= 0.0 && p.y <= 0.0 { radius = r.x; }
    else if p.x > 0.0 && p.y <= 0.0 { radius = r.y; }
    else if p.x <= 0.0 && p.y > 0.0 { radius = r.z; }
    else { radius = r.w; }
    let q = abs(p) - b + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

@vertex
fn vs_main(
    @location(0)  uv:          vec2<f32>,
    @location(1)  i_pos:       vec2<f32>,
    @location(2)  i_size:      vec2<f32>,
    @location(3)  i_uv0:       vec2<f32>,
    @location(4)  i_uv1:       vec2<f32>,
    @location(5)  i_color:     vec4<f32>,
    @location(6)  i_color_b:   vec4<f32>,
    @location(7)  i_radii:     vec4<f32>,
    @location(8)  i_tex_index: u32,
    @location(9)  i_flags:     u32,
    @location(10) i_z_order:   f32,
    @location(11) i_node_id:   u32,
) -> VsOut {
    var pixel = i_pos + uv * i_size;
    var ndc   = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    // Evitamos renderizar las sombras suaves como colisionables al 100% z-order.
    out.clip_pos  = vec4<f32>(ndc, 1.0 - i_z_order, 1.0);
    out.radii     = i_radii;
    out.uv0       = i_uv0;
    out.size      = i_size;
    out.node_id   = i_node_id;
    out.flags     = i_flags;
    out.raw_uv    = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) u32 {
    let px = in.raw_uv.x * in.size.x;
    let py = in.raw_uv.y * in.size.y;
    let half_size = in.size * 0.5;
    let p_center = vec2<f32>(px - half_size.x, py - half_size.y);
    
    // Ignoramos completamente formas que son meras sombras (SHADOW_BIT = 0x80u o 128u) 
    // porque las sombras no deben responder a los clics del mouse con su ID.
    if (in.flags & 128u) != 0u {
        discard;
    }
    
    // Si la forma tiene bordes redondeados (o bordes normales de color), 
    // verificamos que el cursor caiga dentro del cuerpo sólido de la geometría.
    let sdf = sdf_rounded_rect(p_center, half_size, in.radii);
    // 0 es el contorno puro. Mayor que 0.5 o 1 px significa que está fuera.
    // Usamos fwidth o simple check porque aquí solo importa pure inside/outside sin AA:
    if sdf > 0.0 {
        discard;
    }
    
    return in.node_id;
}
