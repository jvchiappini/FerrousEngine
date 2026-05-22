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
                wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 16, shader_location: 1, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 32, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 48, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 64, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 80, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
            ],
        }
    }

    // ── Constructores ergonómicos ──────────────────────────────────────────

    fn from_model(model: glam::Mat4, color: [f32; 4], params: [f32; 4]) -> Self {
        Self {
            transform_c0: model.x_axis.into(),
            transform_c1: model.y_axis.into(),
            transform_c2: model.z_axis.into(),
            transform_c3: model.w_axis.into(),
            color,
            params,
        }
    }

    /// Círculo relleno centrado en `center` con `radius` en unidades de mundo (pixels en Pure2D).
    /// `z` controla la profundidad para Z-sorting: 0.0 = fondo, valores mayores = más al frente.
    pub fn circle(center: glam::Vec2, radius: f32, color: [f32; 4]) -> Self {
        Self::circle_z(center, 0.0, radius, color)
    }

    /// Igual que `circle()` pero con control de profundidad Z explícito.
    pub fn circle_z(center: glam::Vec2, z: f32, radius: f32, color: [f32; 4]) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(radius * 2.0, radius * 2.0, 1.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(center.x, center.y, z),
        );
        // params: border=0, corner_radius=radius (círculo perfecto), smooth=2, filled=1
        Self::from_model(model, color, [0.0, radius, 2.0, 1.0])
    }

    /// Rectángulo relleno centrado en `center` con dimensiones `size` (ancho, alto).
    pub fn rect(center: glam::Vec2, size: glam::Vec2, color: [f32; 4]) -> Self {
        Self::rect_z(center, 0.0, size, color)
    }

    /// Igual que `rect()` pero con control de profundidad Z explícito.
    pub fn rect_z(center: glam::Vec2, z: f32, size: glam::Vec2, color: [f32; 4]) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(center.x, center.y, z),
        );
        // params: border=0, corner_radius=0, smooth=2, filled=1
        Self::from_model(model, color, [0.0, 0.0, 2.0, 1.0])
    }

    /// Rectángulo con esquinas redondeadas relleno.
    pub fn rounded_rect(center: glam::Vec2, size: glam::Vec2, corner_radius: f32, color: [f32; 4]) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        Self::from_model(model, color, [0.0, corner_radius, 2.0, 1.0])
    }

    /// Solo el borde (outline) de un rectángulo, sin relleno.
    pub fn rect_outline(center: glam::Vec2, size: glam::Vec2, thickness: f32, color: [f32; 4]) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        // params: border=thickness, corner_radius=0, smooth=2, filled=0
        Self::from_model(model, color, [thickness, 0.0, 2.0, 0.0])
    }

    /// Línea entre dos puntos 2D con grosor `width` y caps redondeados.
    pub fn line(from: glam::Vec2, to: glam::Vec2, width: f32, color: [f32; 4]) -> Self {
        let delta = to - from;
        let length = delta.length();
        if length < 0.0001 {
            return Self::circle(from, width * 0.5, color);
        }
        let center = (from + to) * 0.5;
        let rotation = delta.y.atan2(delta.x);
        // Añadir `width` a la longitud para que los caps redondeados lleguen
        // exactamente al punto de inicio y fin (radio = width/2).
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(length + width, width, 1.0),
            glam::Quat::from_rotation_z(rotation),
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        // params: border=width (grosor), corner_radius=width/2 (caps redondos), smooth=2, filled=1
        Self::from_model(model, color, [width, width * 0.5, 2.0, 1.0])
    }

    /// Rectángulo rotado, centrado en `center`, con ángulo en radianes.
    pub fn rect_rotated(center: glam::Vec2, size: glam::Vec2, angle_radians: f32, color: [f32; 4]) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::from_rotation_z(angle_radians),
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        Self::from_model(model, color, [0.0, 0.0, 2.0, 1.0])
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

