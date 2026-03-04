// Importance‑sampled prefilter of an environment cube map.  Each mip level
// should be generated in a separate dispatch; the CPU passes `roughness` as a
// uniform value.  Dispatch shape : (mipSize/8, mipSize/8, 6).

const PI: f32 = 3.14159265359;

struct Params {
    roughness: f32,
};
@group(0) @binding(3)
var<uniform> params: Params;

// we'll sample the cube manually using a 2D-array view to avoid
// `textureSample` in compute.
@group(0) @binding(0)
var env_map: texture_2d_array<f32>;
@group(0) @binding(2)
var out_prefilter: texture_storage_2d_array<rgba16float, write>;

// random utilities (VdC / Hammersley)
fn radicalInverse_VdC(bits: u32) -> f32 {
    var b = bits;
    b = ((b << 16u) | (b >> 16u));
    b = ((b & 0x55555555u) << 1u) | ((b & 0xAAAAAAAAu) >> 1u);
    b = ((b & 0x33333333u) << 2u) | ((b & 0xCCCCCCCCu) >> 2u);
    b = ((b & 0x0F0F0F0Fu) << 4u) | ((b & 0xF0F0F0F0u) >> 4u);
    b = ((b & 0x00FF00FFu) << 8u) | ((b & 0xFF00FF00u) >> 8u);
    return f32(b) * 2.3283064365386963e-10; // / 0x100000000
}

fn hammersley(i: u32, N: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(N), radicalInverse_VdC(i));
}

fn importanceSampleGGX(xi: vec2<f32>, N: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cosTheta = sqrt((1.0 - xi.y) / (1.0 + (a*a - 1.0) * xi.y));
    let sinTheta = sqrt(1.0 - cosTheta * cosTheta);

    // spherical to cartesian
    var H = vec3<f32>(sinTheta * cos(phi), sinTheta * sin(phi), cosTheta);
    // transform H to world space with N as up vector
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(N.z) < 0.999);
    let tangent = normalize(cross(up, N));
    let bitangent = cross(N, tangent);
    let sampleVec = tangent * H.x + bitangent * H.y + N * H.z;
    return normalize(sampleVec);
}

// same texel_dir helper as before
fn texel_dir(face: i32, uv: vec2<f32>) -> vec3<f32> {
    let a = 2.0 * uv - vec2<f32>(1.0);
    switch(face) {
        case 0: { return normalize(vec3<f32>( 1.0,    -a.y,   -a.x)); }
        case 1: { return normalize(vec3<f32>(-1.0,    -a.y,    a.x)); }
        case 2: { return normalize(vec3<f32>( a.x,     1.0,    a.y)); }
        case 3: { return normalize(vec3<f32>( a.x,    -1.0,   -a.y)); }
        case 4: { return normalize(vec3<f32>( a.x,    -a.y,    1.0)); }
        default: { return normalize(vec3<f32>(-a.x,   -a.y,   -1.0)); }
    }
}

@compute @workgroup_size(8,8,1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_prefilter);
    if (gid.x >= dims.x || gid.y >= dims.y) { return; }
    let face = i32(gid.z);
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dims.xy);
    let R = texel_dir(face, uv);
    let N = R;
    let sampleCount: u32 = 1024u;
    var prefiltered = vec3<f32>(0.0);
    var totalWeight = 0.0;
    for (var i: u32 = 0u; i < sampleCount; i = i + 1u) {
        let xi = hammersley(i, sampleCount);
        let H = importanceSampleGGX(xi, N, params.roughness);
        let L = normalize(2.0 * dot(R, H) * H - R);
        let NdotL = max(dot(N, L), 0.0);
        if (NdotL > 0.0) {
            // manual cube sample similar to irradiance shader
            let dims = textureDimensions(env_map);
            let absd = abs(L);
            var face_i: i32;
            var uc: f32;
            var vc: f32;
            var ma: f32;
            if (absd.x >= absd.y && absd.x >= absd.z) {
                if (L.x > 0.0) {
                    face_i = 0;
                    uc = -L.z;
                    vc = -L.y;
                    ma = absd.x;
                } else {
                    face_i = 1;
                    uc = L.z;
                    vc = -L.y;
                    ma = absd.x;
                }
            } else if (absd.y >= absd.x && absd.y >= absd.z) {
                if (L.y > 0.0) {
                    face_i = 2;
                    uc = L.x;
                    vc = L.z;
                    ma = absd.y;
                } else {
                    face_i = 3;
                    uc = L.x;
                    vc = -L.z;
                    ma = absd.y;
                }
            } else {
                if (L.z > 0.0) {
                    face_i = 4;
                    uc = L.x;
                    vc = -L.y;
                    ma = absd.z;
                } else {
                    face_i = 5;
                    uc = -L.x;
                    vc = -L.y;
                    ma = absd.z;
                }
            }
            let u = 0.5 * (uc / ma + 1.0);
            let v = 0.5 * (vc / ma + 1.0);
            let texPos = vec2<i32>(i32(u * f32(dims.x)), i32(v * f32(dims.y)));
            let sampleColor = textureLoad(env_map, texPos, face_i, 0).xyz;
            prefiltered += sampleColor * NdotL;
            totalWeight += NdotL;
        }
    }
    prefiltered = prefiltered / totalWeight;
    textureStore(out_prefilter, vec2<i32>(i32(gid.x), i32(gid.y)), face, vec4<f32>(prefiltered, 1.0));
}