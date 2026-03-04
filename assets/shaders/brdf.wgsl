// Compute shader that generates a 2D BRDF integration lookup table.
// Output size should be 512x512 and dispatch dims (size/8, size/8, 1).

const PI: f32 = 3.14159265359;

// sample utilities (same as prefilter)
fn radicalInverse_VdC(bits: u32) -> f32 {
    var b = bits;
    b = ((b << 16u) | (b >> 16u));
    b = ((b & 0x55555555u) << 1u) | ((b & 0xAAAAAAAAu) >> 1u);
    b = ((b & 0x33333333u) << 2u) | ((b & 0xCCCCCCCCu) >> 2u);
    b = ((b & 0x0F0F0F0Fu) << 4u) | ((b & 0xF0F0F0F0u) >> 4u);
    b = ((b & 0x00FF00FFu) << 8u) | ((b & 0xFF00FF00u) >> 8u);
    return f32(b) * 2.3283064365386963e-10;
}

fn hammersley(i: u32, N: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(N), radicalInverse_VdC(i));
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let a = roughness;
    let k = (a * a) / 2.0;
    let nom = NdotV;
    let denom = NdotV * (1.0 - k) + k;
    return nom / denom;
}

fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let gv = geometry_schlick_ggx(NdotV, roughness);
    let gl = geometry_schlick_ggx(NdotL, roughness);
    return gv * gl;
}

fn integrateBRDF(NdotV: f32, roughness: f32) -> vec2<f32> {
    var A: f32 = 0.0;
    var B: f32 = 0.0;
    let V = vec3<f32>(sqrt(1.0 - NdotV * NdotV), 0.0, NdotV);
    let sampleCount: u32 = 1024u;
    for (var i: u32 = 0u; i < sampleCount; i = i + 1u) {
        let Xi = hammersley(i, sampleCount);
        // importance sample GGX
        let a = roughness * roughness;
        let phi = 2.0 * PI * Xi.x;
        let cosTheta = sqrt((1.0 - Xi.y) / (1.0 + (a*a - 1.0) * Xi.y));
        let sinTheta = sqrt(1.0 - cosTheta * cosTheta);
        let H = vec3<f32>(sinTheta * cos(phi), sinTheta * sin(phi), cosTheta);
        let L = normalize(2.0 * dot(V, H) * H - V);
        let NdotL = max(L.z, 0.0);
        let NdotH = max(H.z, 0.0);
        let VdotH = max(dot(V, H), 0.0);
        if (NdotL > 0.0) {
            let G = geometry_smith(NdotV, NdotL, roughness);
            let G_Vis = (G * VdotH) / (NdotH * NdotV);
            let Fc = pow(1.0 - VdotH, 5.0);
            A += (1.0 - Fc) * G_Vis;
            B += Fc * G_Vis;
        }
    }
    A = A / f32(sampleCount);
    B = B / f32(sampleCount);
    return vec2<f32>(A, B);
}

@group(0) @binding(0)
var out_tex: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8,8,1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) { return; }
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dims.xy);
    let NdotV = uv.x;
    let roughness = uv.y;
    let result = integrateBRDF(NdotV, roughness);
    textureStore(out_tex, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(result, 0.0, 1.0));
}