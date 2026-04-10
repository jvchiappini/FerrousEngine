// hdda_primary.wgsl — HDDA two-level voxel raymarching → G-Buffer
//
// Dispatch: ceil(width / 8) × ceil(height / 8) × 1 workgroups, 8×8×1 threads.
//
// Outputs (storage textures written per-pixel):
//   gbuf_pos    Rgba32Float  (world_x, world_y, world_z, linear_depth)
//   gbuf_norm   Rgba8Snorm   (nx, ny, nz, hit: 1.0 = hit  0.0 = sky)
//   gbuf_albedo Rgba8Unorm   (r, g, b, material_id / 255.0)
//
// The pass visualises hit normals as RGB colours:
//   +X → red,   -X → dark red
//   +Y → green, -Y → dark green
//   +Z → blue,  -Z → dark blue
//   sky (no hit) → black
//
// ─── WGSL Struct Definitions ──────────────────────────────────────────────────

/// One node in the HashDAG.  Matches GpuDagNode in dag/gpu_types.rs.
/// children[octant] == 0xFFFFFFFF means the octant is empty.
struct DagNode {
    children       : array<u32, 8>,
    occupancy_mask : u32,
    emissive_mask  : u32,
}

/// One live streaming chunk (≈ 40 m cube) stored in the roots SSBO.
struct ChunkRoot {
    cx       : i32,
    cy       : i32,
    cz       : i32,
    root_idx : u32,
}

/// Camera + projection data uploaded every frame.
struct CameraUniform {
    inv_view_proj : mat4x4<f32>,  // clip → world
    eye           : vec4<f32>,    // xyz = eye position, w unused
    resolution    : vec4<f32>,    // xy = (width, height), zw unused
    near_far      : vec4<f32>,    // x = near, y = far, zw unused
}

/// Per-level base indices in the flat dag_nodes array.
/// base[L] = first node index for level L (L=0 finest, L=12 root).
/// Stored as 5 × vec4<u32> (= 80 bytes) to satisfy uniform 16-byte alignment.
/// Slots 0-3: level offsets 0-15 (only 0-12 are used).
/// Slot 4: .x = total_nodes, .yzw = reserved.
struct LevelOffsets {
    base : array<vec4<u32>, 5>,
}

// ─── Bindings ─────────────────────────────────────────────────────────────────

@group(0) @binding(0) var<storage, read> dag_nodes     : array<DagNode>;
@group(0) @binding(1) var<storage, read> chunk_roots   : array<ChunkRoot>;
@group(0) @binding(2) var<uniform>       camera        : CameraUniform;
@group(0) @binding(3) var<uniform>       level_offsets : LevelOffsets;

@group(1) @binding(0) var gbuf_pos    : texture_storage_2d<rgba32float, write>;
@group(1) @binding(1) var gbuf_norm   : texture_storage_2d<rgba8snorm,  write>;
@group(1) @binding(2) var gbuf_albedo : texture_storage_2d<rgba8unorm,  write>;

// ─── Constants ────────────────────────────────────────────────────────────────

// Number of DAG levels (must match DAG_LEVELS in Rust).
const DAG_LEVELS : u32 = 13u;
// Chunk size in voxels at level 12 (= 2^12 = 4096).
const CHUNK_SIZE : f32 = 4096.0;
// Maximum HDDA steps before giving up (prevents infinite loops).
const MAX_STEPS  : u32 = 256u;

// ─── Helper: base index for a level ──────────────────────────────────────────

fn level_base(level: u32) -> u32 {
    // LevelOffsets.base is stored as array<vec4<u32>, 5>.
    // level L maps to base[L/4][L%4].  Slots 0-3 hold levels 0-15.
    let slot = level / 4u;
    let comp = level % 4u;
    let v = level_offsets.base[slot];
    // Manual component select (WGSL forbids dynamic vec indexing pre-2024).
    if comp == 0u { return v.x; }
    if comp == 1u { return v.y; }
    if comp == 2u { return v.z; }
    return v.w;
}

// ─── Binary search in chunk_roots array ──────────────────────────────────────

