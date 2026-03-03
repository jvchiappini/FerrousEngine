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

    // one constant per face color for readability
    const RED: [f32; 3] = [1.0, 0.0, 0.0];
    const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
    const BLUE: [f32; 3] = [0.0, 0.0, 1.0];
    const YELLOW: [f32; 3] = [1.0, 1.0, 0.0];
    const MAGENTA: [f32; 3] = [1.0, 0.0, 1.0];
    const CYAN: [f32; 3] = [0.0, 1.0, 1.0];

    // helper to compute UV coordinates based on face index; the texture is
    // assumed to be laid out as a horizontal strip of six regions.  this way
    // each cube face can sample a different portion of the same texture.
    let uv_for = |face: usize, u: f32, v: f32| -> [f32; 2] {
        let region = face as f32 / 6.0;
        [region + u / 6.0, v]
    };

    #[rustfmt::skip]
    let vertices: Vec<Vertex> = vec![
        // front  (z+)
        v([-1.0, -1.0,  1.0], [0.0,0.0,1.0], RED,     uv_for(0, 0.0, 0.0)), v([ 1.0, -1.0,  1.0], [0.0,0.0,1.0], RED,     uv_for(0, 1.0, 0.0)),
        v([ 1.0,  1.0,  1.0], [0.0,0.0,1.0], RED,     uv_for(0, 1.0, 1.0)), v([-1.0,  1.0,  1.0], [0.0,0.0,1.0], RED,     uv_for(0, 0.0, 1.0)),
        // back   (z-)
        v([-1.0, -1.0, -1.0], [0.0,0.0,-1.0], GREEN,   uv_for(1, 0.0, 0.0)), v([ 1.0, -1.0, -1.0], [0.0,0.0,-1.0], GREEN,   uv_for(1, 1.0, 0.0)),
        v([ 1.0,  1.0, -1.0], [0.0,0.0,-1.0], GREEN,   uv_for(1, 1.0, 1.0)), v([-1.0,  1.0, -1.0], [0.0,0.0,-1.0], GREEN,   uv_for(1, 0.0, 1.0)),
        // left   (x-)
        v([-1.0, -1.0, -1.0], [-1.0,0.0,0.0], BLUE,    uv_for(2, 0.0, 0.0)), v([-1.0, -1.0,  1.0], [-1.0,0.0,0.0], BLUE,    uv_for(2, 1.0, 0.0)),
        v([-1.0,  1.0,  1.0], [-1.0,0.0,0.0], BLUE,    uv_for(2, 1.0, 1.0)), v([-1.0,  1.0, -1.0], [-1.0,0.0,0.0], BLUE,    uv_for(2, 0.0, 1.0)),
        // right  (x+)
        v([ 1.0, -1.0, -1.0], [1.0,0.0,0.0], YELLOW,  uv_for(3, 0.0, 0.0)), v([ 1.0, -1.0,  1.0], [1.0,0.0,0.0], YELLOW,  uv_for(3, 1.0, 0.0)),
        v([ 1.0,  1.0,  1.0], [1.0,0.0,0.0], YELLOW,  uv_for(3, 1.0, 1.0)), v([ 1.0,  1.0, -1.0], [1.0,0.0,0.0], YELLOW,  uv_for(3, 0.0, 1.0)),
        // top    (y+)
        v([-1.0,  1.0, -1.0], [0.0,1.0,0.0], MAGENTA,uv_for(4, 0.0, 0.0)), v([-1.0,  1.0,  1.0], [0.0,1.0,0.0], MAGENTA,uv_for(4, 1.0, 0.0)),
        v([ 1.0,  1.0,  1.0], [0.0,1.0,0.0], MAGENTA,uv_for(4, 1.0, 1.0)), v([ 1.0,  1.0, -1.0], [0.0,1.0,0.0], MAGENTA,uv_for(4, 0.0, 1.0)),
        // bottom (y-)
        v([-1.0, -1.0, -1.0], [0.0,-1.0,0.0], CYAN,   uv_for(5, 0.0, 0.0)), v([-1.0, -1.0,  1.0], [0.0,-1.0,0.0], CYAN,   uv_for(5, 1.0, 0.0)),
        v([ 1.0, -1.0,  1.0], [0.0,-1.0,0.0], CYAN,   uv_for(5, 1.0, 1.0)), v([ 1.0, -1.0, -1.0], [0.0,-1.0,0.0], CYAN,   uv_for(5, 0.0, 1.0)),
    ];

    #[rustfmt::skip]
    let indices: &[u16] = &[
        0,  1,  2,  2,  3,  0,  // front
        4,  6,  5,  4,  7,  6,  // back  (CCW flip)
        8,  9,  10, 8,  10, 11, // left
        12, 14, 13, 12, 15, 14, // right (CCW flip)
        16, 17, 18, 16, 18, 19, // top
        20, 22, 21, 20, 23, 22, // bottom (CCW flip)
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
