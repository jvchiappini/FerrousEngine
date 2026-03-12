// Shader para renderizar quads de UI. Las coordenadas se pasan en píxeles
// y se convierten a clip space mediante la resolución de la pantalla.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    // size in pixels, required for correct corner SDF
    @location(5) size: vec2<f32>,
    // texture slot index (flat so it isn't interpolated)
    @location(6) @interpolate(flat) tex_index: u32,
    // WebGPU requires @interpolate(flat) on integer vertex outputs
    @location(7) @interpolate(flat) flags: u32,
    // raw vertex UV in [0,1]x[0,1] — always correct for SDF pixel coords
    @location(8) raw_uv: vec2<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// array of textures/samplers for image quads; the slot count must match
// `MAX_TEXTURE_SLOTS` in the Rust code (currently 8).  keep the arrays
// sufficiently small to avoid excessive bind group size.
const MAX_TEXTURE_SLOTS: u32 = 8u;

@group(1) @binding(0)
var gui_textures: binding_array<texture_2d<f32>, MAX_TEXTURE_SLOTS>;
@group(1) @binding(1)
var gui_samplers: binding_array<sampler, MAX_TEXTURE_SLOTS>;

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
    @location(3) i_uv0: vec2<f32>,
    @location(4) i_uv1: vec2<f32>,
    @location(5) i_color: vec4<f32>,
    @location(6) i_radii: vec4<f32>,
    @location(7) i_tex_index: u32,
    @location(8) i_flags: u32,
) -> VsOut {
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    ndc.y = -ndc.y;
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.color = i_color;
    out.radii = i_radii;
    // compute interpolated texture coordinate; if the quad is not
    // textured the uv0/uv1 values will default to 0..1 and behaviour
    // matches the previous non-textured path.
    out.uv = i_uv0 + uv * (i_uv1 - i_uv0);
    out.uv0 = i_uv0;
    out.uv1 = i_uv1;
    out.size = i_size;
    out.tex_index = i_tex_index;
    out.flags = i_flags;
    out.raw_uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // wheel mode: render hue/saturation gradient
    if (in.flags == 1u) {
        let px = in.raw_uv.x * in.size.x;
        let py = in.raw_uv.y * in.size.y;
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

    // rounded rectangle mode — use raw_uv (always 0..1) for pixel coords
    let px = in.raw_uv.x * in.size.x;
    let py = in.raw_uv.y * in.size.y;

    // GRADIENT_BIT (0x4): single full-rect quad.
    //   uv0 = color1.rg, uv1 = color1.ba  — blend left→right by raw_uv.x.
    let gradient_bit: u32 = 4u;
    let is_gradient = (in.flags & gradient_bit) != 0u;
    var gradient_alpha: f32 = in.color.a;
    if is_gradient {
        let color1 = vec4<f32>(in.uv0.x, in.uv0.y, in.uv1.x, in.uv1.y);
        base_rgb = mix(in.color.rgb, color1.rgb, in.raw_uv.x);
        gradient_alpha = mix(in.color.a, color1.a, in.raw_uv.x);
    }

    // GRADIENT_STRIP_BIT (0x8): thin strip of a larger rect (radial/conic).
    //   uv0.x = normalised left-edge X of this strip, uv1 = (full_w, full_h).
    let gradient_strip_bit: u32 = 8u;
    let is_strip = (in.flags & gradient_strip_bit) != 0u;
    let full_w  = select(in.size.x, in.uv1.x, is_strip);
    let full_h  = select(in.size.y, in.uv1.y, is_strip);
    let px_full = select(px, in.uv0.x * full_w + px, is_strip);
    let py_full = py;

    let r_tl = in.radii.x;
    let r_tr = in.radii.y;
    let r_bl = in.radii.z;
    let r_br = in.radii.w;

    // hard-clip corners (alpha may already be 0 from triangle logic)
    if (r_tl > 0.0 && px_full < r_tl && py_full < r_tl) {
        let dx = r_tl - px_full;
        let dy = r_tl - py_full;
        if (dx * dx + dy * dy > r_tl * r_tl) {
            return vec4<f32>(base_rgb, 0.0);
        }
    }
    if (r_tr > 0.0 && px_full > (full_w - r_tr) && py_full < r_tr) {
        let dx = px_full - (full_w - r_tr);
        let dy = r_tr - py_full;
        if (dx * dx + dy * dy > r_tr * r_tr) {
            return vec4<f32>(base_rgb, 0.0);
        }
    }
    if (r_bl > 0.0 && px_full < r_bl && py_full > (full_h - r_bl)) {
        let dx = r_bl - px_full;
        let dy = py_full - (full_h - r_bl);
        if (dx * dx + dy * dy > r_bl * r_bl) {
            return vec4<f32>(base_rgb, 0.0);
        }
    }
    if (r_br > 0.0 && px_full > (full_w - r_br) && py_full > (full_h - r_br)) {
        let dx = px_full - (full_w - r_br);
        let dy = py_full - (full_h - r_br);
        if (dx * dx + dy * dy > r_br * r_br) {
            return vec4<f32>(base_rgb, 0.0);
        }
    }

    // smooth corners
    var dist: f32 = 0.0;
    if (px_full < r_tl && py_full < r_tl) {
        dist = length(vec2<f32>(px_full - r_tl, py_full - r_tl)) - r_tl;
    } else if (px_full > full_w - r_tr && py_full < r_tr) {
        dist = length(vec2<f32>(px_full - (full_w - r_tr), py_full - r_tr)) - r_tr;
    } else if (px_full < r_bl && py_full > full_h - r_bl) {
        dist = length(vec2<f32>(px_full - r_bl, py_full - (full_h - r_bl))) - r_bl;
    } else if (px_full > full_w - r_br && py_full > full_h - r_br) {
        dist = length(vec2<f32>(px_full - (full_w - r_br), py_full - (full_h - r_br))) - r_br;
    }
    let aa_corner: f32 = 1.0;
    var alpha = gradient_alpha * (1.0 - smoothstep(0.0, aa_corner, dist));
    if (in.flags == 3u) {
        alpha = alpha * diag_alpha;
    }
    // if the textured bit is set we sample and tint; otherwise use the
    // flat colour path that was previously implemented.
    if ((in.flags & 0x2u) != 0u) {
        // safe because tex_index is guaranteed < MAX_TEXTURE_SLOTS by the
        // Rust code that constructs the batch
        let tex = gui_textures[in.tex_index];
        let samp = gui_samplers[in.tex_index];
        let texel = textureSample(tex, samp, in.uv);
        return texel * vec4<f32>(base_rgb, alpha);
    }
    return vec4<f32>(base_rgb, alpha);
}
