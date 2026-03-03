use crate::geometry::compute_tangents;
/// Unit cube primitive centred at the origin.
///
/// Each of the six faces has a distinct vertex color so that camera movement
/// is clearly visible during development.  The cube uses 24 unique vertices
/// (4 per face) and 36 indices (2 triangles per face × 6 faces).
use crate::geometry::{Mesh, Vertex};
use crate::resources::buffer;

pub fn cube(device: &wgpu::Device) -> Mesh {
    // helper that includes uv coordinates in addition to position and color
    // helper that builds a vertex with explicit normal and allows
    // overriding the color for debugging/face colouring purposes.
    let v = |pos: [f32; 3], norm: [f32; 3], col: [f32; 3], uv: [f32; 2]| {
        let mut vert = Vertex::new(pos, norm, uv);
        vert.color = col;
        vert
    };

    // All faces use white so the material base_color is not tinted.
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

    // Each face uses its own [0,1]×[0,1] UV space so that:
    //   1. Normal-map sampling is correct (full-range UVs).
    //   2. compute_tangents produces well-conditioned tangents (no 1/6 compression).
    //
    // Winding rule: CCW when viewed from outside (from the direction the normal points).
    // All quads use the same index pattern 0→1→2, 2→3→0.
    // Vertex layout per face: bottom-left, bottom-right, top-right, top-left
    // (matching UV [0,0],[1,0],[1,1],[0,1]).

    #[rustfmt::skip]
    let vertices: Vec<Vertex> = vec![
        // front  (z+)  — viewed from +Z: X right, Y up
        v([-1.0, -1.0,  1.0], [0.0, 0.0, 1.0], WHITE, [0.0, 1.0]),
        v([ 1.0, -1.0,  1.0], [0.0, 0.0, 1.0], WHITE, [1.0, 1.0]),
        v([ 1.0,  1.0,  1.0], [0.0, 0.0, 1.0], WHITE, [1.0, 0.0]),
        v([-1.0,  1.0,  1.0], [0.0, 0.0, 1.0], WHITE, [0.0, 0.0]),
        // back   (z-)  — viewed from -Z: X left, Y up  → flip X order
        v([ 1.0, -1.0, -1.0], [0.0, 0.0,-1.0], WHITE, [0.0, 1.0]),
        v([-1.0, -1.0, -1.0], [0.0, 0.0,-1.0], WHITE, [1.0, 1.0]),
        v([-1.0,  1.0, -1.0], [0.0, 0.0,-1.0], WHITE, [1.0, 0.0]),
        v([ 1.0,  1.0, -1.0], [0.0, 0.0,-1.0], WHITE, [0.0, 0.0]),
        // left   (x-)  — viewed from -X: Z right (toward +Z), Y up → -Z first
        v([-1.0, -1.0, -1.0], [-1.0, 0.0, 0.0], WHITE, [0.0, 1.0]),
        v([-1.0, -1.0,  1.0], [-1.0, 0.0, 0.0], WHITE, [1.0, 1.0]),
        v([-1.0,  1.0,  1.0], [-1.0, 0.0, 0.0], WHITE, [1.0, 0.0]),
        v([-1.0,  1.0, -1.0], [-1.0, 0.0, 0.0], WHITE, [0.0, 0.0]),
        // right  (x+)  — viewed from +X: Z left (toward -Z), Y up → +Z first
        v([ 1.0, -1.0,  1.0], [1.0, 0.0, 0.0], WHITE, [0.0, 1.0]),
        v([ 1.0, -1.0, -1.0], [1.0, 0.0, 0.0], WHITE, [1.0, 1.0]),
        v([ 1.0,  1.0, -1.0], [1.0, 0.0, 0.0], WHITE, [1.0, 0.0]),
        v([ 1.0,  1.0,  1.0], [1.0, 0.0, 0.0], WHITE, [0.0, 0.0]),
        // top    (y+)  — viewed from +Y: X right, Z down (toward -Z)
        v([-1.0,  1.0,  1.0], [0.0, 1.0, 0.0], WHITE, [0.0, 1.0]),
        v([ 1.0,  1.0,  1.0], [0.0, 1.0, 0.0], WHITE, [1.0, 1.0]),
        v([ 1.0,  1.0, -1.0], [0.0, 1.0, 0.0], WHITE, [1.0, 0.0]),
        v([-1.0,  1.0, -1.0], [0.0, 1.0, 0.0], WHITE, [0.0, 0.0]),
        // bottom (y-)  — viewed from -Y: X right, Z up (toward +Z)
        v([-1.0, -1.0, -1.0], [0.0,-1.0, 0.0], WHITE, [0.0, 1.0]),
        v([ 1.0, -1.0, -1.0], [0.0,-1.0, 0.0], WHITE, [1.0, 1.0]),
        v([ 1.0, -1.0,  1.0], [0.0,-1.0, 0.0], WHITE, [1.0, 0.0]),
        v([-1.0, -1.0,  1.0], [0.0,-1.0, 0.0], WHITE, [0.0, 0.0]),
    ];

    // Uniform pattern: every quad is two CCW triangles 0→1→2 and 2→3→0
    #[rustfmt::skip]
    let indices: &[u16] = &[
         0,  1,  2,  2,  3,  0,  // front
         4,  5,  6,  6,  7,  4,  // back
         8,  9, 10, 10, 11,  8,  // left
        12, 13, 14, 14, 15, 12,  // right
        16, 17, 18, 18, 19, 16,  // top
        20, 21, 22, 22, 23, 20,  // bottom
    ];

    // compute tangents before uploading
    let mut vertices = vertices; // make mutable now for tangent computation
    let idx32: Vec<u32> = indices.iter().map(|&i| i as u32).collect();
    compute_tangents(&mut vertices, &idx32);

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Cube VB", &vertices),
        index_buffer: buffer::create_index(device, "Cube IB", indices),
        index_count: indices.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
    }
}
