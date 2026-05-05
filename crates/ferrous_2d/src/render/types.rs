use bytemuck::{Pod, Zeroable};

/// Matches the `InstanceInput` struct in `sprite.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SpriteInstance {
    pub transform_c0: [f32; 4],
    pub transform_c1: [f32; 4],
    pub transform_c2: [f32; 4],
    pub transform_c3: [f32; 4],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],        // x, y, width, height (for atlas)
    pub properties: [f32; 4],     // x=flip_x, y=flip_y, z=is_lit, w=reserved
}

impl SpriteInstance {
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 16, shader_location: 1, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 32, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 48, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 64, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 80, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 96, shader_location: 6, format: wgpu::VertexFormat::Float32x4 },
            ],
        }
    }
}

/// Instance data for non-textured 2D shapes (technical drawing).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ShapeInstance {
    pub transform_c0: [f32; 4],
    pub transform_c1: [f32; 4],
    pub transform_c2: [f32; 4],
    pub transform_c3: [f32; 4],
    pub color: [f32; 4],
    pub params: [f32; 4],         // x=border_thickness, y=corner_radius, z=smoothing, w=is_filled
}

impl ShapeInstance {
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ShapeInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 16, shader_location: 1, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 32, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 48, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 64, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 80, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
            ],
        }
    }
}

/// Shared uniform data for 2D passes.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniform2d {
    pub view_proj: [f32; 16],
    pub resolution: [f32; 2],
    pub padding: [f32; 2], // 16-byte alignment
}

