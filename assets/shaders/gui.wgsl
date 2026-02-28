// Shader para renderizar quads de UI. Las coordenadas se pasan en píxeles
// y se convierten a clip space mediante la resolución de la pantalla.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) uv: vec2<f32>,
    // size in pixels, required for correct corner SDF
    @location(3) size: vec2<f32>,
    @location(4) flags: u32,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// utility: convert HSV colour to RGB.  This is shared with the CPU
// implementation so that colour computations match exactly.
fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(vec3<f32>(h) + K.xyz) * 6.0 - K.www);
    return v * mix(K.xxx, clamp(p - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

// simple passthrough vertex shader converting pixel coordinates to NDC
@vertex
fn vs_main(
    @location(0) uv: vec2<f32>,
    @location(1) i_pos: vec2<f32>,
    @location(2) i_size: vec2<f32>,
    @location(3) i_color: vec4<f32>,
    @location(4) i_radii: vec4<f32>,
    @location(5) i_flags: u32,
) -> VsOut {
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    ndc.y = -ndc.y;
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.color = i_color;
    out.radii = i_radii;
    out.uv = uv;
    out.size = i_size;
    out.flags = i_flags;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // wheel mode: render hue/saturation gradient
    if (in.flags == 1u) {
        let px = in.uv.x * in.size.x;
        let py = in.uv.y * in.size.y;
        let cx = in.size.x * 0.5;
        let cy = in.size.y * 0.5;

        let dx = px - cx;
        let dy = py - cy;
        let r = min(in.size.x, in.size.y) * 0.5;
        let dist = length(vec2<f32>(dx, dy));

        let hue = ((atan2(dy, dx) / (2.0 * 3.141592653589793)) + 1.0) % 1.0;
        let sat = clamp(dist / r, 0.0, 1.0);
        let rgb = hsv2rgb(hue, sat, 1.0);

        let aa: f32 = 1.0;
        let d = dist - r; // positive outside
        let alpha = 1.0 - smoothstep(0.0, aa, d);
        return vec4<f32>(rgb, alpha);
    }

    // non-wheel cases may still want a hue/sat gradient; compute
    // a "base" colour that will later be blended with corner alpha.
    var base_rgb: vec3<f32> = in.color.rgb;
    var diag_alpha: f32 = 1.0; // additional mask for triangle diagonal
    if (in.flags == 2u) {
        // rectangular swatch: hue varies X, saturation varies inverse Y
        let hue = in.uv.x;
        let sat = 1.0 - in.uv.y;
        base_rgb = hsv2rgb(hue, sat, 1.0);
    } else if (in.flags == 3u) {
        // triangular picker: only lower-left right triangle is valid
        let nx = in.uv.x;
        let ny = in.uv.y;
        let sat = 1.0 - ny;
        let hue = select(0.0, nx / (1.0 - ny), sat != 0.0);
        base_rgb = hsv2rgb(hue, sat, 1.0);
        // compute anti-aliasing for diagonal border
        let aa_diag: f32 = 1.414 / in.size.x;
        let ddiag = nx + ny - 1.0;
        diag_alpha = 1.0 - smoothstep(0.0, aa_diag, ddiag);
    }

    // rounded rectangle mode
    let px = in.uv.x * in.size.x;
    let py = in.uv.y * in.size.y;
    let r_tl = in.radii.x;
    let r_tr = in.radii.y;
    let r_bl = in.radii.z;
    let r_br = in.radii.w;

    // hard-clip corners (alpha may already be 0 from triangle logic)
    if (r_tl > 0.0 && px < r_tl && py < r_tl) {
        let dx = r_tl - px;
        let dy = r_tl - py;
        if (dx * dx + dy * dy > r_tl * r_tl) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    if (r_tr > 0.0 && px > (in.size.x - r_tr) && py < r_tr) {
        let dx = px - (in.size.x - r_tr);
        let dy = r_tr - py;
        if (dx * dx + dy * dy > r_tr * r_tr) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    if (r_bl > 0.0 && px < r_bl && py > (in.size.y - r_bl)) {
        let dx = r_bl - px;
        let dy = py - (in.size.y - r_bl);
        if (dx * dx + dy * dy > r_bl * r_bl) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }
    if (r_br > 0.0 && px > (in.size.x - r_br) && py > (in.size.y - r_br)) {
        let dx = px - (in.size.x - r_br);
        let dy = py - (in.size.y - r_br);
        if (dx * dx + dy * dy > r_br * r_br) {
            return vec4<f32>(in.color.rgb, 0.0);
        }
    }

    // smooth corners
    var dist: f32 = 0.0;
    if (px < r_tl && py < r_tl) {
        dist = length(vec2<f32>(px - r_tl, py - r_tl)) - r_tl;
    } else if (px > in.size.x - r_tr && py < r_tr) {
        dist = length(vec2<f32>(px - (in.size.x - r_tr), py - r_tr)) - r_tr;
    } else if (px < r_bl && py > in.size.y - r_bl) {
        dist = length(vec2<f32>(px - r_bl, py - (in.size.y - r_bl))) - r_bl;
    } else if (px > in.size.x - r_br && py > in.size.y - r_br) {
        dist = length(vec2<f32>(px - (in.size.x - r_br), py - (in.size.y - r_br))) - r_br;
    }
    let aa_corner: f32 = 1.0;
    var alpha = in.color.a * (1.0 - smoothstep(0.0, aa_corner, dist));
    if (in.flags == 3u) {
        alpha = alpha * diag_alpha;
    }
    return vec4<f32>(base_rgb, alpha);
}
