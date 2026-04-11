// GPU Skinning Compute Shader

struct Vertex {
    pos    : vec3<f32>,
    normal : vec3<f32>,
    uv     : vec2<f32>,
};

struct Influence {
    indices : vec4<u32>,
    weights : vec4<f32>,
};

struct BonePalette {
    matrices : array<mat4x4<f32>, 128>,
};

@group(0) @binding(0)
var<storage, read> in_vertices: array<Vertex>;

@group(0) @binding(1)
var<storage, read> influences: array<Influence>;

@group(1) @binding(0)
var<uniform> palette: BonePalette;

@group(2) @binding(0)
var<storage, read_write> out_vertices: array<Vertex>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let id = global_id.x;
    
    // Safety check (we should pass the vertex count in a uniform)
    // For now we assume the dispatch is correct.
    
    let v = in_vertices[id];
    let infl = influences[id];
    
    var skinned_pos = vec4<f32>(0.0);
    var skinned_normal = vec3<f32>(0.0);
    
    for (var i = 0u; i < 4u; i++) {
        let bone_idx = infl.indices[i];
        let weight = infl.weights[i];
        
        if (weight > 0.0) {
            let bone_mat = palette.matrices[bone_idx];
            
            // Transform position
            skinned_pos += (bone_mat * vec4<f32>(v.pos, 1.0)) * weight;
            
            // Transform normal (using the same matrix for now, 
            // strictly should be inverse-transpose but for rigid skins this is fine)
            skinned_normal += (mat3x3<f32>(bone_mat[0].xyz, bone_mat[1].xyz, bone_mat[2].xyz) * v.normal) * weight;
        }
    }
    
    var out_v: Vertex;
    out_v.pos = skinned_pos.xyz;
    out_v.normal = normalize(skinned_normal);
    out_v.uv = v.uv;
    
    out_vertices[id] = out_v;
}
