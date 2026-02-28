// Shader para renderizar quads de UI. Las coordenadas se pasan en píxeles
// y se convierten a clip space mediante la resolución de la pantalla.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) uv: vec2<f32>,
    // size in pixels, required for correct corner SDF
    @location(3) size: vec2<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// helper to test a corner given uv, quad size, offset and radius

@vertex
fn vs_main(
    @location(0) uv: vec2<f32>,
    @location(1) i_pos: vec2<f32>,
    @location(2) i_size: vec2<f32>,
    @location(3) i_color: vec4<f32>,
    @location(4) i_radii: vec4<f32>,
) -> VsOut {
    // uv va de (0,0) en la esquina inferior izquierda a (1,1) en la
    // esquina superior derecha del quad unitario.
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    // invertir Y para coincidir con coordenadas de pantalla típicas
    ndc.y = -ndc.y;
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.color = i_color;
    out.radii = i_radii;
    out.uv = uv;
    out.size = i_size;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // compute rounded rectangle signed distance. based on Inigo Quilez's
    // "2D Rounded Box" formula adapted to uv coordinates.
    // we check each corner separately based on its own radius.
    let r_tl = in.radii.x;
    let r_tr = in.radii.y;
    let r_bl = in.radii.z;
    let r_br = in.radii.w;
    // helper to test a corner given its uv offset and radius
    // convert to pixel coordinates
    // since the quad vertex coordinates are generated with uv.y=0 at the
    // *top* edge (i_pos is the top-left of the rectangle), py increases
    // downward. treat py=0 as top.
    let px = in.uv.x * in.size.x;
    let py = in.uv.y * in.size.y;
    // top-left corner (py small)
    if (r_tl > 0.0 && px < r_tl && py < r_tl) {
        let dx = r_tl - px;
        let dy = r_tl - py;
        if (dx * dx + dy * dy > r_tl * r_tl) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    // top-right
    if (r_tr > 0.0 && px > (in.size.x - r_tr) && py < r_tr) {
        let dx = px - (in.size.x - r_tr);
        let dy = r_tr - py;
        if (dx * dx + dy * dy > r_tr * r_tr) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    // bottom-left
    if (r_bl > 0.0 && px < r_bl && py > (in.size.y - r_bl)) {
        let dx = r_bl - px;
        let dy = py - (in.size.y - r_bl);
        if (dx * dx + dy * dy > r_bl * r_bl) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    // bottom-right
    if (r_br > 0.0 && px > (in.size.x - r_br) && py > (in.size.y - r_br)) {
        let dx = px - (in.size.x - r_br);
        let dy = py - (in.size.y - r_br);
        if (dx * dx + dy * dy > r_br * r_br) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    return in.color;
}
