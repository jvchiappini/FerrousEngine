// ============================================================================
//  antialiasing.wgsl — Ferrous Engine Post-Process Anti-Aliasing Shaders
//
//  Provides selectable antialiasing entry-points:
//   · fs_passthrough  — AA disabled (direct copy, no extra GPU load)
//   · fs_fxaa         — NVIDIA FXAA 3.11 quality (fast, works on every GPU)
//   · fs_smaa_edge    — SMAA 1x edge-detection (first sub-pass)
//   · fs_smaa_blend   — SMAA 1x blending-weight pass (second sub-pass)
//   · fs_smaa_final   — SMAA 1x neighborhood blending (third sub-pass)
//
//  All passes share a single vertex shader that synthesises a fullscreen
//  triangle from vertex_index (no vertex buffer).
// ============================================================================

// ---------------------------------------------------------------------------
//  Module-level constants (must be at top-level in WGSL)
// ---------------------------------------------------------------------------
const FXAA_SEARCH_STEPS        : i32 = 12;    // tap count along edge (quality)
const SMAA_THRESHOLD           : f32 = 0.05;  // more sensitive edge detection
const SMAA_MAX_SEARCH_STEPS    : i32 = 32;    // longer search range for smoother long lines
const SMAA_CORNER_ROUNDING     : f32 = 25.0;  // corner rounding in [0-100]

// ---------------------------------------------------------------------------
//  Bind group 0  — input texture + optional auxiliary texture
// ---------------------------------------------------------------------------
@group(0) @binding(0) var t_color : texture_2d<f32>;
@group(0) @binding(1) var s_color : sampler;
// Used by SMAA sub-passes that need a second input (edge / blend-weight tex).
@group(0) @binding(2) var t_aux   : texture_2d<f32>;
@group(0) @binding(3) var s_aux   : sampler;

// ---------------------------------------------------------------------------
//  Bind group 1  — per-frame uniform (resolution + FXAA quality params)
// ---------------------------------------------------------------------------
struct AaParams {
    resolution_x            : f32,
    resolution_y            : f32,
    fxaa_edge_threshold     : f32,   // default 0.0312  — minimum edge contrast
    fxaa_edge_threshold_min : f32,   // default 0.0833  — low-luma cutoff
    fxaa_subpix             : f32,   // default 0.75    — sub-pixel quality
    _pad0 : f32,
    _pad1 : f32,
    _pad2 : f32,
};
@group(1) @binding(0) var<uniform> aa : AaParams;

// ---------------------------------------------------------------------------
//  Shared vertex shader (fullscreen triangle)
// ---------------------------------------------------------------------------
struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0)       uv  : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out : VsOut;
    let x = f32(i32(vi & 1u) << 2u) - 1.0;
    let y = f32(i32(vi & 2u) << 1u) - 1.0;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv  = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// ---------------------------------------------------------------------------
//  Luminance helper (ITU-R BT.601)
// ---------------------------------------------------------------------------
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// ===========================================================================
//  PASS: Passthrough — AA disabled
// ===========================================================================
@fragment
fn fs_passthrough(in: VsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(t_color, s_color, in.uv, 0.0);
}

// ===========================================================================
//  PASS: FXAA 3.11 Quality
//
//  Reference: Timothy Lottes, "FXAA 3.11", NVIDIA (2012)
// ===========================================================================
fn fxaa_sample(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(t_color, s_color, uv, 0.0).rgb;
}

fn subpix_quality(delta: f32, strength: f32) -> f32 {
    return min(1.0, delta * strength * 3.0);
}

