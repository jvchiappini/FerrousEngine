/// Unit cube primitive centred at the origin.
///
/// Each of the six faces has a distinct vertex color so that camera movement
/// is clearly visible during development.  The cube uses 24 unique vertices
/// (4 per face) and 36 indices (2 triangles per face × 6 faces).
use crate::geometry::{Mesh, Vertex};
use crate::resources::buffer;

pub fn cube(device: &wgpu::Device) -> Mesh {
    let v = |pos: [f32; 3], col: [f32; 3]| Vertex { position: pos, color: col };

    // one constant per face color for readability
    const RED:     [f32; 3] = [1.0, 0.0, 0.0];
    const GREEN:   [f32; 3] = [0.0, 1.0, 0.0];
    const BLUE:    [f32; 3] = [0.0, 0.0, 1.0];
    const YELLOW:  [f32; 3] = [1.0, 1.0, 0.0];
    const MAGENTA: [f32; 3] = [1.0, 0.0, 1.0];
    const CYAN:    [f32; 3] = [0.0, 1.0, 1.0];

    #[rustfmt::skip]
    let vertices: &[Vertex] = &[
        // front  (z+)
        v([-1.0, -1.0,  1.0], RED),    v([ 1.0, -1.0,  1.0], RED),
        v([ 1.0,  1.0,  1.0], RED),    v([-1.0,  1.0,  1.0], RED),
        // back   (z-)
        v([-1.0, -1.0, -1.0], GREEN),  v([ 1.0, -1.0, -1.0], GREEN),
        v([ 1.0,  1.0, -1.0], GREEN),  v([-1.0,  1.0, -1.0], GREEN),
        // left   (x-)
        v([-1.0, -1.0, -1.0], BLUE),   v([-1.0, -1.0,  1.0], BLUE),
        v([-1.0,  1.0,  1.0], BLUE),   v([-1.0,  1.0, -1.0], BLUE),
        // right  (x+)
        v([ 1.0, -1.0, -1.0], YELLOW), v([ 1.0, -1.0,  1.0], YELLOW),
        v([ 1.0,  1.0,  1.0], YELLOW), v([ 1.0,  1.0, -1.0], YELLOW),
        // top    (y+)
        v([-1.0,  1.0, -1.0], MAGENTA),v([-1.0,  1.0,  1.0], MAGENTA),
        v([ 1.0,  1.0,  1.0], MAGENTA),v([ 1.0,  1.0, -1.0], MAGENTA),
        // bottom (y-)
        v([-1.0, -1.0, -1.0], CYAN),   v([-1.0, -1.0,  1.0], CYAN),
        v([ 1.0, -1.0,  1.0], CYAN),   v([ 1.0, -1.0, -1.0], CYAN),
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

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Cube VB", vertices),
        index_buffer:  buffer::create_index(device, "Cube IB", indices),
        index_count:   indices.len() as u32,
        index_format:  wgpu::IndexFormat::Uint16,
    }
}
