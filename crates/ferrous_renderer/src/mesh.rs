use wgpu::util::DeviceExt;

/// Simple vertex type for the renderer.
///
/// Position and color are packed as vec3<f32> each so that they fit nicely in
/// the GPU buffer. We derive `Pod`/`Zeroable` to allow safe casting with
/// `bytemuck` when creating the buffers.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    /// Returns a `VertexBufferLayout` that matches the memory layout of this
    /// struct. This is used when constructing the render pipeline so the GPU
    /// knows how to interpret the vertex buffer contents.
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
            ],
        }
    }
}

/// A simple mesh containing a vertex and index buffer along with the number of
/// indices to draw. Higher‑level code can create arbitrary meshes and render
/// them with the appropriate pipeline.
pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl Mesh {
    /// Convenience constructor that produces a unit cube centered at the origin.
    ///
    /// Each face has a distinct color so that it's obvious when the camera
    /// moves around. The cube uses 16 unique vertices (shared positions but
    /// different colors on each face) and 36 indices (12 triangles).
    pub fn cube(device: &wgpu::Device) -> Self {
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
            // front
            0, 1, 2, 2, 3, 0, // back
            4, 5, 6, 6, 7, 4, // left
            8, 9, 10, 10, 11, 8, // right
            12, 13, 14, 14, 15, 12, // top
            16, 17, 18, 18, 19, 16, // bottom
            20, 21, 22, 22, 23, 20,
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

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        }
    }
}