@fragment
fn fs_fxaa(in: VsOut) -> @location(0) vec4<f32> {
    let inv = vec2<f32>(1.0 / aa.resolution_x, 1.0 / aa.resolution_y);

    // ── Center + 4 cardinal neighbours ──────────────────────────────────────
    let rgbM = fxaa_sample(in.uv);
    let rgbN = fxaa_sample(in.uv + vec2<f32>( 0.0,      -inv.y));
    let rgbS = fxaa_sample(in.uv + vec2<f32>( 0.0,       inv.y));
    let rgbW = fxaa_sample(in.uv + vec2<f32>(-inv.x,     0.0));
    let rgbE = fxaa_sample(in.uv + vec2<f32>( inv.x,     0.0));

    let lumaM = luma(rgbM);
    let lumaN = luma(rgbN);
    let lumaS = luma(rgbS);
    let lumaW = luma(rgbW);
    let lumaE = luma(rgbE);

    let lumaMin   = min(lumaM, min(min(lumaN, lumaS), min(lumaW, lumaE)));
    let lumaMax   = max(lumaM, max(max(lumaN, lumaS), max(lumaW, lumaE)));
    let lumaRange = lumaMax - lumaMin;

    // ── Early exit: flat region ──────────────────────────────────────────────
    let et  = aa.fxaa_edge_threshold;
    let etm = aa.fxaa_edge_threshold_min;
    if lumaRange < max(etm, lumaMax * et) {
        return vec4<f32>(rgbM, 1.0);
    }

    // ── Diagonal neighbours ──────────────────────────────────────────────────
    let rgbNW = fxaa_sample(in.uv + vec2<f32>(-inv.x, -inv.y));
    let rgbNE = fxaa_sample(in.uv + vec2<f32>( inv.x, -inv.y));
    let rgbSW = fxaa_sample(in.uv + vec2<f32>(-inv.x,  inv.y));
    let rgbSE = fxaa_sample(in.uv + vec2<f32>( inv.x,  inv.y));

    let lumaNW = luma(rgbNW);
    let lumaNE = luma(rgbNE);
    let lumaSW = luma(rgbSW);
    let lumaSE = luma(rgbSE);

    // ── Sub-pixel aliasing removal ───────────────────────────────────────────
    let subpix = aa.fxaa_subpix;
    // 3×3 weighted average (cardinals ×2, diagonals ×1)
    let lumaAvg = ((lumaN + lumaS + lumaW + lumaE) * 2.0
                 + (lumaNW + lumaNE + lumaSW + lumaSE)) / 12.0;
    let subpixDelta = abs(lumaAvg - lumaM) / lumaRange;
    let subpixBlend = smoothstep(0.0, 1.0, subpixDelta)
                    * subpixDelta
                    * subpix_quality(subpixDelta, subpix);

    // ── Edge direction ───────────────────────────────────────────────────────
    let edgeH   = abs((lumaNW + 2.0*lumaN + lumaNE) - (lumaSW + 2.0*lumaS + lumaSE));
    let edgeV   = abs((lumaNW + 2.0*lumaW + lumaSW) - (lumaNE + 2.0*lumaE + lumaSE));
    let isHoriz = edgeH >= edgeV;

    // ── Step direction and gradient ──────────────────────────────────────────
    let luma1 = select(lumaS, lumaE, isHoriz);
    let luma2 = select(lumaN, lumaW, isHoriz);
    let grad1  = abs(luma1 - lumaM);
    let grad2  = abs(luma2 - lumaM);
    let isL1   = grad1 >= grad2;
    let gradSc = 0.25 * max(grad1, grad2);

    var stepLen = select(-inv.y, -inv.x, isHoriz);
    if !isL1 { stepLen = -stepLen; }

    var uv1 = in.uv;
    var uv2 = in.uv;
    if isHoriz {
        uv1.y += stepLen * 0.5;
        uv2.y += stepLen * 0.5;
    } else {
        uv1.x += stepLen * 0.5;
        uv2.x += stepLen * 0.5;
    }

    let stepDir    = select(vec2<f32>(0.0, inv.y), vec2<f32>(inv.x, 0.0), isHoriz);
    let lumaEndRef = (luma1 + luma2) * 0.5;

    // ── 12-tap search along edge ─────────────────────────────────────────────
    var reached1 = false;
    var reached2 = false;
    var lumaEnd1 = 0.0;
    var lumaEnd2 = 0.0;
    for (var i = 0; i < FXAA_SEARCH_STEPS; i++) {
        if !reached1 {
            uv1      -= stepDir;
            lumaEnd1  = luma(fxaa_sample(uv1)) - lumaEndRef;
            reached1  = abs(lumaEnd1) >= gradSc;
        }
        if !reached2 {
            uv2      += stepDir;
            lumaEnd2  = luma(fxaa_sample(uv2)) - lumaEndRef;
            reached2  = abs(lumaEnd2) >= gradSc;
        }
        if reached1 && reached2 { break; }
    }

    // ── Blend factor ─────────────────────────────────────────────────────────
    let dist1       = select(in.uv.y - uv1.y, in.uv.x - uv1.x, isHoriz);
    let dist2       = select(uv2.y - in.uv.y, uv2.x - in.uv.x, isHoriz);
    let isD1Closer  = dist1 < dist2;
    let spanLen     = dist1 + dist2;
    let pixOffset   = -select(dist2, dist1, isD1Closer) / spanLen + 0.5;

    let lumaCloser  = select(lumaEnd2, lumaEnd1, isD1Closer);
    let isCorrect   = ((lumaM - lumaEndRef) < 0.0) != (lumaCloser < 0.0);
    let finalOffset = select(0.0, pixOffset, isCorrect);
    let blendOffset = max(finalOffset, subpixBlend);

    var uvFinal = in.uv;
    if isHoriz {
        uvFinal.y += blendOffset * stepLen;
    } else {
        uvFinal.x += blendOffset * stepLen;
    }

    return vec4<f32>(fxaa_sample(uvFinal), 1.0);
}

