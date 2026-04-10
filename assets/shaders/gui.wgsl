// ─── GUI Quad Shader ──────────────────────────────────────────────────────────
//
// Renderiza quads de UI instanciados. Las coordenadas se pasan en píxeles
// y se convierten a clip space mediante la resolución de la pantalla.
//
// Atributos de instancia (GuiQuad, 96 bytes):
//   location 1: i_pos      [f32; 2]  offset   0
//   location 2: i_size     [f32; 2]  offset   8
//   location 3: i_uv0      [f32; 2]  offset  16   (también: border_width, blur_radius)
//   location 4: i_uv1      [f32; 2]  offset  24   (también: shadow offsets)
//   location 5: i_color    [f32; 4]  offset  32   (color primario / inicio de gradiente)
//   location 6: i_color_b  [f32; 4]  offset  48   (color fin de gradiente / color sombra)
//   location 7: i_radii    [f32; 4]  offset  64
//   location 8: i_tex_index u32      offset  80
//   location 9: i_flags    u32       offset  84
//   location10: i_z_order  f32       offset  88
//
// Flags (bitfield):
//   bit 0 (0x01): COLOR_WHEEL_BIT     — rueda HSV del color picker
//   bit 1 (0x02): TEXTURED_BIT        — muestrear gui_textures[tex_index]
//   bit 2 (0x04): GRADIENT_BIT        — gradiente 2-color GPU (color → color_b)
//   bit 3 (0x08): GRADIENT_STRIP_BIT  — tira de gradiente legacy
//   bit 4 (0x10): GRADIENT_V_BIT      — dirección gradiente: vertical (combinado con GRADIENT_BIT)
//   bit 5 (0x20): GRADIENT_RADIAL_BIT — gradiente radial (combinado con GRADIENT_BIT)
//   bit 6 (0x40): BORDER_BIT          — borde (outline) sin relleno; uv0.x = border_width
//   bit 7 (0x80): SHADOW_BIT          — sombra suavizada (box-shadow aproximado)

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color:     vec4<f32>,   // color primario
    @location(1) color_b:   vec4<f32>,   // color secundario (gradiente, sombra)
    @location(2) radii:     vec4<f32>,
    @location(3) uv:        vec2<f32>,   // UV interpolado (para texturas)
    @location(4) uv0:       vec2<f32>,   // uv0 datos extra (border_width, blur)
    @location(5) uv1:       vec2<f32>,   // uv1 datos extra (shadow offset)
    @location(6) size:      vec2<f32>,   // tamaño del quad en píxeles
    @location(7) @interpolate(flat) tex_index: u32,
    @location(8) @interpolate(flat) flags:     u32,
    @location(9) raw_uv:    vec2<f32>,   // UV en [0,1]×[0,1] del quad completo
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

const MAX_TEXTURE_SLOTS: u32 = 16u;

