// ─── GUI Quad Shader (Web Compatibility Version) ───────────────────────────
//
// This version uses separate texture bindings instead of binding_array
// to ensure compatibility with all WebGPU implementations.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color:     vec4<f32>,
    @location(1) color_b:   vec4<f32>,
    @location(2) radii:     vec4<f32>,
    @location(3) uv:        vec2<f32>,
    @location(4) uv0:       vec2<f32>,
    @location(5) uv1:       vec2<f32>,
    @location(6) size:      vec2<f32>,
    @location(7) @interpolate(flat) tex_index: u32,
    @location(8) @interpolate(flat) flags:     u32,
    @location(9) raw_uv:    vec2<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0) var tex0: texture_2d<f32>;
@group(1) @binding(1) var samp0: sampler;
@group(1) @binding(2) var tex1: texture_2d<f32>;
@group(1) @binding(3) var samp1: sampler;
@group(1) @binding(4) var tex2: texture_2d<f32>;
@group(1) @binding(5) var samp2: sampler;
@group(1) @binding(6) var tex3: texture_2d<f32>;
@group(1) @binding(7) var samp3: sampler;
// Para simplificar en web, soportamos hasta 8 slots por ahora (suficiente para Font + 7 assets)
// El backend de ferrous_web puede ajustarse.
@group(1) @binding(8) var tex4: texture_2d<f32>;
@group(1) @binding(9) var samp4: sampler;
@group(1) @binding(10) var tex5: texture_2d<f32>;
@group(1) @binding(11) var samp5: sampler;
@group(1) @binding(12) var tex6: texture_2d<f32>;
@group(1) @binding(13) var samp6: sampler;
@group(1) @binding(14) var tex7: texture_2d<f32>;
@group(1) @binding(15) var samp7: sampler;

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(vec3<f32>(h) + K.xyz) * 6.0 - K.www);
    return v * mix(K.xxx, clamp(p - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var radius: f32;
    if p.x <= 0.0 && p.y <= 0.0 { radius = r.x; }
    else if p.x > 0.0 && p.y <= 0.0 { radius = r.y; }
    else if p.x <= 0.0 && p.y > 0.0 { radius = r.z; }
    else { radius = r.w; }
    let q = abs(p) - b + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

fn shadow_alpha(sdf_dist: f32, blur: f32) -> f32 {
    if blur < 0.001 { return select(0.0, 1.0, sdf_dist < 0.0); }
    return clamp(exp(-sdf_dist / blur) * 0.5, 0.0, 1.0);
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
) -> VsOut {
    var pixel = i_pos + uv * i_size;
    var ndc   = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.clip_pos  = vec4<f32>(ndc, 1.0 - i_z_order, 1.0);
    out.color     = i_color;
    out.color_b   = i_color_b;
    out.radii     = i_radii;
    out.uv        = i_uv0 + uv * (i_uv1 - i_uv0);
    out.uv0       = i_uv0;
    out.uv1       = i_uv1;
    out.size      = i_size;
    out.tex_index = i_tex_index;
    out.flags     = i_flags;
    out.raw_uv    = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.raw_uv.x * in.size.x;
    let py = in.raw_uv.y * in.size.y;
    let half_size = in.size * 0.5;
    let p_center = vec2<f32>(px - half_size.x, py - half_size.y);

    // ── Pre-compute SDFs and fwidth BEFORE any non-uniform branching ──────────
    // WebGPU requires fwidth() to be called from uniform control flow only.
    // We eagerly compute all SDF values and their derivatives here so the
    // compiler sees them as uniformly reachable, regardless of flag values.
    let border_w_early  = in.uv0.x;
    let inner_b_early   = half_size - vec2<f32>(border_w_early);
    let outer_sdf_early = sdf_rounded_rect(p_center, half_size, in.radii);
    let inner_sdf_early = sdf_rounded_rect(p_center, max(inner_b_early, vec2<f32>(0.0)), in.radii);
    let aa_border       = fwidth(outer_sdf_early);   // uniform: computed before any branch
    let sdf_main        = sdf_rounded_rect(p_center, half_size, in.radii);
    let aa_main         = max(fwidth(sdf_main), 0.5); // uniform: computed before any branch

    // ── COLOR WHEEL (flag 0x01) ────────────────────────────────────────────────
    if (in.flags & 0x01u) != 0u {
        let r = min(half_size.x, half_size.y);
        let dist = length(p_center);
        let hue  = ((atan2(p_center.y, p_center.x) / (2.0 * 3.141592653589793)) + 1.0) % 1.0;
        let sat  = clamp(dist / r, 0.0, 1.0);
        let rgb  = hsv2rgb(hue, sat, 1.0);
        let d    = dist - r;
        let alpha = 1.0 - smoothstep(0.0, 1.0, d);
        return vec4<f32>(rgb, alpha);
    }

    // ── SHADOW (flag 0x80) ─────────────────────────────────────────────────────
    if (in.flags & 0x80u) != 0u {
        let blur      = in.uv0.x;
        let orig_size = in.color_b.xy;
        let sdf = sdf_rounded_rect(p_center, orig_size * 0.5, in.radii);
        let a   = shadow_alpha(sdf, blur) * in.color.a;
        return vec4<f32>(in.color.rgb, a);
    }

    // ── COLOR BASE (solid o gradiente) ─────────────────────────────────────────
    var base_color = in.color;
    if (in.flags & 0x04u) != 0u {
        var t: f32;
        if (in.flags & 0x20u) != 0u {
            t = clamp(length(p_center) / min(half_size.x, half_size.y), 0.0, 1.0);
        } else if (in.flags & 0x10u) != 0u {
            t = in.raw_uv.y;
        } else {
            t = in.raw_uv.x;
        }
        base_color = mix(in.color, in.color_b, t);
    }

    // ── BORDER OUTLINE (flag 0x40) ─────────────────────────────────────────────
    // Uses pre-computed SDFs and fwidth from uniform control flow above.
    if (in.flags & 0x40u) != 0u {
        let outer_alpha  = 1.0 - smoothstep(-aa_border, aa_border, outer_sdf_early);
        let inner_alpha  = 1.0 - smoothstep(-aa_border, aa_border, inner_sdf_early);
        let border_alpha = outer_alpha * (1.0 - inner_alpha);
        return vec4<f32>(base_color.rgb, base_color.a * border_alpha);
    }

    // ── SDF rounded corners (all other paths) ──────────────────────────────────
    // Uses pre-computed sdf_main and aa_main from uniform control flow above.
    var alpha = base_color.a * (1.0 - smoothstep(-aa_main, aa_main, sdf_main));

    // ── TEXTURED (flag 0x02) ───────────────────────────────────────────────────
    if ((in.flags & 0x02u) != 0u) {
        var texel: vec4<f32>;
        switch(in.tex_index) {
            case 0u: { texel = textureSampleLevel(tex0, samp0, in.uv, 0.0); }
            case 1u: { texel = textureSampleLevel(tex1, samp1, in.uv, 0.0); }
            case 2u: { texel = textureSampleLevel(tex2, samp2, in.uv, 0.0); }
            case 3u: { texel = textureSampleLevel(tex3, samp3, in.uv, 0.0); }
            case 4u: { texel = textureSampleLevel(tex4, samp4, in.uv, 0.0); }
            case 5u: { texel = textureSampleLevel(tex5, samp5, in.uv, 0.0); }
            case 6u: { texel = textureSampleLevel(tex6, samp6, in.uv, 0.0); }
            case 7u: { texel = textureSampleLevel(tex7, samp7, in.uv, 0.0); }
            default: { texel = vec4<f32>(1.0, 0.0, 1.0, 1.0); }
        }
        return texel * vec4<f32>(base_color.rgb, alpha);
    }

    return vec4<f32>(base_color.rgb, alpha);
}