// ===========================================================================
//  SMAA 1x — Sub-pixel Morphological Anti-Aliasing
//
//  Reference: J.M. Lopez Morales et al., "SMAA: Enhanced Subpixel
//             Morphological Anti-Aliasing", Eurographics 2012.
//
//  Three-pass approach:
//    Pass 1 (fs_smaa_edge)  : luma edge detection → Rgba8Unorm edge map
//    Pass 2 (fs_smaa_blend) : blending weights     → Rgba8Unorm weight map
//    Pass 3 (fs_smaa_final) : neighbourhood blend  → final AA colour (HDR)
// ===========================================================================

// ── Pass 1: Luma Edge Detection ──────────────────────────────────────────────
@fragment
fn fs_smaa_edge(in: VsOut) -> @location(0) vec4<f32> {
    let inv = vec2<f32>(1.0 / aa.resolution_x, 1.0 / aa.resolution_y);

    let lumaC = luma(textureSampleLevel(t_color, s_color, in.uv, 0.0).rgb);
    let lumaN = luma(textureSampleLevel(t_color, s_color, in.uv + vec2<f32>( 0.0,  -inv.y), 0.0).rgb);
    let lumaS = luma(textureSampleLevel(t_color, s_color, in.uv + vec2<f32>( 0.0,   inv.y), 0.0).rgb);
    let lumaW = luma(textureSampleLevel(t_color, s_color, in.uv + vec2<f32>(-inv.x,  0.0), 0.0).rgb);
    let lumaE = luma(textureSampleLevel(t_color, s_color, in.uv + vec2<f32>( inv.x,  0.0), 0.0).rgb);

    var delta : vec2<f32>;
    delta.x = abs(lumaC - lumaW);
    delta.y = abs(lumaC - lumaN);

    var edges = step(vec2<f32>(SMAA_THRESHOLD), delta);
    if dot(edges, vec2<f32>(1.0)) == 0.0 {
        discard;
    }

    // Local contrast adaptation (suppresses false edges in smooth gradients)
    let maxDeltaH  = max(delta.x, abs(lumaC - lumaE));
    let maxDeltaV  = max(delta.y, abs(lumaC - lumaS));
    let lumaNN = luma(textureSampleLevel(t_color, s_color, in.uv + vec2<f32>( 0.0,    -2.0*inv.y), 0.0).rgb);
    let lumaWW = luma(textureSampleLevel(t_color, s_color, in.uv + vec2<f32>(-2.0*inv.x, 0.0), 0.0).rgb);
    edges.x *= step(0.5 * max(maxDeltaH, abs(lumaN - lumaNN)), delta.x);
    edges.y *= step(0.5 * max(maxDeltaV, abs(lumaW - lumaWW)), delta.y);

    return vec4<f32>(edges, 0.0, 1.0);
}

// ── Helpers for Pass 2 ───────────────────────────────────────────────────────
fn smaa_walk_x_left(uv: vec2<f32>, inv: vec2<f32>) -> f32 {
    var p = vec2<f32>(uv.x - inv.x, uv.y);
    for (var i = 0; i < SMAA_MAX_SEARCH_STEPS; i++) {
        if textureSampleLevel(t_color, s_color, p, 0.0).g < 0.9 { break; }
        p.x -= inv.x;
    }
    return max(-f32(SMAA_MAX_SEARCH_STEPS), (p.x - uv.x) / inv.x);
}
fn smaa_walk_x_right(uv: vec2<f32>, inv: vec2<f32>) -> f32 {
    var p = vec2<f32>(uv.x + inv.x, uv.y);
    for (var i = 0; i < SMAA_MAX_SEARCH_STEPS; i++) {
        if textureSampleLevel(t_color, s_color, p, 0.0).g < 0.9 { break; }
        p.x += inv.x;
    }
    return min(f32(SMAA_MAX_SEARCH_STEPS), (p.x - uv.x) / inv.x);
}
fn smaa_walk_y_up(uv: vec2<f32>, inv: vec2<f32>) -> f32 {
    var p = vec2<f32>(uv.x, uv.y - inv.y);
    for (var i = 0; i < SMAA_MAX_SEARCH_STEPS; i++) {
        if textureSampleLevel(t_color, s_color, p, 0.0).r < 0.9 { break; }
        p.y -= inv.y;
    }
    return max(-f32(SMAA_MAX_SEARCH_STEPS), (p.y - uv.y) / inv.y);
}
fn smaa_walk_y_down(uv: vec2<f32>, inv: vec2<f32>) -> f32 {
    var p = vec2<f32>(uv.x, uv.y + inv.y);
    for (var i = 0; i < SMAA_MAX_SEARCH_STEPS; i++) {
        if textureSampleLevel(t_color, s_color, p, 0.0).r < 0.9 { break; }
        p.y += inv.y;
    }
    return min(f32(SMAA_MAX_SEARCH_STEPS), (p.y - uv.y) / inv.y);
}
fn smaa_weight(dist: f32, is_corner: bool) -> f32 {
    var d = dist;
    if is_corner { d *= 1.0 - SMAA_CORNER_ROUNDING / 100.0; }
    return d / f32(SMAA_MAX_SEARCH_STEPS);
}

