//! GPU instance types for the 2D render path.
//!
//! # Colour-space contract (audit #12)
//!
//! The 2D pipeline renders in **linear light**. `ShapeInstance::color` and
//! `SpriteInstance::color` are linear RGBA values; sRGB sprite/GUI textures are
//! decoded to linear by wgpu on sample; and the output target is an `*Srgb`
//! surface format, so wgpu applies the sRGB encode on write. Constants authored
//! in display-referred sRGB must be converted to linear before being passed
//! here (apply the sRGB EOTF: a display mid-grey of `0.5` becomes `~0.214`
//! linear). This is the single colour semantic shared by shapes and sprites.

use bytemuck::{Pod, Zeroable};

/// Matches the `InstanceInput` struct in `sprite.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SpriteInstance {
    /// First column of the model matrix (x basis).
    pub transform_c0: [f32; 4],
    /// Second column of the model matrix (y basis).
    pub transform_c1: [f32; 4],
    /// Third column of the model matrix (z basis).
    pub transform_c2: [f32; 4],
    /// Fourth column of the model matrix (translation; `.z` carries depth).
    pub transform_c3: [f32; 4],
    /// Linear RGBA tint multiplied with the sampled texel (the texel is
    /// sRGB-decoded by wgpu because sprite textures are `Rgba8UnormSrgb`).
    pub color: [f32; 4],
    /// Source region in UV space `[x, y, width, height]` (for atlas/spritesheet).
    pub uv_rect: [f32; 4],        // x, y, width, height (for atlas)
    /// `[flip_x, flip_y, is_lit, reserved]`.
    pub properties: [f32; 4],     // x=flip_x, y=flip_y, z=is_lit, w=reserved
}

impl SpriteInstance {
    /// Vertex-buffer layout describing this instance type to wgpu.
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

/// Shape-kind sentinel stored in `ShapeInstance::params[3]` (`params.w`).
///
/// # Flag encoding (additive)
///
/// `params.w` used to be documented as `is_filled` but was **never read** by
/// `shape_2d.wgsl`, so the slot is free.  To stay source-compatible the
/// existing constructors keep emitting the historical values and the shader
/// keeps ignoring them:
///
/// | `params.w` | producer | shader branch |
/// |---|---|---|
/// | `0.0` | `rect_outline` (legacy "is_filled = 0") | rounded-rect/circle SDF |
/// | `1.0` | `rect*` / `circle*` / `line*` (legacy "is_filled = 1") | rounded-rect/circle SDF |
/// | `2.0` | **only** [`ShapeInstance::triangle`] | triangle SDF |
///
/// The shader selects the triangle branch with the test `params.w > 1.5`, so
/// the rounded-rect/circle path is byte-for-byte unchanged for every existing
/// instance.  No consumer-facing field or constructor changed shape.
pub const SHAPE_KIND_TRIANGLE: f32 = 2.0;

/// Instance data for non-textured 2D shapes (technical drawing).
///
/// The four transform columns encode a full model matrix. The shader supports
/// the rounded-rectangle SDF family (rectangle, circle, stroked segment) and,
/// since the polygon primitive landed, a filled anti-aliased triangle
/// ([`ShapeInstance::triangle`], selected by [`SHAPE_KIND_TRIANGLE`] in
/// `params.w`). Prefer the ergonomic constructors below over building the
/// matrix by hand.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ShapeInstance {
    /// First column of the model matrix (x basis; its length is the local width).
    pub transform_c0: [f32; 4],
    /// Second column of the model matrix (y basis; its length is the local height).
    pub transform_c1: [f32; 4],
    /// Third column of the model matrix (z basis).
    pub transform_c2: [f32; 4],
    /// Fourth column of the model matrix (translation; `.z` carries depth).
    pub transform_c3: [f32; 4],
    /// Linear RGBA colour (see the module colour-space contract).
    pub color: [f32; 4],
    /// `[border_thickness, corner_radius, smoothing, shape_kind]`.
    ///
    /// `x`/`y`/`z` keep their historical meaning (border, corner radius,
    /// AA smoothing). `w` is the shape-kind flag: `0.0`/`1.0` are the legacy
    /// `is_filled` values for the rounded-rect/circle family (ignored by the
    /// shader) and [`SHAPE_KIND_TRIANGLE`] selects the triangle SDF.
    pub params: [f32; 4],
}

impl ShapeInstance {
    /// Vertex-buffer layout describing this instance type to wgpu.
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

    /// Línea entre dos puntos 2D con grosor `width` y caps redondeados (por defecto).
    pub fn line(from: glam::Vec2, to: glam::Vec2, width: f32, color: [f32; 4]) -> Self {
        Self::line_with_cap(from, to, width, color, 1) // 1 = Round
    }