/// Return root_idx for chunk (cx, cy, cz), or 0xFFFFFFFF if not found.
fn find_root(cx: i32, cy: i32, cz: i32) -> u32 {
    let n = level_offsets.base[4].y;
    if n == 0u { return 0xFFFFFFFFu; }

    var lo: u32 = 0u;
    var hi: u32 = n;
    loop {
        if lo >= hi { break; }
        let mid = lo + (hi - lo) / 2u;
        let r = chunk_roots[mid];
        if r.cx < cx || (r.cx == cx && r.cy < cy) || (r.cx == cx && r.cy == cy && r.cz < cz) {
            lo = mid + 1u;
        } else if r.cx == cx && r.cy == cy && r.cz == cz {
            return r.root_idx;
        } else {
            if mid == 0u { break; }
            hi = mid;
        }
    }
    return 0xFFFFFFFFu;
}

// ─── HDDA traversal ───────────────────────────────────────────────────────────

/// Result of one HDDA traversal.
struct HitResult {
    hit       : bool,
    pos       : vec3<f32>,   // world-space hit position (surface, not voxel centre)
    normal    : vec3<f32>,   // outward face normal (one of ±X, ±Y, ±Z)
    material  : u32,         // material_id from the leaf voxel
    depth     : f32,         // linear distance from eye
    debug     : vec3<f32>,   // debug color payload
}

/// Compute the octant index (0-7) for a position inside a cell of given half-size.
fn octant_of(local: vec3<f32>, half: f32) -> u32 {
    return  (u32(local.x >= half)      )
          | (u32(local.y >= half) << 1u)
          | (u32(local.z >= half) << 2u);
}

