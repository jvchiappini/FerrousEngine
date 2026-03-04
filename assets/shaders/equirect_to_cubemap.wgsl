// Converts an equirectangular HDR texture to a cube map.
// Each invocation writes one texel of one face.  Dispatch dimensions should
// be (cubemapSize/8, cubemapSize/8, 6) so that `@builtin(global_invocation_id).z`
// selects the cube face.

const PI: f32 = 3.14159265359;

@group(0) @binding(0)
var hdr_map: texture_2d<f32>;
@group(0) @binding(1)
var hdr_sampler: sampler;
// storage texture is a 2d array with six layers representing the cube faces
@group(0) @binding(2)
var out_cube: texture_storage_2d_array<rgba16float, write>;

// convert 2D UV [0,1] and face index to a direction vector
fn texel_dir(face: i32, uv: vec2<f32>) -> vec3<f32> {
    let a = 2.0 * uv - vec2<f32>(1.0);
    switch(face) {
        // +X
        case 0: { return normalize(vec3<f32>( 1.0,    -a.y,   -a.x)); }
        // -X
        case 1: { return normalize(vec3<f32>(-1.0,    -a.y,    a.x)); }
        // +Y
        case 2: { return normalize(vec3<f32>( a.x,     1.0,    a.y)); }
        // -Y
        case 3: { return normalize(vec3<f32>( a.x,    -1.0,   -a.y)); }
        // +Z
        case 4: { return normalize(vec3<f32>( a.x,    -a.y,    1.0)); }
        // -Z
        default: { return normalize(vec3<f32>(-a.x,   -a.y,   -1.0)); }
    }
}

// Converts an equirectangular HDR texture to a cube map.
// Each invocation writes one texel of one face.  Dispatch dimensions should
// be (cubemapSize/8, cubemapSize/8, 6) so that `@builtin(global_invocation_id).z`
// selects the cube face.

// full equirect->cube conversion using only textureLoad so that we avoid
// `textureSample` on a 2D texture, which is illegal in the compute stage
// on some drivers.  We perform a manual bilinear filter for smoothness.

// hard-coded cube face resolution; must match `env_size` in Rust
const CUBE_SIZE: f32 = 1024.0;

@compute @workgroup_size(8,8,1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // compute cube face direction as before
    let face = i32(gid.z);
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(CUBE_SIZE, CUBE_SIZE);
    let dir = texel_dir(face, uv);

    // convert to equirectangular UV coordinates
    let theta = atan2(dir.z, dir.x);
    let phi = asin(clamp(dir.y, -1.0, 1.0));
    let u = (theta / (2.0 * PI)) + 0.5;
    let v = (phi / PI) + 0.5;

    // fetch dimensions of the HDR map and sample via integer loads
    let dims2 = textureDimensions(hdr_map);
    let tex_pos = vec2<f32>(u * f32(dims2.x), v * f32(dims2.y)) - vec2<f32>(0.5);
    let i0 = vec2<i32>(tex_pos);
    let i1 = i0 + vec2<i32>(1, 1);
    let f = fract(tex_pos);
    let c00 = textureLoad(hdr_map, clamp(i0, vec2<i32>(0), vec2<i32>(dims2.xy) - vec2<i32>(1)), 0);
    let c10 = textureLoad(hdr_map, clamp(vec2<i32>(i1.x, i0.y), vec2<i32>(0), vec2<i32>(dims2.xy) - vec2<i32>(1)), 0);
    let c01 = textureLoad(hdr_map, clamp(vec2<i32>(i0.x, i1.y), vec2<i32>(0), vec2<i32>(dims2.xy) - vec2<i32>(1)), 0);
    let c11 = textureLoad(hdr_map, clamp(i1, vec2<i32>(0), vec2<i32>(dims2.xy) - vec2<i32>(1)), 0);
    // bilinear interpolation
    let colx0 = mix(c00, c10, f.x);
    let colx1 = mix(c01, c11, f.x);
    let color = mix(colx0, colx1, f.y);

    textureStore(out_cube, vec2<i32>(i32(gid.x), i32(gid.y)), face, color);
}