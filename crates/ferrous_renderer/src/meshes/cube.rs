use wgpu::util::DeviceExt;

// Re-export types from the parent module to avoid long paths in callers.
use crate::mesh::{Mesh, Vertex};

/// Convenience constructor that produces a unit cube centered at the origin.
///
/// The implementation used to live directly in `mesh.rs`; it has been moved
/// here so that the renderer can keep a small, generic `Mesh` type and a
/// collection of specialised primitives in a separate directory. The returned
/// `Mesh` is identical to the previous version, with 16 coloured vertices and
/// 36 indices forming a cube.
pub fn cube(device: &wgpu::Device) -> Mesh {
    let vertices: &[Vertex] = &[
        // front (z+)
        Vertex {
            position: [-1.0, -1.0, 1.0],
            color: [1.0, 0.0, 0.0],
        },
        Vertex {
            position: [1.0, -1.0, 1.0],
            color: [1.0, 0.0, 0.0],
        },
        Vertex {
            position: [1.0, 1.0, 1.0],
            color: [1.0, 0.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 1.0],
            color: [1.0, 0.0, 0.0],
        },
        // back (z-)
        Vertex {
            position: [-1.0, -1.0, -1.0],
            color: [0.0, 1.0, 0.0],
        },
        Vertex {
            position: [1.0, -1.0, -1.0],
            color: [0.0, 1.0, 0.0],
        },
        Vertex {
            position: [1.0, 1.0, -1.0],
            color: [0.0, 1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, -1.0],
            color: [0.0, 1.0, 0.0],
        },
        // left (x-)
        Vertex {
            position: [-1.0, -1.0, -1.0],
            color: [0.0, 0.0, 1.0],
        },
        Vertex {
            position: [-1.0, -1.0, 1.0],
            color: [0.0, 0.0, 1.0],
        },
        Vertex {
            position: [-1.0, 1.0, 1.0],
            color: [0.0, 0.0, 1.0],
        },
        Vertex {
            position: [-1.0, 1.0, -1.0],
            color: [0.0, 0.0, 1.0],
        },
        // right (x+)
        Vertex {
            position: [1.0, -1.0, -1.0],
            color: [1.0, 1.0, 0.0],
        },
        Vertex {
            position: [1.0, -1.0, 1.0],
            color: [1.0, 1.0, 0.0],
        },
        Vertex {
            position: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 0.0],
        },
        Vertex {
            position: [1.0, 1.0, -1.0],
            color: [1.0, 1.0, 0.0],
        },
        // top (y+)
        Vertex {
            position: [-1.0, 1.0, -1.0],
            color: [1.0, 0.0, 1.0],
        },
        Vertex {
            position: [-1.0, 1.0, 1.0],
            color: [1.0, 0.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 1.0],
            color: [1.0, 0.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, -1.0],
            color: [1.0, 0.0, 1.0],
        },
        // bottom (y-)
        Vertex {
            position: [-1.0, -1.0, -1.0],
            color: [0.0, 1.0, 1.0],
        },
        Vertex {
            position: [-1.0, -1.0, 1.0],
            color: [0.0, 1.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 1.0],
            color: [0.0, 1.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, -1.0],
            color: [0.0, 1.0, 1.0],
        },
    ];

    let indices: &[u16] = &[
        // front (z+)
        0, 1, 2, 2, 3, 0, // back (z-)
        4, 6, 5, 4, 7, 6, // left (x-)
        8, 9, 10, 8, 10, 11,
        // right (x+)
        // winding reversed so that normal points +X
        12, 14, 13, 12, 15, 14, // top (y+)
        16, 17, 18, 16, 18, 19, // bottom (y-)
        20, 22, 21, 20, 23, 22,
    ];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Mesh Vertex Buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Mesh Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    Mesh {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    }
}