/// Trace one ray through the HashDAG.  Returns a HitResult.
fn hdda_trace(ray_o: vec3<f32>, ray_d: vec3<f32>) -> HitResult {
    var result: HitResult;
    result.hit = false;
    result.debug = vec3<f32>(0.0); // Default black debug

    // −− Chunk-level DDA −−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−−
    // We march chunk by chunk (40 m cubes) along the ray, then for each
    // occupied chunk descend the DAG to find the precise hit.

    // Ray direction reciprocal (safe: avoid div-by-zero with small epsilon).
      let eps = 1e-6;
      let dx = select(ray_d.x, select(-eps, eps, ray_d.x >= 0.0), abs(ray_d.x) < eps);
      let dy = select(ray_d.y, select(-eps, eps, ray_d.y >= 0.0), abs(ray_d.y) < eps);
      let dz = select(ray_d.z, select(-eps, eps, ray_d.z >= 0.0), abs(ray_d.z) < eps);
      let inv_d = vec3<f32>(1.0 / dx, 1.0 / dy, 1.0 / dz);    // Current chunk coordinate (integer grid).
    var cx: i32 = i32(floor(ray_o.x / CHUNK_SIZE));
    var cy: i32 = i32(floor(ray_o.y / CHUNK_SIZE));
    var cz: i32 = i32(floor(ray_o.z / CHUNK_SIZE));

    // DDA step direction per axis.
    let step_x: i32 = select(-1, 1, ray_d.x >= 0.0);
    let step_y: i32 = select(-1, 1, ray_d.y >= 0.0);
    let step_z: i32 = select(-1, 1, ray_d.z >= 0.0);

    // t at the next chunk boundary per axis.
    let chunk_min = vec3<f32>(f32(cx), f32(cy), f32(cz)) * CHUNK_SIZE;
    let next_x = chunk_min.x + select(0.0, CHUNK_SIZE, ray_d.x >= 0.0);
    let next_y = chunk_min.y + select(0.0, CHUNK_SIZE, ray_d.y >= 0.0);
    let next_z = chunk_min.z + select(0.0, CHUNK_SIZE, ray_d.z >= 0.0);

    var t_max = vec3<f32>(
        (next_x - ray_o.x) * inv_d.x,
        (next_y - ray_o.y) * inv_d.y,
        (next_z - ray_o.z) * inv_d.z,
    );
    let t_delta = vec3<f32>(CHUNK_SIZE) * abs(inv_d);

    var last_normal = vec3<f32>(0.0, 1.0, 0.0);
    var t_enter = 0.0;
    
    // Check if total nodes is even uploaded correctly
    let total_roots = level_offsets.base[4].y;
    if total_roots == 0u {
        result.debug = vec3<f32>(0.0, 1.0, 1.0); // CYAN: 0 roots uploaded to uniform
        return result;
    }

    if abs(ray_d.x) + abs(ray_d.y) + abs(ray_d.z) < 0.1 {
        result.debug = vec3<f32>(1.0, 0.5, 0.0); // ORANGE: ray_d is broken
        return result;
    }

    // Let's debug the ray by showing its direction!
    // result.debug = abs(ray_d); return result;

    result.debug = vec3<f32>(0.2, 0.2, 0.2); // GRAY: Passed setup

    var touched_chunk = false;
    for (var step = 0u; step < MAX_STEPS; step++) {
        // Check if this chunk is occupied.
        let root_idx = find_root(cx, cy, cz);
        if root_idx != 0xFFFFFFFFu {
            touched_chunk = true;
            result.debug = vec3<f32>(1.0, 0.0, 1.0); // MAGENTA: Found a chunk! (but DAG failed)

            // Descend the DAG to find the precise hit inside this chunk.
            let chunk_origin = vec3<f32>(f32(cx), f32(cy), f32(cz)) * CHUNK_SIZE;
            let local_o = ray_o + ray_d * t_enter - chunk_origin;

            let hit = dag_descend(root_idx, local_o, ray_d, inv_d);
            if hit.hit {
                result.hit = true;
                result.pos = chunk_origin + hit.pos;
                result.normal = hit.normal;
                result.material = hit.material;
                result.depth = t_enter + hit.depth;
                result.debug = vec3<f32>(0.0, 1.0, 0.0); // GREEN: Hit!
                return result;
            } else {
                // Keep recording debug colours in case it misses all chunks
                if hit.min_level == 19u {
                    result.debug = vec3<f32>(0.0, 1.0, 1.0); // TURQUOISE: Ray missed all children
                } else if hit.min_level == 21u {
                    result.debug = vec3<f32>(1.0, 0.0, 1.0); // MAGENTA: NaN in intersection test
                } else if hit.min_level == 15u {
                    result.debug = vec3<f32>(0.0, 0.0, 1.0); // Blue: near > far
                } else if hit.min_level == 16u {
                    result.debug = vec3<f32>(0.0, 1.0, 0.0); // Green: far < 0
                } else if hit.min_level == 17u {
                    result.debug = vec3<f32>(1.0, 1.0, 0.0); // Yellow: empty node
                } else if hit.min_level == 18u {
                    result.debug = vec3<f32>(1.0, 0.0, 1.0); // Magenta: inconsistent DAG
                } else if hit.min_level == 19u {
                    result.debug = vec3<f32>(0.0, 1.0, 1.0); // Cyan: missed all occupied subcells (child_pushed == false)
                } else if hit.min_level == 21u {
                    result.debug = vec3<f32>(1.0, 0.5, 0.0); // Orange: NaN
                } else {
                    // actual hit levels 0-13 (12 = red, 0 = blue)
                    let shade = f32(hit.min_level) / 12.0;
                    result.debug = vec3<f32>(shade, 0.0, 1.0 - shade);
                }
                // Do NOT return here! A chunk might be sparsely populated.
                // We must continue the chunk DDA to check the next chunk along the ray.
            }
        }        // Advance to next chunk along the ray (DDA step).
        if t_max.x < t_max.y && t_max.x < t_max.z {
            t_enter = t_max.x;
            t_max.x += t_delta.x;
            cx += step_x;
            last_normal = vec3<f32>(-f32(step_x), 0.0, 0.0);
        } else if t_max.y < t_max.z {
            t_enter = t_max.y;
            t_max.y += t_delta.y;
            cy += step_y;
            last_normal = vec3<f32>(0.0, -f32(step_y), 0.0);
        } else {
            t_enter = t_max.z;
            t_max.z += t_delta.z;
            cz += step_z;
            last_normal = vec3<f32>(0.0, 0.0, -f32(step_z));
        }
    }

    if touched_chunk {
        // Keep the debug color from the dag_descend miss
        // result.debug = ... already set ...
    } else {
        result.debug = vec3<f32>(0.0, 0.0, 1.0); // BLUE: No chunk found at all!
    }
    return result; // miss
}

// ─── DAG descent inside one chunk ────────────────────────────────────────────

/// Sub-result from traversing one chunk's DAG subtree.
struct ChunkHit {
    hit: bool,
    pos: vec3<f32>,
    normal: vec3<f32>,
    depth: f32,
    material: u32,
    min_level: u32,
}