@group(1) @binding(0)
var gui_textures: binding_array<texture_2d<f32>, MAX_TEXTURE_SLOTS>;
@group(1) @binding(1)
var gui_samplers: binding_array<sampler, MAX_TEXTURE_SLOTS>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(vec3<f32>(h) + K.xyz) * 6.0 - K.www);
    return v * mix(K.xxx, clamp(p - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

/// SDF de rectángulo redondeado con 4 radios independientes.
/// p = posición relativa al centro del quad (puede ser negativa).
/// b = semitamaño del quad (half-size).
/// r = [top-left, top-right, bottom-left, bottom-right]
fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    // Elegir radio según cuadrante
    var radius: f32;
    if p.x <= 0.0 && p.y <= 0.0 { radius = r.x; }         // TL
    else if p.x > 0.0 && p.y <= 0.0 { radius = r.y; }     // TR
    else if p.x <= 0.0 && p.y > 0.0 { radius = r.z; }     // BL
    else { radius = r.w; }                                   // BR
    let q = abs(p) - b + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

/// Aproximación gaussiana de box-shadow usando SDF exponencial.
fn shadow_alpha(sdf_dist: f32, blur: f32) -> f32 {
    if blur < 0.001 { return select(0.0, 1.0, sdf_dist < 0.0); }
    return clamp(exp(-sdf_dist / blur) * 0.5, 0.0, 1.0);
}

// ─── Vertex Stage ─────────────────────────────────────────────────────────────

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
    out.clip_pos  = vec4<f32>(ndc, 1.0 - i_z_order, 1.0);  // z = profundidad normalizada
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

// ─── Fragment Stage ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.raw_uv.x * in.size.x;
    let py = in.raw_uv.y * in.size.y;
    let half_size = in.size * 0.5;
    // Coordenadas relativas al centro del quad (para SDF)
    let p_center = vec2<f32>(px - half_size.x, py - half_size.y);

    // ── Pre-compute SDFs y fwidth ANTES de cualquier branch no-uniforme ───────
    // WebGPU exige que fwidth() se llame solo desde flujo de control uniforme.
    // Calculamos todos los valores SDF y sus derivadas aquí para que el
    // compilador los vea siempre alcanzables, sin importar el valor de flags.
    let border_w_early  = in.uv0.x;
    let inner_b_early   = half_size - vec2<f32>(border_w_early);
    let outer_sdf_early = sdf_rounded_rect(p_center, half_size, in.radii);
    let inner_sdf_early = sdf_rounded_rect(p_center, max(inner_b_early, vec2<f32>(0.0)), in.radii);
    let aa_border       = fwidth(outer_sdf_early);   // uniforme: calculado antes de cualquier branch
    let sdf_main        = sdf_rounded_rect(p_center, half_size, in.radii);
    let aa_main         = max(fwidth(sdf_main), 0.5); // uniforme: calculado antes de cualquier branch

    // ── COLOR WHEEL (flag 0x01) ───────────────────────────────────────────────
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

    // ── SHADOW (flag 0x80) ────────────────────────────────────────────────────
    if (in.flags & 0x80u) != 0u {
        let blur      = in.uv0.x;
        // color_b.xy encodes original rect size for SDF computation
        let orig_size = in.color_b.xy;
        let sdf = sdf_rounded_rect(p_center, orig_size * 0.5, in.radii);
        let a   = shadow_alpha(sdf, blur) * in.color.a;
        return vec4<f32>(in.color.rgb, a);
    }

    // ── COLOR BASE (solid, gradient o textura) ────────────────────────────────
    var base_color = in.color;

    // GRADIENT_BIT (0x04): interpolación GPU de 2 colores
    if (in.flags & 0x04u) != 0u {
        var t: f32;
        if (in.flags & 0x20u) != 0u {
            // GRADIENT_RADIAL_BIT: distancia normalizada desde el centro
            t = clamp(length(p_center) / min(half_size.x, half_size.y), 0.0, 1.0);
        } else if (in.flags & 0x10u) != 0u {
            // GRADIENT_V_BIT: top → bottom
            t = in.raw_uv.y;
        } else {
            // Horizontal por defecto: left → right
            t = in.raw_uv.x;
        }
        base_color = mix(in.color, in.color_b, t);
    }

    // GRADIENT_STRIP_BIT (0x08): tira delgada de gradiente legacy
    if (in.flags & 0x08u) != 0u {
        // uv0.x = normalised left-edge; uv1 = (full_w, full_h)
        let full_w  = in.uv1.x;
        let full_h  = in.uv1.y;
        let px_full = in.uv0.x * full_w + px;
        let py_full = py;
        let t = clamp(px_full / full_w, 0.0, 1.0);
        base_color = mix(in.color, in.color_b, t);
    }

    // ── BORDER OUTLINE (flag 0x40) ────────────────────────────────────────────
    // Usa los SDFs y fwidth pre-calculados desde flujo uniforme arriba.
    if (in.flags & 0x40u) != 0u {
        let outer_alpha  = 1.0 - smoothstep(-aa_border, aa_border, outer_sdf_early);
        let inner_alpha  = 1.0 - smoothstep(-aa_border, aa_border, inner_sdf_early);
        let border_alpha = outer_alpha * (1.0 - inner_alpha);
        return vec4<f32>(base_color.rgb, base_color.a * border_alpha);
    }

    // ── SDF de esquinas redondeadas (todos los demás paths) ───────────────────
    // Usa sdf_main y aa_main pre-calculados desde flujo uniforme arriba.
    var alpha = base_color.a * (1.0 - smoothstep(-aa_main, aa_main, sdf_main));

    // ── TEXTURED (flag 0x02) ──────────────────────────────────────────────────
    if (in.flags & 0x02u) != 0u {
        let tex    = gui_textures[in.tex_index];
        let samp   = gui_samplers[in.tex_index];
        let texel  = textureSample(tex, samp, in.uv);
        return texel * vec4<f32>(base_color.rgb, alpha);
    }

    return vec4<f32>(base_color.rgb, alpha);
}