// ── Pass 2: Blending Weight Calculation ─────────────────────────────────────
// t_color is the edge map produced by Pass 1.
@fragment
fn fs_smaa_blend(in: VsOut) -> @location(0) vec4<f32> {
    let inv  = vec2<f32>(1.0 / aa.resolution_x, 1.0 / aa.resolution_y);
    let edge = textureSampleLevel(t_color, s_color, in.uv, 0.0).rg;
    var wt   = vec4<f32>(0.0);

    // Horizontal edge → blend left / right
    if edge.g > 0.0 {
        let d   = vec2<f32>(smaa_walk_x_left(in.uv, inv), smaa_walk_x_right(in.uv, inv));
        let uvy = in.uv + vec2<f32>(0.0, -inv.y);
        let eL  = textureSampleLevel(t_color, s_color, uvy + vec2<f32>(d.x * inv.x, 0.0), 0.0).r;
        let eR  = textureSampleLevel(t_color, s_color, uvy + vec2<f32>(d.y * inv.x, 0.0), 0.0).r;
        let w   = abs(d.x) + abs(d.y) + 1.0;
        wt.r = smaa_weight(abs(d.x), eL > 0.9) / max(1.0, w);
        wt.g = smaa_weight(abs(d.y), eR > 0.9) / max(1.0, w);
    }

    // Vertical edge → blend up / down
    if edge.r > 0.0 {
        let d   = vec2<f32>(smaa_walk_y_up(in.uv, inv), smaa_walk_y_down(in.uv, inv));
        let uvx = in.uv + vec2<f32>(-inv.x, 0.0);
        let eU  = textureSampleLevel(t_color, s_color, uvx + vec2<f32>(0.0, d.x * inv.y), 0.0).g;
        let eD  = textureSampleLevel(t_color, s_color, uvx + vec2<f32>(0.0, d.y * inv.y), 0.0).g;
        let w   = abs(d.x) + abs(d.y) + 1.0;
        wt.b = smaa_weight(abs(d.x), eU > 0.9) / max(1.0, w);
        wt.a = smaa_weight(abs(d.y), eD > 0.9) / max(1.0, w);
    }

    return wt;
}

// ── Pass 3: Neighbourhood Blending ──────────────────────────────────────────
// t_color = original HDR,  t_aux = blend weights from Pass 2.
@fragment
fn fs_smaa_final(in: VsOut) -> @location(0) vec4<f32> {
    let inv  = vec2<f32>(1.0 / aa.resolution_x, 1.0 / aa.resolution_y);
    let wt   = textureSampleLevel(t_aux, s_aux, in.uv, 0.0);
    var c    = textureSampleLevel(t_color, s_color, in.uv, 0.0);

    if abs(wt.r) + abs(wt.g) > 1e-4 {
        c = mix(c, textureSampleLevel(t_color, s_color, in.uv - vec2<f32>(inv.x, 0.0), 0.0), wt.r);
        c = mix(c, textureSampleLevel(t_color, s_color, in.uv + vec2<f32>(inv.x, 0.0), 0.0), wt.g);
    }
    if abs(wt.b) + abs(wt.a) > 1e-4 {
        c = mix(c, textureSampleLevel(t_color, s_color, in.uv - vec2<f32>(0.0, inv.y), 0.0), wt.b);
        c = mix(c, textureSampleLevel(t_color, s_color, in.uv + vec2<f32>(0.0, inv.y), 0.0), wt.a);
    }

    return c;
}