    /// Línea entre dos puntos 2D con estilo de punta configurable.
    ///
    /// `cap_style`:
    /// - `0` = Flat (extremos rectos exactamente en los endpoints)
    /// - `1` = Round (extremos redondeados, radio = width/2)
    /// - `2` = Square (extremos rectos que sobresalen width/2 más allá de los endpoints)
    ///
    /// Prefer the typed [`ShapeInstance::line_with_cap_style`] in new code: an
    /// out-of-range `u8` here is not rejected and silently falls back to `Round`.
    pub fn line_with_cap(from: glam::Vec2, to: glam::Vec2, width: f32, color: [f32; 4], cap_style: u8) -> Self {
        let delta = to - from;
        let length = delta.length();
        if length < 0.0001 {
            return Self::circle(from, width * 0.5, color);
        }
        let center = (from + to) * 0.5;
        let rotation = delta.y.atan2(delta.x);

        let (geom_length, radius) = match cap_style {
            0 => (length, 0.0),                 // Flat
            1 => (length + width, width * 0.5), // Round
            2 => (length + width, 0.0),         // Square
            _ => (length + width, width * 0.5), // Default a Round
        };

        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(geom_length, width, 1.0),
            glam::Quat::from_rotation_z(rotation),
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        // params: border=width, corner_radius=radius, smooth=2, filled=1
        Self::from_model(model, color, [width, radius, 2.0, 1.0])
    }

    /// Tipada de [`ShapeInstance::line_with_cap`] que acepta el enum
    /// [`CapStyle`](crate::components::CapStyle) en vez del `u8` mágico.
    ///
    /// Es la forma recomendada de construir una línea con una punta concreta:
    /// el compilador rechaza valores inválidos y la intención queda explícita.
    /// Internamente delega en `line_with_cap` para que la geometría generada
    /// sea idéntica.
    ///
    /// ```rust
    /// use ferrous_2d::components::CapStyle;
    /// use ferrous_2d::render::ShapeInstance;
    /// use glam::Vec2;
    ///
    /// let line = ShapeInstance::line_with_cap_style(
    ///     Vec2::ZERO,
    ///     Vec2::new(10.0, 0.0),
    ///     2.0,
    ///     [1.0, 1.0, 1.0, 1.0],
    ///     CapStyle::Flat,
    /// );
    /// ```
    pub fn line_with_cap_style(
        from: glam::Vec2,
        to: glam::Vec2,
        width: f32,
        color: [f32; 4],
        cap_style: crate::components::CapStyle,
    ) -> Self {
        let raw = match cap_style {
            crate::components::CapStyle::Flat => 0,
            crate::components::CapStyle::Round => 1,
            crate::components::CapStyle::Square => 2,
        };
        Self::line_with_cap(from, to, width, color, raw)
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

    /// Triángulo **relleno y anti-aliaseado** con vértices `a`, `b`, `c` en
    /// unidades de mundo (píxeles en Pure2D, origen en el centro de pantalla,
    /// Y hacia arriba).
    ///
    /// # Encoding
    ///
    /// Todo triángulo es la imagen afín del triángulo canónico de esquina
    /// `(0,0)`, `(1,0)`, `(0,1)`, así que cabe en las cuatro columnas `Mat4`
    /// existentes:
    ///
    /// ```text
    /// c0 = b - a
    /// c1 = c - a
    /// c2 = (0, 0, 1, 0)
    /// c3 = a
    /// ```
    ///
    /// El vertex shader de triángulos usa las esquinas del quad en `uv ∈
    /// {0,1}²` (a diferencia del path rect, que las centra en `[-0.5, 0.5]²`),
    /// de modo que `(0,0) → a`, `(1,0) → b`, `(0,1) → c`. El fragment shader
    /// evalúa un SDF de triángulo en espacio de mundo, por lo que el relleno
    /// es **two-sided**: el resultado es el mismo sin importar el winding.
    ///
    /// `params` emitido: `[0, 0, smoothing=2, SHAPE_KIND_TRIANGLE]`. Usar
    /// triángulos en abanico (fan) desde el centroide es la forma directa de
    /// rellenar un polígono convexo con un vértice por lado, en vez de
    /// aproximarlo con muchas instancias de rect.
    pub fn triangle(a: glam::Vec2, b: glam::Vec2, c: glam::Vec2, color: [f32; 4]) -> Self {
        let model = glam::Mat4::from_cols(
            glam::Vec4::new(b.x - a.x, b.y - a.y, 0.0, 0.0),
            glam::Vec4::new(c.x - a.x, c.y - a.y, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, 1.0, 0.0),
            glam::Vec4::new(a.x, a.y, 0.0, 1.0),
        );
        Self::from_model(model, color, [0.0, 0.0, 2.0, SHAPE_KIND_TRIANGLE])
    }
}


/// Shared uniform data for 2D passes.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniform2d {
    /// Combined view-projection matrix, column-major.
    pub view_proj: [f32; 16],
    /// Viewport size in physical pixels `[width, height]`.
    pub resolution: [f32; 2],
    /// Padding to satisfy 16-byte uniform alignment.
    pub padding: [f32; 2], // 16-byte alignment
}