/// Descend the DAG from level 12 down to level 0 following the ray.
///
/// `local_o` is the ray origin relative to the chunk's corner, already
/// clipped to the chunk interior (or slightly inside).
/// Returns a ChunkHit with `pos` in chunk-local coordinates.
fn dag_descend(root_idx: u32, local_o: vec3<f32>, ray_d: vec3<f32>, inv_d: vec3<f32>) -> ChunkHit {
    var result: ChunkHit;
    result.hit = false;
    result.min_level = 12u;

    // Stack for the iterative descent (max depth = DAG_LEVELS = 13).
    // Each entry: (node_idx, cell_origin, cell_size, level).
    // We keep it simple: one stack per dimension, max 13 entries.
    var stack_node   : array<u32,     128>;
    var stack_origin : array<vec3<f32>, 128>;
    var stack_size   : array<f32,     128>;
    var stack_level  : array<u32,     128>;
    var stack_top    : i32 = -1;

    // Push root.
    stack_top = 0;
    stack_node  [0] = root_idx;
    stack_origin[0] = vec3<f32>(0.0);
    stack_size  [0] = CHUNK_SIZE;
    stack_level [0] = DAG_LEVELS - 1u;

      for (var iter = 0u; iter < 4000u && stack_top >= 0; iter++) {
        if iter >= 3995u {
            result.hit = false;
            return result;
        }          let top   = u32(stack_top);
          let idx   = stack_node  [top];
          let orig  = stack_origin[top];
          let csize = stack_size  [top];
          let level = stack_level [top];
          stack_top--;

          if level < result.min_level {
              result.min_level = level;
          }

          // Compute ray's slab intersection with this cell.
          let cell_min = orig;
          let cell_max = orig + vec3<f32>(csize);

        var t0 = (cell_min - local_o) * inv_d;
        var t1 = (cell_max - local_o) * inv_d;
        // Ensure t0 <= t1.
        let t_near_v = min(t0, t1);
        let t_far_v  = max(t0, t1);
        let t_near = max(max(t_near_v.x, t_near_v.y), t_near_v.z);
        let t_far  = min(min(t_far_v.x,  t_far_v.y),  t_far_v.z);

        // Detect NaNs early; they can happen if inv_d contains NaN/Inf.
        // WGSL in this version may not provide isNan(), so use self-inequality.
        if any(t_near_v != t_near_v) || any(t_far_v != t_far_v) {
            if level == 12u {
                result.min_level = 21u; // NaN in intersection test
            }
            continue;
        }

        if t_near > t_far + 0.001 {
            // bounding-box miss; record a sentinel on the root level for
            // post-mortem coloring but do not try to write to a field that
            // doesn't exist on ChunkHit (WGSL errors out!).
            if level == 12u {
                result.min_level = 15u; // near > far
            }
            continue;
        }
        if t_far < -0.001 {
            if level == 12u {
                result.min_level = 16u; // Special debug code: far < 0
            }
            continue;
        }

        // Leaf level: this IS the voxel.
        if level == 0u {
            // Reconstruct hit surface normal from which face was entered.
            let norm = entry_normal(t_near_v, ray_d);
            let hit_pos = local_o + ray_d * max(t_near, 0.0);

            // Read material from the leaf node's children[0] (packed voxel).
            let base = level_base(0u);
            let leaf_node = dag_nodes[base + idx];
            let packed = leaf_node.children[0];
            let mat = packed & 0xFFu;

            result.hit     = true;
            result.pos     = hit_pos;
            result.normal  = norm;
            result.material = mat;
            result.depth   = max(t_near, 0.0);
            return result;
        }

        // Interior node: visit children ordered by ray direction (near→far).
        let half = csize * 0.5;
          let base = level_base(level);
          let node = dag_nodes[base + idx];

          if node.occupancy_mask == 0u {
              if level == 12u {
                  result.min_level = 17u;
              }
              continue;
          }        // Determine which octant contains the ray entry point.
        let entry = local_o + ray_d * max(t_near, 0.0);
        let local_entry = entry - orig;
        let first_oct = octant_of(local_entry, half);

        // Visit all 8 octants, but push only occupied ones.
        // We push in reverse priority order so the nearest is popped first.
        // Simple approach: push all occupied children, rely on stack ordering.
        // (A full HDDA would use 3D DDA to visit in strict order; this is
        //  correct but may visit a few extra nodes.)
        var child_pushed = false;
        for (var o = 0u; o < 8u; o++) {
            // Push octants in reverse distance order so first_oct is popped first.
            let oc = (7u - o) ^ first_oct;
            let bit = 1u << oc;
            if (node.occupancy_mask & bit) == 0u { continue; }
            let child_idx = node.children[oc];
            if child_idx == 0xFFFFFFFFu {
                // inconsistent DAG data; mark specially and bail
                if level == 12u {
                    result.min_level = 18u;
                }
                continue;
            }
            let child_orig = orig + vec3<f32>(
                f32((oc     ) & 1u) * half,
                f32((oc >> 1u) & 1u) * half,
                f32((oc >> 2u) & 1u) * half,
            );

            // Ray-AABB check for this child; skip children that are not hit.
            let child_min = child_orig;
            let child_max = child_orig + vec3<f32>(half);
            let c_t0 = (child_min - local_o) * inv_d;
            let c_t1 = (child_max - local_o) * inv_d;
            let c_near_v = min(c_t0, c_t1);
            let c_far_v  = max(c_t0, c_t1);
            let c_near = max(max(c_near_v.x, c_near_v.y), c_near_v.z);
            let c_far  = min(min(c_far_v.x,  c_far_v.y),  c_far_v.z);
            if c_near > c_far + 0.001 || c_far < -0.001 {
                continue;
            }

            if stack_top < 127 {
                child_pushed = true;
                stack_top++;
                let st = u32(stack_top);
                stack_node  [st] = child_idx;
                stack_origin[st] = child_orig;
                stack_size  [st] = half;
                stack_level [st] = level - 1u;
            }
        }

        // If we didn't push any children, the ray misses all occupied subcells.
        if !child_pushed {
            if level == 12u {
                result.min_level = 19u;
            }
            continue;
        }
    }

    return result; // miss inside chunk
}

