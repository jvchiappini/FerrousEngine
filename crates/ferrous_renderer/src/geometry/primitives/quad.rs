use crate::geometry::{Mesh, Vertex};
use crate::resources::buffer;

/// Unit quad in the XY plane centred at the origin.
///
/// The mesh spans [-1.0,1.0] in X and Y so that a transform scale of
/// (width*0.5, height*0.5, 1.0) yields a quad of the desired size.
pub fn quad(device: &wgpu::Device) -> Mesh {
    let v = |pos: [f32; 3], uv: [f32; 2]| Vertex {
        position: pos,
        color: [1.0, 1.0, 1.0],
        uv,
    };

    #[rustfmt::skip]
    let vertices: &[Vertex] = &[
        v([-1.0, -1.0, 0.0], [0.0, 0.0]),
        v([ 1.0, -1.0, 0.0], [1.0, 0.0]),
        v([ 1.0,  1.0, 0.0], [1.0, 1.0]),
        v([-1.0,  1.0, 0.0], [0.0, 1.0]),
    ];

    // single-faced winding (CCW) -- back-face triangles will be generated
    // via pipeline culling or a second pipeline when double-sided is needed.
    #[rustfmt::skip]
    let indices: &[u16] = &[
        0, 1, 2, 2, 3, 0,
    ];

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Quad VB", vertices),
        index_buffer: buffer::create_index(device, "Quad IB", indices),
        index_count: indices.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
    }
}
