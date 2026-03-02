/// GPU vertex type used across all built-in render pipelines.
///
/// The layout encodes position and vertex color as contiguous `vec3<f32>`
/// fields so that `bytemuck` can safely reinterpret the slice as bytes.
/// The matching WGSL attribute locations are declared in `assets/shaders/base.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Linear RGB vertex color.
    pub color: [f32; 3],
    /// UV coordinates for texture lookup.  Base shader will sample this
    /// when a material with a texture is bound.
    pub uv: [f32; 2],
}

impl Vertex {
    /// Returns the `VertexBufferLayout` that matches this struct's memory
    /// layout.  Pass this to `wgpu::VertexState::buffers` when building a
    /// render pipeline.
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // @location(0) position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                // @location(1) color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
                // @location(2) uv
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                },
            ],
        }
    }
}
