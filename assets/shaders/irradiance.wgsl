// Compute shader that convolves an environment cube map to produce an
// irradiance cubemap.  Output resolution should be small (e.g. 32x32).  We
// perform a simple Monte Carlo integration over the hemisphere oriented at the
// sample direction.  Dispatch dimensions: (outSize/8, outSize/8, 6).

const PI: f32 = 3.14159265359;

// we will sample the cube manually via textureLoad from a 2D array view
@group(0) @binding(0)
var env_map: texture_2d_array<f32>;
@group(0) @binding(2)
var out_irr: texture_storage_2d_array<rgba16float, write>;

// same helper used by eq shader (uv -> direction)
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

// uniformly sample a direction in the hemisphere using simple stratified
fn sample_hemisphere(u: vec2<f32>) -> vec3<f32> {
    let phi = 2.0 * PI * u.x;
    let cos_theta = u.y;
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    return vec3<f32>(sin_theta * cos(phi), cos_theta, sin_theta * sin(phi));
}

@compute @workgroup_size(8,8,1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_irr);
    if (gid.x >= dims.x || gid.y >= dims.y) { return; }
    let face = i32(gid.z);
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dims.xy);
    let N = texel_dir(face, uv);

    // build tangent space basis
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let right = normalize(cross(up, N));
    let forward = cross(N, right);

    var irradiance = vec3<f32>(0.0);
    let SAMPLE_COUNT: u32 = 1024u;
    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        // simple sequence kepler or just use i/N
        let xi = vec2<f32>(fract(f32(i) * 0.618034), fract(f32(i) * 0.754877));
        let hemi = sample_hemisphere(xi);
        // transform to world
        let sample_dir = hemi.x * right + hemi.y * N + hemi.z * forward;
        // sample env_map by converting direction -> face/uv and load texel
        let dims = textureDimensions(env_map);
        // compute approximate face & uv for cube sampling
        let absdir = abs(sample_dir);
        var face_i: i32;
        var uc: f32;
        var vc: f32;
        var ma: f32;
        if (absdir.x >= absdir.y && absdir.x >= absdir.z) {
            if (sample_dir.x > 0.0) {
                face_i = 0;
                uc = -sample_dir.z;
                vc = -sample_dir.y;
                ma = absdir.x;
            } else {
                face_i = 1;
                uc = sample_dir.z;
                vc = -sample_dir.y;
                ma = absdir.x;
            }
        } else if (absdir.y >= absdir.x && absdir.y >= absdir.z) {
            if (sample_dir.y > 0.0) {
                face_i = 2;
                uc = sample_dir.x;
                vc = sample_dir.z;
                ma = absdir.y;
            } else {
                face_i = 3;
                uc = sample_dir.x;
                vc = -sample_dir.z;
                ma = absdir.y;
            }
        } else {
            if (sample_dir.z > 0.0) {
                face_i = 4;
                uc = sample_dir.x;
                vc = -sample_dir.y;
                ma = absdir.z;
            } else {
                face_i = 5;
                uc = -sample_dir.x;
                vc = -sample_dir.y;
                ma = absdir.z;
            }
        }
        let u = 0.5 * (uc / ma + 1.0);
        let v = 0.5 * (vc / ma + 1.0);
        let texPos = vec2<i32>(i32(u * f32(dims.x)), i32(v * f32(dims.y)));
        let color = textureLoad(env_map, texPos, face_i, 0).xyz;
        irradiance += color * hemi.y; // cos(theta)
    }
    irradiance = irradiance * (PI / f32(SAMPLE_COUNT));
    textureStore(out_irr, vec2<i32>(i32(gid.x), i32(gid.y)), face, vec4<f32>(irradiance, 1.0));
}