/// Compute the entry normal from per-axis slab t values.
fn entry_normal(t_near_v: vec3<f32>, ray_d: vec3<f32>) -> vec3<f32> {
    if t_near_v.x >= t_near_v.y && t_near_v.x >= t_near_v.z {
        return vec3<f32>(-sign(ray_d.x), 0.0, 0.0);
    } else if t_near_v.y >= t_near_v.z {
        return vec3<f32>(0.0, -sign(ray_d.y), 0.0);
    } else {
        return vec3<f32>(0.0, 0.0, -sign(ray_d.z));
    }
}

// ─── Main compute entry point ─────────────────────────────────────────────────

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let res = vec2<u32>(u32(camera.resolution.x), u32(camera.resolution.y));
    if gid.x >= res.x || gid.y >= res.y { return; }

    // ── Reconstruct world-space ray ──────────────────────────────────────────
    // NDC in [-1, 1] (y flipped: wgpu/WGSL convention is y-up in clip space).
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(camera.resolution.xy);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);

    // Unproject near and far plane points through inverse view-projection.
    let clip_near = vec4<f32>(ndc, 0.0, 1.0);
    let clip_far  = vec4<f32>(ndc, 1.0, 1.0);
    let world_near_h = camera.inv_view_proj * clip_near;
    let world_far_h  = camera.inv_view_proj * clip_far;
    let world_near = world_near_h.xyz / world_near_h.w;
    let world_far  = world_far_h.xyz  / world_far_h.w;

    let ray_o = camera.eye.xyz;
    let ray_d = normalize(world_far - world_near);

    // ── Trace ────────────────────────────────────────────────────────────────
    let hit = hdda_trace(ray_o, ray_d);

    let coord = vec2<i32>(gid.xy);

    if hit.hit {
        // G-Buffer: position + depth.
        textureStore(gbuf_pos, coord, vec4<f32>(hit.pos, hit.depth));

        // G-Buffer: normal (snorm; w = 1.0 = hit).
        textureStore(gbuf_norm, coord, vec4<f32>(hit.normal, 1.0));

        // G-Buffer: albedo — visualise normal as colour for Phase 3 debug.
        let norm_colour = hit.normal * 0.5 + 0.5;
        let mat_f = f32(hit.material) / 255.0;
        textureStore(gbuf_albedo, coord, vec4<f32>(norm_colour, mat_f));
    } else {
        // Sky / miss - write debug colors instead of sky!
        textureStore(gbuf_pos,    coord, vec4<f32>(0.0));
        textureStore(gbuf_norm,   coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        
        // Write the debug color:
        textureStore(gbuf_albedo, coord, vec4<f32>(hit.debug, 1.0));
    }
}
