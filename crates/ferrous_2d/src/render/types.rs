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
//!
//! # Alpha contract
//!
//! Instance colours are **straight** (non-premultiplied) linear RGBA.  The
//! fragment shaders premultiply on output (`vec4(rgb * a, a)`) and every
//! Pure2D pipeline blends with a premultiplied state, so callers never pass
//! premultiplied colours.  Over an opaque destination this is mathematically
//! identical to straight-alpha blending; on a transparent target it is the
//! correct compositing result (see [`crate::render::BlendMode2d`]).

use bytemuck::{Pod, Zeroable};

use crate::handle::Handle;

/// Matches the `InstanceInput` struct in `sprite.wgsl`.
///
/// `color` is a **straight** linear RGBA tint (not premultiplied); the sprite
/// shader premultiplies after sampling and tinting.  `properties.w` is the
/// per-texture premultiplied-data flag, filled in by the renderer from the
/// texture registry (0 = straight alpha data, 1 = premultiplied data).
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
    pub uv_rect: [f32; 4], // x, y, width, height (for atlas)
    /// `[flip_x, flip_y, is_lit, reserved]`.
    pub properties: [f32; 4], // x=flip_x, y=flip_y, z=is_lit, w=reserved
}

impl SpriteInstance {
    /// Vertex-buffer layout describing this instance type to wgpu.
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Stable, generation-tracked handle to a font registered with
/// [`crate::render::Renderer2d`]'s owner (the engine `Renderer`), used by
/// [`TextInstance`] draw calls.
///
/// The index identifies the font slot and the generation makes handles issued
/// before a [`crate::handle::SlotRegistry::remove`] stale, so a recycled font
/// can never be addressed by an old handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId2d {
    index: u32,
    generation: u32,
}

impl FontId2d {
    /// Returns the font slot index.
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Returns the generation of the font slot when the handle was issued.
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// Returns the underlying generational `Handle`.
    pub const fn handle(&self) -> Handle {
        Handle::from_parts(self.index, self.generation)
    }
}

impl From<Handle> for FontId2d {
    fn from(handle: Handle) -> Self {
        Self {
            index: handle.index(),
            generation: handle.generation(),
        }
    }
}

impl From<FontId2d> for Handle {
    fn from(font: FontId2d) -> Self {
        font.handle()
    }
}

/// GPU instance for one MSDF glyph quad (Pure2D text path).
///
/// `color` is a **straight** linear RGBA colour (not premultiplied); the text
/// shader premultiplies when it writes the fragment.
///
/// The model matrix maps the canonical local quad `[0, 1]²` (Y up) to the
/// glyph's pixel rectangle, exactly like [`SpriteInstance`] maps its own quad,
/// so rotation, non-uniform scale and parent transforms compose the same way.
/// `transform_c3[2]` carries the depth used by the unified painter-order
/// command stream (shared by every glyph of one `draw_text` call).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextInstance {
    /// First column of the model matrix (x basis; its length is the quad width).
    pub transform_c0: [f32; 4],
    /// Second column of the model matrix (y basis; its length is the quad height).
    pub transform_c1: [f32; 4],
    /// Third column of the model matrix.
    pub transform_c2: [f32; 4],
    /// Fourth column (translation; `.z` carries depth).
    pub transform_c3: [f32; 4],
    /// Linear RGBA colour (see the module colour-space contract).
    pub color: [f32; 4],
    /// Atlas UV rectangle `[u0, v0, u1, v1]`.
    pub uv_rect: [f32; 4],
    /// `[px_range, 0, 0, 0]` where `px_range` is `FontAtlas::px_range`.
    pub params: [f32; 4],
}

impl TextInstance {
    /// Vertex-buffer layout describing this instance type to wgpu.
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }

    /// Build an instance from a model matrix, an atlas UV rect and a colour.
    ///
    /// `px_range` must be the atlas' [`crate::render::TextInstance::params`]
    /// range (see `ferrous_font::FontAtlas::px_range`).
    pub fn new(model: glam::Mat4, uv_rect: [f32; 4], color: [f32; 4], px_range: f32) -> Self {
        Self {
            transform_c0: model.x_axis.into(),
            transform_c1: model.y_axis.into(),
            transform_c2: model.z_axis.into(),
            transform_c3: model.w_axis.into(),
            color,
            uv_rect,
            params: [
                if px_range.is_finite() && px_range > 0.0 {
                    px_range
                } else {
                    6.0
                },
                0.0,
                0.0,
                0.0,
            ],
        }
    }

    /// Depth used by the unified painter-order command stream.
    #[inline]
    pub fn depth(&self) -> f32 {
        self.transform_c3[2]
    }

    /// World-space AABB `(min, max)` of the canonical `[0, 1]²` quad.
    ///
    /// Conservative (the MSDF AA band extends slightly past the quad); callers
    /// expand the cull rect by a pixel margin before testing.
    #[inline]
    pub fn world_aabb(&self) -> (glam::Vec2, glam::Vec2) {
        let c0 = glam::Vec2::new(self.transform_c0[0], self.transform_c0[1]);
        let c1 = glam::Vec2::new(self.transform_c1[0], self.transform_c1[1]);
        let c3 = glam::Vec2::new(self.transform_c3[0], self.transform_c3[1]);
        let p0 = c3;
        let p1 = c3 + c0;
        let p2 = c3 + c1;
        let p3 = c3 + c0 + c1;
        (p0.min(p1).min(p2).min(p3), p0.max(p1).max(p2).max(p3))
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

/// Triangle edge-AA mask bit for the `a → b` edge (bit 0).
///
/// Set the bit to anti-alias that edge; clear it (hard edge) for an internal
/// edge shared with a neighbouring triangle, so the two triangles tile exactly
/// and no seam appears.  See [`ShapeInstance::polygon_triangle`].
pub const TRIANGLE_EDGE_AB: u8 = 1 << 0;
/// Triangle edge-AA mask bit for the `b → c` edge (bit 1).
pub const TRIANGLE_EDGE_BC: u8 = 1 << 1;
/// Triangle edge-AA mask bit for the `c → a` edge (bit 2).
pub const TRIANGLE_EDGE_CA: u8 = 1 << 2;
/// All three triangle edges anti-aliased (`0b111`); the default of
/// [`ShapeInstance::triangle`].
pub const TRIANGLE_EDGES_ALL: u8 = TRIANGLE_EDGE_AB | TRIANGLE_EDGE_BC | TRIANGLE_EDGE_CA;

/// Instance data for non-textured 2D shapes (technical drawing).
///
/// `color` is **straight** linear RGBA (not premultiplied) for every shape
/// kind; the shape shader premultiplies its output.
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
    ///
    /// `xy` is unused by the 2D shader because the canonical quad always has
    /// `local z = 0`, so only the line constructors write flags there: `x` and
    /// `y` are the **per-cap hard flags** (`1.0` = that cap is hard / no AA,
    /// `0.0` = normal AA) for [`ShapeInstance::line_with_cap_aa`]. Every other
    /// constructor leaves both at `0.0`, so the shader path is unchanged.
    pub transform_c2: [f32; 4],
    /// Fourth column of the model matrix (translation; `.z` carries depth).
    pub transform_c3: [f32; 4],
    /// Linear RGBA colour (see the module colour-space contract).
    pub color: [f32; 4],
    /// `[border_thickness, corner_radius, min_px, shape_kind]`.
    ///
    /// `x`/`y` keep their historical meaning (border, corner radius). `z` is
    /// the **minimum stroke width in logical pixels** (`0.0` = no floor) used
    /// only by line instances; it replaces the old AA `smoothing` value, since
    /// the 1-pixel box coverage filter no longer needs a smoothing width.
    /// `w` is the shape-kind flag: `0.0`/`1.0` are the legacy `is_filled`
    /// values for the rounded-rect/circle family (ignored by the shader) and
    /// [`SHAPE_KIND_TRIANGLE`] selects the triangle SDF.
    ///
    /// For the triangle kind, `x` is repurposed as the **edge-AA bitmask**
    /// (bits [`TRIANGLE_EDGE_AB`] / [`TRIANGLE_EDGE_BC`] / [`TRIANGLE_EDGE_CA`]);
    /// see [`ShapeInstance::polygon_triangle`]. The rounded-rect/circle path
    /// never sees a triangle instance, so there is no collision.
    pub params: [f32; 4],
    /// Extended data (joined segment: `n_start.xy`, `n_end.xy`).
    ///
    /// For [`ShapeInstance::joined_segment`] these are the per-end joint plane
    /// normals in world space. `(0, 0)` means "free cap" on that end. Other
    /// constructors leave it zeroed.
    pub ext0: [f32; 4],
    /// Segment distance at the start point `a` along the polyline (dash phase).
    pub ext1: f32,
}

impl ShapeInstance {
    /// Vertex-buffer layout describing this instance type to wgpu.
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ShapeInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
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
            ext0: [0.0; 4],
            ext1: 0.0,
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
        // params: border=0, corner_radius=radius (círculo perfecto), min_px=0, filled=1
        Self::from_model(model, color, [0.0, radius, 0.0, 1.0])
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
        // params: border=0, corner_radius=0, min_px=0, filled=1
        Self::from_model(model, color, [0.0, 0.0, 0.0, 1.0])
    }

    /// Rectángulo con esquinas redondeadas relleno.
    pub fn rounded_rect(
        center: glam::Vec2,
        size: glam::Vec2,
        corner_radius: f32,
        color: [f32; 4],
    ) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        Self::from_model(model, color, [0.0, corner_radius, 0.0, 1.0])
    }

    /// Solo el borde (outline) de un rectángulo, sin relleno.
    pub fn rect_outline(
        center: glam::Vec2,
        size: glam::Vec2,
        thickness: f32,
        color: [f32; 4],
    ) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        // params: border=thickness, corner_radius=0, min_px=0, filled=0
        Self::from_model(model, color, [thickness, 0.0, 0.0, 0.0])
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
    pub fn line_with_cap(
        from: glam::Vec2,
        to: glam::Vec2,
        width: f32,
        color: [f32; 4],
        cap_style: u8,
    ) -> Self {
        Self::line_with_cap_aa(from, to, width, color, cap_style, true, true)
    }

    /// Línea con control de AA **por extremo**, para teselar muros conectados
    /// sin costura.
    ///
    /// Es idéntica a [`ShapeInstance::line_with_cap`] salvo que `aa_start` /
    /// `aa_end` desactivan el anti-aliasing del cap `from` / `to`
    /// respectivamente:
    ///
    /// - `true` (default): el cap recibe AA normal (extremo libre).
    /// - `false`: el cap es **duro** — el shader recorta exactamente en el
    ///   plano del extremo, sin fade, y satura la cobertura hacia adentro. Dos
    ///   segmentos contiguos que comparten un extremo, ambos con ese extremo en
    ///   `false`, teselan borde con borde sin dejar hueco ni doble-blend.
    ///
    /// `cap_style` usa el mismo `u8` que [`ShapeInstance::line_with_cap`]
    /// (`0` = Flat, `1` = Round, `2` = Square).
    ///
    /// # Cómo marcar muros conectados (p.ej. Koncreta)
    ///
    /// Al construir una cadena/miter/T de segmentos, marcá `aa_start`/`aa_end`
    /// en `false` para los extremos que se unen con otro segmento y en `true`
    /// para los extremos libres (esquinas exteriores). La codificación de los
    /// flags vive en `transform_c2.xy` (slot no usado por el shader 2D); el
    /// resto de constructores lo dejan en `0`. Las funciones puramente
    /// geométricas no cambian.
    pub fn line_with_cap_aa(
        from: glam::Vec2,
        to: glam::Vec2,
        width: f32,
        color: [f32; 4],
        cap_style: u8,
        aa_start: bool,
        aa_end: bool,
    ) -> Self {
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

        let mut model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(geom_length, width, 1.0),
            glam::Quat::from_rotation_z(rotation),
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        // transform_c2.xy is unused by the 2D shader (local z is always 0), so
        // it carries the per-cap hard flags: 1.0 = disable AA on that cap.
        model.z_axis.x = if aa_start { 0.0 } else { 1.0 };
        model.z_axis.y = if aa_end { 0.0 } else { 1.0 };
        // z is only used as a "is a stroked line" marker: `ShapeGpu::pack`
        // maps `c2.z < 0.5` to `SHAPE_GPU_LINE`, which enables the minimum
        // screen-width floor. Rect / circle constructors keep c2.z = 1.
        model.z_axis.z = 0.0;
        // params: border=width, corner_radius=radius, min_px=0, filled=1
        Self::from_model(model, color, [width, radius, 0.0, 1.0])
    }

    /// Line with a fixed **minimum** width in logical pixels (`min_px`).
    ///
    /// `width_world` is still the real width in world units (metres in
    /// Koncreta); the shader renders `max(width_world, min_px · device px)`
    /// without touching opacity. This is the constructor used by the CAD /
    /// wall pipelines (hybrid lineweight mode).
    pub fn line_px(
        from: glam::Vec2,
        to: glam::Vec2,
        width_world: f32,
        min_px: f32,
        color: [f32; 4],
        cap_style: u8,
    ) -> Self {
        Self::line_with_cap(from, to, width_world, color, cap_style).with_min_px(min_px)
    }

    /// Sets the minimum stroke width in logical pixels (`0.0` disables the
    /// floor). Only affects instances created by a `line*` constructor, since
    /// the shader gates the widening on `SHAPE_GPU_LINE`.
    pub fn with_min_px(mut self, min_px: f32) -> Self {
        self.params[2] = if min_px.is_finite() {
            min_px.max(0.0)
        } else {
            0.0
        };
        self
    }

    /// One segment of a joined polyline / polygon stroke.
    ///
    /// Encoding (see `shape_2d.wgsl`):
    /// - `c0 = b - a` (world vector), `c3 = midpoint`;
    /// - `dims.x = width_world`, `dims.y = cap_style` (0 flat, 1 round, 2 square);
    /// - `ext0 = (n_start.xy, n_end.xy)` joint plane normals (`(0,0)` = cap);
    /// - `ext1 = s0` accumulated polyline length at `a` (dash phase).
    ///
    /// `round_start` / `round_end` select a round join instead of a miter on
    /// that end (used when the miter limit is exceeded). A round **join** still
    /// needs `n_*` to split the joint disc exactly with the neighbour segment.
    /// Free caps use the `cap_style`.
    ///
    /// `dash_id` indexes the shader dash table (0 = continuous).
    #[allow(clippy::too_many_arguments)]
    pub fn joined_segment(
        a: glam::Vec2,
        b: glam::Vec2,
        width_world: f32,
        min_px: f32,
        color: [f32; 4],
        cap_style: u8,
        n_start: glam::Vec2,
        n_end: glam::Vec2,
        round_start: bool,
        round_end: bool,
        s0: f32,
        dash_id: u8,
    ) -> Self {
        let delta = b - a;
        let len = delta.length();
        if len < 1e-6 {
            return Self::circle(a, width_world.max(0.001) * 0.5, color);
        }
        let mid = (a + b) * 0.5;
        Self {
            transform_c0: [delta.x, delta.y, 0.0, if round_start { 1.0 } else { 0.0 }],
            transform_c1: [0.0, 0.0, 0.0, if round_end { 1.0 } else { 0.0 }],
            // c2.w = joined-segment marker (c2.z stays 0: also a stroked line).
            transform_c2: [0.0, 0.0, 0.0, 1.0],
            transform_c3: [mid.x, mid.y, 0.0, dash_id as f32],
            color,
            params: [
                width_world.max(0.0),
                cap_style as f32,
                if min_px.is_finite() {
                    min_px.max(0.0)
                } else {
                    0.0
                },
                1.0,
            ],
            ext0: [n_start.x, n_start.y, n_end.x, n_end.y],
            ext1: s0,
        }
    }

    /// Whether this is a joined-segment instance.
    pub fn is_joined(&self) -> bool {
        let kind = self.transform_c2[3];
        kind > 0.5 && kind < 1.5
    }

    /// Whether this is an analytic arc/circle instance.
    pub fn is_arc(&self) -> bool {
        let kind = self.transform_c2[3];
        kind > 1.5 && kind < 2.5
    }

    /// Whether this is a ring (outline) instance.
    pub fn is_ring(&self) -> bool {
        self.transform_c2[3] > 2.5
    }

    /// Marks the dash table lengths (`dash_id`) as **logical px** instead of
    /// world units. Only meaningful for joined/arc instances (elevation and
    /// section hidden lines).
    pub fn with_dash_px(mut self) -> Self {
        self.transform_c1[2] = 1.0;
        self
    }

    /// Analytic circular arc / circle stroke.
    ///
    /// Encoding: `c3 = centre`, `dims.x = radius`, `dims.y = width`,
    /// `ext0 = (start_angle, sweep, cap_style, 0)`, `ext1 = s0` (dash phase).
    /// A full circle uses `sweep = TAU`. The shader evaluates
    /// `abs(|p-c| - radius) - half_w`, so no tessellation facets appear at any
    /// zoom. Free caps are Flat/Round/Square.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        center: glam::Vec2,
        radius: f32,
        start: f32,
        sweep: f32,
        width_world: f32,
        min_px: f32,
        color: [f32; 4],
        cap_style: u8,
        s0: f32,
        dash_id: u8,
    ) -> Self {
        Self {
            transform_c0: [0.0; 4],
            transform_c1: [0.0; 4],
            transform_c2: [0.0, 0.0, 0.0, 2.0],
            transform_c3: [center.x, center.y, 0.0, dash_id as f32],
            color,
            params: [
                radius.max(0.0),
                width_world.max(0.0),
                if min_px.is_finite() {
                    min_px.max(0.0)
                } else {
                    0.0
                },
                1.0,
            ],
            ext0: [start, sweep, cap_style as f32, 0.0],
            ext1: s0,
        }
    }

    /// Turns a circle/rect instance into a real hollow ring of `thickness`.
    pub fn with_ring(mut self, thickness: f32, min_px: f32) -> Self {
        self.params[0] = thickness.max(0.0);
        self.params[2] = if min_px.is_finite() {
            min_px.max(0.0)
        } else {
            0.0
        };
        self.transform_c2[3] = 3.0;
        self
    }

    /// Real circular outline (ring), not a filled disc.
    pub fn circle_ring(center: glam::Vec2, radius: f32, thickness: f32, color: [f32; 4]) -> Self {
        Self::circle(center, radius, color).with_ring(thickness, 1.0)
    }

    /// Real rectangular outline (ring), not a filled rectangle.
    pub fn rect_ring(
        center: glam::Vec2,
        size: glam::Vec2,
        thickness: f32,
        color: [f32; 4],
    ) -> Self {
        Self::rect(center, size, color).with_ring(thickness, 1.0)
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
        Self::line_with_cap_style_aa(from, to, width, color, cap_style, true, true)
    }

    /// Tipada de [`ShapeInstance::line_with_cap_aa`]: igual que
    /// [`ShapeInstance::line_with_cap_style`] pero con `aa_start` / `aa_end`
    /// para desactivar el AA de caps conectados (ver esa función para el
    /// detalle y para cómo marcar cadenas de muros).
    ///
    /// ```rust
    /// use ferrous_2d::components::CapStyle;
    /// use ferrous_2d::render::ShapeInstance;
    /// use glam::Vec2;
    ///
    /// // Cadena horizontal: el extremo derecho se une a otro muro (duro),
    /// // el izquierdo queda libre (AA).
    /// let wall = ShapeInstance::line_with_cap_style_aa(
    ///     Vec2::ZERO,
    ///     Vec2::new(10.0, 0.0),
    ///     2.0,
    ///     [1.0, 1.0, 1.0, 1.0],
    ///     CapStyle::Flat,
    ///     true,
    ///     false,
    /// );
    /// ```
    pub fn line_with_cap_style_aa(
        from: glam::Vec2,
        to: glam::Vec2,
        width: f32,
        color: [f32; 4],
        cap_style: crate::components::CapStyle,
        aa_start: bool,
        aa_end: bool,
    ) -> Self {
        let raw = match cap_style {
            crate::components::CapStyle::Flat => 0,
            crate::components::CapStyle::Round => 1,
            crate::components::CapStyle::Square => 2,
        };
        Self::line_with_cap_aa(from, to, width, color, raw, aa_start, aa_end)
    }

    /// Rectángulo rotado, centrado en `center`, con ángulo en radianes.
    pub fn rect_rotated(
        center: glam::Vec2,
        size: glam::Vec2,
        angle_radians: f32,
        color: [f32; 4],
    ) -> Self {
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(size.x, size.y, 1.0),
            glam::Quat::from_rotation_z(angle_radians),
            glam::Vec3::new(center.x, center.y, 0.0),
        );
        Self::from_model(model, color, [0.0, 0.0, 0.0, 1.0])
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
    /// `params` emitido: `[edge_aa_mask=0b111, 0, min_px=0,
    /// SHAPE_KIND_TRIANGLE]`. Equivalente a
    /// [`ShapeInstance::polygon_triangle`] con [`TRIANGLE_EDGES_ALL`]. Usar
    /// triángulos en abanico (fan) desde el centroide es la forma directa de
    /// rellenar un polígono convexo con un vértice por lado, en vez de
    /// aproximarlo con muchas instancias de rect.
    pub fn triangle(a: glam::Vec2, b: glam::Vec2, c: glam::Vec2, color: [f32; 4]) -> Self {
        Self::polygon_triangle(a, b, c, color, TRIANGLE_EDGES_ALL)
    }

    /// Triángulo **relleno** con anti-aliasing **selectivo por arista**, pensado
    /// para triangulaciones que deben verse como un relleno continuo.
    ///
    /// `edge_aa_mask` bits (ver [`TRIANGLE_EDGE_AB`], [`TRIANGLE_EDGE_BC`],
    /// [`TRIANGLE_EDGE_CA`]):
    /// - bit 0 = arista `a → b`
    /// - bit 1 = arista `b → c`
    /// - bit 2 = arista `c → a`
    ///
    /// Bit en 1 = esa arista recibe AA (silueta exterior). Bit en 0 = arista
    /// dura: el fragment shader recorta exactamente en la línea de la arista
    /// sin atenuar cobertura. Para un borde **interno** compartido por dos
    /// triángulos, ambos lo marcan en 0, de modo que cada uno rellena hasta la
    /// línea a cobertura plena; los dos triángulos teselan sin dejar la costura
    /// clara que produce el AA duplicado.
    ///
    /// El AA de los bordes marcados usa la distancia firmada al segmento
    /// (con clamp a los vértices), de modo que dos bordes marcados concurrentes
    /// redondean la esquina y el resultado sigue siendo winding-agnostic
    /// (two-sided). `edge_aa_mask = 0` produce un triángulo de bordes duros
    /// (sin AA); `edge_aa_mask = 0b111` es idéntico a
    /// [`ShapeInstance::triangle`].
    ///
    /// # Cómo marcar una triangulación (p.ej. Koncreta)
    ///
    /// Una arista es **externa** si pertenece a un único triángulo de la
    /// triangulación; es **interna** si dos triángulos la comparten. Marcá en 1
    /// solo las aristas externas y en 0 las internas. El borde exterior queda
    /// suavizado y las diagonales internas duras.
    pub fn polygon_triangle(
        a: glam::Vec2,
        b: glam::Vec2,
        c: glam::Vec2,
        color: [f32; 4],
        edge_aa_mask: u8,
    ) -> Self {
        let model = glam::Mat4::from_cols(
            glam::Vec4::new(b.x - a.x, b.y - a.y, 0.0, 0.0),
            glam::Vec4::new(c.x - a.x, c.y - a.y, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, 1.0, 0.0),
            glam::Vec4::new(a.x, a.y, 0.0, 1.0),
        );
        let mask = (edge_aa_mask & TRIANGLE_EDGES_ALL) as f32;
        Self::from_model(model, color, [mask, 0.0, 0.0, SHAPE_KIND_TRIANGLE])
    }
}

impl ShapeInstance {
    /// Whether this instance uses the triangle SDF ([`SHAPE_KIND_TRIANGLE`]).
    #[inline]
    pub fn is_triangle(&self) -> bool {
        self.params[3] > 1.5
    }

    /// World-space AABB `(min, max)` of the canonical quad.
    ///
    /// Conservative for culling but not the AA-padded bounds: the shader's
    /// anti-aliasing band extends a few pixels past it, so callers expand the
    /// cull rect by a pixel margin before testing.
    #[inline]
    pub fn world_aabb(&self) -> (glam::Vec2, glam::Vec2) {
        let c0 = glam::Vec2::new(self.transform_c0[0], self.transform_c0[1]);
        let c1 = glam::Vec2::new(self.transform_c1[0], self.transform_c1[1]);
        let c3 = glam::Vec2::new(self.transform_c3[0], self.transform_c3[1]);
        if self.is_joined() {
            // Endpoints a/b plus the miter wedge (max ratio 4) and the stroke
            // half-width. The AA / min_px margin is added by the caller.
            let a = c3 - c0 * 0.5;
            let b = c3 + c0 * 0.5;
            let pad = glam::Vec2::splat((self.params[0].abs() * 2.5).max(1e-6));
            return (a.min(b) - pad, a.max(b) + pad);
        }
        if self.is_arc() {
            // Full-circle bound: centre ± (radius + half width).
            let reach = self.params[0].abs() + self.params[1].abs() * 0.5 + 1e-6;
            let pad = glam::Vec2::splat(reach);
            return (c3 - pad, c3 + pad);
        }
        // Triangles map [0,1]² (origin = vertex a); the rest map [-0.5,0.5]².
        let origin = if self.is_triangle() {
            c3
        } else {
            c3 - (c0 + c1) * 0.5
        };
        let ext = c0.abs() + c1.abs();
        let min = origin + c0.min(glam::Vec2::ZERO) + c1.min(glam::Vec2::ZERO);
        (min, min + ext)
    }
}

/// Compact GPU-side encoding of a [`ShapeInstance`] (64 bytes instead of 120).
///
/// [`ShapeInstance`] stays the authoring type; the renderer packs it into this
/// layout right before upload, so callers never build it by hand. What is kept:
///
/// - the 2D affine part of the model matrix (`c0.xy`, `c1.xy`, `c3.xy`) — the
///   2D shader never used the z row/column, and depth ordering is CPU-side;
/// - `border` / `radius` in full `f32` (world units can be large or tiny);
/// - the linear colour as four `f16` (11-bit precision, HDR-safe);
/// - every flag in one `u32`:
///
/// | bits    | meaning |
/// |---------|---------|
/// | 0..=15  | minimum line width in logical px, unsigned 8.8 fixed point |
/// | 16      | triangle kind ([`SHAPE_KIND_TRIANGLE`]) |
/// | 17      | hollow ring / real outline (`ShapeInstance::with_ring`) |
/// | 18..=20 | triangle edge-AA mask |
/// | 21      | hard start cap ([`ShapeInstance::line_with_cap_aa`]) |
/// | 22      | hard end cap |
/// | 23      | stroked line ([`ShapeInstance::line_px`], min-width floor) |
/// | 24      | joined segment ([`ShapeInstance::joined_segment`]) |
/// | 25      | start end is a round join (not miter) |
/// | 26      | end end is a round join (not miter) |
/// | 27      | analytic arc/circle (`ShapeInstance::arc`) |
/// | 28..=31 | dash pattern index (`0` = continuous) |
///
/// Bits 0..=15 used to hold the AA `smoothing` value. The 1-pixel box coverage
/// filter no longer needs it, so the slot now carries the per-instance minimum
/// width (resolved to world units by the shader using the camera's
/// device-pixels-per-logical-pixel ratio).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ShapeGpu {
    /// Model x basis (`transform_c0.xy`).
    pub c0: [f32; 2],
    /// Model y basis (`transform_c1.xy`).
    pub c1: [f32; 2],
    /// Translation (`transform_c3.xy`).
    pub c3: [f32; 2],
    /// `[border_thickness, corner_radius]` (border is 0 for triangles).
    pub dims: [f32; 2],
    /// Linear RGBA as IEEE half floats.
    pub color: [u16; 4],
    /// Packed min width + kind + masks (see the type docs).
    pub flags: u32,
    /// Joined segment: `(n_start.xy, n_end.xy)` joint plane normals.
    pub ext0: [f32; 4],
    /// Joined segment: accumulated polyline length at the start (dash phase).
    pub ext1: f32,
}

/// `ShapeGpu::flags` bit: triangle kind.
pub const SHAPE_GPU_TRIANGLE: u32 = 1 << 16;
/// `ShapeGpu::flags` shift of the 3-bit triangle edge-AA mask.
pub const SHAPE_GPU_MASK_SHIFT: u32 = 18;
/// `ShapeGpu::flags` bit: hard start cap.
pub const SHAPE_GPU_CAP_START_HARD: u32 = 1 << 21;
/// `ShapeGpu::flags` bit: hard end cap.
pub const SHAPE_GPU_CAP_END_HARD: u32 = 1 << 22;
/// `ShapeGpu::flags` bit: stroked line (enables the min-width floor).
pub const SHAPE_GPU_LINE: u32 = 1 << 23;
/// Mask of the 8.8 fixed-point minimum line width in `ShapeGpu::flags`.
pub const SHAPE_GPU_MIN_PX_MASK: u32 = 0xFFFF;
/// `ShapeGpu::flags` bit: joined segment (per-end joint planes in `ext0`).
pub const SHAPE_GPU_JOINED: u32 = 1 << 24;
/// `ShapeGpu::flags` bit: the start end uses a round join.
pub const SHAPE_GPU_JOIN_ROUND_START: u32 = 1 << 25;
/// `ShapeGpu::flags` bit: the end end uses a round join.
pub const SHAPE_GPU_JOIN_ROUND_END: u32 = 1 << 26;
/// `ShapeGpu::flags` bit: analytic arc/circle stroke.
pub const SHAPE_GPU_ARC: u32 = 1 << 27;
/// `ShapeGpu::flags` bit: dash pattern lengths are logical px (not world).
pub const SHAPE_GPU_DASH_PX: u32 = 1 << 18;
/// `ShapeGpu::flags` bit: hollow ring (real outline).
pub const SHAPE_GPU_RING: u32 = 1 << 17;
/// Shift of the 4-bit dash pattern index in `ShapeGpu::flags`.
pub const SHAPE_GPU_DASH_SHIFT: u32 = 28;
/// Upper bound (logical px) the shader accepts for the per-instance minimum
/// width. CPU culling uses it to size the AA/widening margin.
pub const MAX_MIN_PX_LOGICAL: f32 = 8.0;

impl ShapeGpu {
    /// Vertex-buffer layout describing this instance type to wgpu.
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ShapeGpu>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float16x4,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: 44,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 60,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }

    /// Pack an authoring [`ShapeInstance`] into the GPU layout.
    #[inline]
    pub fn pack(s: &ShapeInstance) -> Self {
        let min_px = if s.params[2].is_finite() {
            (s.params[2].max(0.0) * 256.0 + 0.5) as u32
        } else {
            0
        };
        let mut flags = min_px.min(SHAPE_GPU_MIN_PX_MASK);
        let mut border = s.params[0];
        if s.is_triangle() {
            // Same decode as the old shader: `u32(params.x + 0.5)`, >= 7 = all.
            let mask = ((s.params[0].max(0.0) + 0.5) as u32).min(7);
            flags |= SHAPE_GPU_TRIANGLE | (mask << SHAPE_GPU_MASK_SHIFT);
            border = 0.0;
        } else if s.is_joined() {
            flags |= SHAPE_GPU_JOINED | SHAPE_GPU_LINE;
            if s.transform_c0[3] > 0.5 {
                flags |= SHAPE_GPU_JOIN_ROUND_START;
            }
            if s.transform_c1[3] > 0.5 {
                flags |= SHAPE_GPU_JOIN_ROUND_END;
            }
            let dash = s.transform_c3[3].clamp(0.0, 15.0) as u32;
            flags |= dash << SHAPE_GPU_DASH_SHIFT;
            if s.transform_c1[2] > 0.5 {
                flags |= SHAPE_GPU_DASH_PX;
            }
        } else if s.is_arc() {
            flags |= SHAPE_GPU_ARC | SHAPE_GPU_LINE;
            let dash = s.transform_c3[3].clamp(0.0, 15.0) as u32;
            flags |= dash << SHAPE_GPU_DASH_SHIFT;
            if s.transform_c1[2] > 0.5 {
                flags |= SHAPE_GPU_DASH_PX;
            }
        } else if s.is_ring() {
            flags |= SHAPE_GPU_RING;
        } else {
            if s.transform_c2[0] > 0.5 {
                flags |= SHAPE_GPU_CAP_START_HARD;
            }
            if s.transform_c2[1] > 0.5 {
                flags |= SHAPE_GPU_CAP_END_HARD;
            }
            // `line*` constructors clear c2.z; every other constructor keeps 1.
            if s.transform_c2[2] < 0.5 {
                flags |= SHAPE_GPU_LINE;
            }
        }
        Self {
            c0: [s.transform_c0[0], s.transform_c0[1]],
            c1: [s.transform_c1[0], s.transform_c1[1]],
            c3: [s.transform_c3[0], s.transform_c3[1]],
            dims: [border, s.params[1]],
            color: [
                f32_to_f16(s.color[0]),
                f32_to_f16(s.color[1]),
                f32_to_f16(s.color[2]),
                f32_to_f16(s.color[3]),
            ],
            flags,
            ext0: s.ext0,
            ext1: s.ext1,
        }
    }
}

impl From<&ShapeInstance> for ShapeGpu {
    #[inline]
    fn from(s: &ShapeInstance) -> Self {
        Self::pack(s)
    }
}

/// IEEE 754 binary32 → binary16, round-to-nearest-even (no external crate).
#[inline]
pub fn f32_to_f16(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mant = x & 0x007f_ffff;
    if exp == 0xff {
        // Inf / NaN (keep NaN quiet).
        return sign | 0x7c00 | if mant != 0 { 0x0200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mant | 0x0080_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rem = m & ((1u32 << shift) - 1);
        let halfway = 1u32 << (shift - 1);
        let r = if rem > halfway || (rem == halfway && (half & 1) == 1) {
            half + 1
        } else {
            half
        };
        return sign | r as u16;
    }
    let half = ((e as u32) << 10) | (mant >> 13);
    let rem = mant & 0x1fff;
    // A carry out of the mantissa correctly bumps the exponent (up to inf).
    let r = if rem > 0x1000 || (rem == 0x1000 && (half & 1) == 1) {
        half + 1
    } else {
        half
    };
    sign | r as u16
}

#[cfg(test)]
mod gpu_pack_tests {
    use super::*;

    #[test]
    fn shape_gpu_is_64_bytes() {
        assert_eq!(std::mem::size_of::<ShapeGpu>(), 64);
    }

    /// `CameraUniform` in the 2D shaders must match this layout exactly:
    /// mat4 (64) + resolution (8) + padding (8) + viewport_origin (8) + _pad
    /// (8) + 32 `vec4` dash slots (512) = 608 bytes, 16-byte aligned.
    #[test]
    fn uniform2d_matches_the_wgsl_camera_layout() {
        assert_eq!(std::mem::size_of::<Uniform2d>(), 608);
        assert_eq!(std::mem::align_of::<Uniform2d>(), 4);
        let origin = std::mem::offset_of!(Uniform2d, viewport_origin);
        assert_eq!(origin, 80);
        assert_eq!(std::mem::offset_of!(Uniform2d, dash), 96);
    }

    #[test]
    fn f16_known_values() {
        assert_eq!(f32_to_f16(0.0), 0x0000);
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(-2.0), 0xc000);
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(65504.0), 0x7bff);
        assert_eq!(f32_to_f16(1.0e6), 0x7c00);
        assert_eq!(f32_to_f16(5.960_464_5e-8), 0x0001);
    }

    #[test]
    fn pack_keeps_flags() {
        let t = ShapeInstance::polygon_triangle(
            glam::Vec2::ZERO,
            glam::Vec2::X,
            glam::Vec2::Y,
            [1.0; 4],
            TRIANGLE_EDGE_AB,
        );
        let g = ShapeGpu::pack(&t);
        assert_ne!(g.flags & SHAPE_GPU_TRIANGLE, 0);
        assert_eq!((g.flags >> SHAPE_GPU_MASK_SHIFT) & 7, 1);
        // Bits 0..=15 now hold min_px (8.8 fixed). Triangles do not ask for a
        // minimum stroke width, so they pack 0 here.
        assert_eq!(g.flags & SHAPE_GPU_MIN_PX_MASK, 0);
        assert_eq!(g.flags & SHAPE_GPU_LINE, 0);

        let l = ShapeInstance::line_with_cap_aa(
            glam::Vec2::ZERO,
            glam::Vec2::new(5.0, 0.0),
            1.0,
            [1.0; 4],
            0,
            false,
            true,
        );
        let g = ShapeGpu::pack(&l);
        assert_eq!(g.flags & SHAPE_GPU_TRIANGLE, 0);
        assert_ne!(g.flags & SHAPE_GPU_CAP_START_HARD, 0);
        assert_eq!(g.flags & SHAPE_GPU_CAP_END_HARD, 0);
        assert_eq!(g.flags & SHAPE_GPU_MIN_PX_MASK, 0);
        assert_ne!(g.flags & SHAPE_GPU_LINE, 0);
        assert_eq!(g.dims[0], 1.0);
    }

    #[test]
    fn pack_line_min_px_and_line_flag() {
        let l = ShapeInstance::line_px(
            glam::Vec2::ZERO,
            glam::Vec2::new(10.0, 0.0),
            0.015,
            1.0,
            [1.0; 4],
            0,
        );
        let g = ShapeGpu::pack(&l);
        assert_ne!(g.flags & SHAPE_GPU_LINE, 0);
        assert_eq!(g.flags & SHAPE_GPU_MIN_PX_MASK, 256); // 1.0 in 8.8

        // Rect / circle constructors must not be treated as lines.
        let r = ShapeInstance::rect(glam::Vec2::ZERO, glam::Vec2::new(2.0, 1.0), [1.0; 4]);
        assert_eq!(ShapeGpu::pack(&r).flags & SHAPE_GPU_LINE, 0);

        // `with_min_px` clamps negatives / NaN to zero.
        let l = l.with_min_px(-3.0);
        assert_eq!(ShapeGpu::pack(&l).flags & SHAPE_GPU_MIN_PX_MASK, 0);
        let l = l.with_min_px(f32::NAN);
        assert_eq!(ShapeGpu::pack(&l).flags & SHAPE_GPU_MIN_PX_MASK, 0);
    }

    #[test]
    fn pack_joined_segment_flags_ext_and_dash() {
        let j = ShapeInstance::joined_segment(
            glam::Vec2::ZERO,
            glam::Vec2::new(10.0, 0.0),
            0.02,
            1.0,
            [1.0; 4],
            0,
            glam::Vec2::new(0.707, 0.707),
            glam::Vec2::new(0.707, -0.707),
            false,
            true,
            2.5,
            3,
        );
        let g = ShapeGpu::pack(&j);
        assert_ne!(g.flags & SHAPE_GPU_JOINED, 0);
        assert_ne!(g.flags & SHAPE_GPU_JOIN_ROUND_END, 0);
        assert_eq!(g.flags & SHAPE_GPU_JOIN_ROUND_START, 0);
        assert_eq!((g.flags >> SHAPE_GPU_DASH_SHIFT) & 15, 3);
        assert_eq!(g.flags & SHAPE_GPU_MIN_PX_MASK, 256);
        assert_eq!(g.dims, [0.02, 0.0]);
        assert_eq!(g.ext0[0], 0.707);
        assert_eq!(g.ext0[3], -0.707);
        assert_eq!(g.ext1, 2.5);
        let (min, max) = j.world_aabb();
        assert!(min.x <= 0.0 && max.x >= 10.0);
        // `0.02 * 2.5` rounds to just under `0.05` in f32, so compare with a
        // tolerance rather than against the decimal literal.
        assert!(min.y <= -0.05 + 1e-6 && max.y >= 0.05 - 1e-6);
    }

    #[test]
    fn pack_arc_and_ring_flags() {
        let a = ShapeInstance::arc(
            glam::Vec2::new(3.0, 4.0),
            10.0,
            0.5,
            std::f32::consts::TAU,
            0.05,
            1.0,
            [1.0; 4],
            1,
            0.0,
            2,
        );
        let g = ShapeGpu::pack(&a);
        assert_ne!(g.flags & SHAPE_GPU_ARC, 0);
        assert_eq!(g.flags & SHAPE_GPU_RING, 0);
        assert_eq!(g.dims, [10.0, 0.05]);
        assert_eq!(g.ext0[0], 0.5);
        assert_eq!(g.ext0[1], std::f32::consts::TAU);
        assert_eq!((g.flags >> SHAPE_GPU_DASH_SHIFT) & 15, 2);

        let r = ShapeInstance::circle_ring(glam::Vec2::ZERO, 5.0, 0.02, [1.0; 4]);
        let g = ShapeGpu::pack(&r);
        assert_ne!(g.flags & SHAPE_GPU_RING, 0);
        assert_eq!(g.flags & SHAPE_GPU_ARC, 0);
        assert_eq!(g.dims, [0.02, 5.0]);
        let (min, max) = r.world_aabb();
        assert!(min.x <= -5.0 && max.x >= 5.0);
    }

    #[test]
    fn world_aabb_matches_corners() {
        let r = ShapeInstance::rect_rotated(
            glam::Vec2::new(3.0, 4.0),
            glam::Vec2::new(2.0, 1.0),
            0.7,
            [1.0; 4],
        );
        let (min, max) = r.world_aabb();
        let c0 = glam::Vec2::new(r.transform_c0[0], r.transform_c0[1]);
        let c1 = glam::Vec2::new(r.transform_c1[0], r.transform_c1[1]);
        let c3 = glam::Vec2::new(r.transform_c3[0], r.transform_c3[1]);
        for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
            let p = c3 + c0 * x + c1 * y;
            assert!(p.x >= min.x - 1e-5 && p.y >= min.y - 1e-5);
            assert!(p.x <= max.x + 1e-5 && p.y <= max.y + 1e-5);
        }
    }
}

/// Parameters for the procedural, camera-following 2D background grid.
///
/// The grid is evaluated per fragment from the inverse view-projection, so it
/// is truly infinite, has constant cost and never needs CPU geometry. Widths
/// are in **logical pixels**; colours are linear RGBA.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Grid2dParams {
    /// Base (user) minor spacing in world metres.
    pub base_step: f32,
    /// Major line every `major_every` minor steps (>= 1).
    pub major_every: f32,
    pub minor_width_px: f32,
    pub major_width_px: f32,
    pub axis_width_px: f32,
    pub minor_color: [f32; 4],
    pub major_color: [f32; 4],
    /// Colour of the horizontal axis (y = 0).
    pub axis_x_color: [f32; 4],
    /// Colour of the vertical axis (x = 0).
    pub axis_y_color: [f32; 4],
    /// Draw the coloured axes at world x = 0 / y = 0.
    pub axes: bool,
}

impl Default for Grid2dParams {
    fn default() -> Self {
        Self {
            base_step: 1.0,
            major_every: 5.0,
            minor_width_px: 1.0,
            major_width_px: 1.0,
            axis_width_px: 1.5,
            minor_color: [0.35, 0.42, 0.55, 0.14],
            major_color: [0.50, 0.65, 0.85, 0.26],
            axis_x_color: [0.88, 0.28, 0.28, 0.50],
            axis_y_color: [0.28, 0.80, 0.38, 0.50],
            axes: true,
        }
    }
}

/// GPU uniform for `grid_2d.wgsl` (176 bytes, 16-byte aligned).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct Grid2dUniform {
    pub inv_view_proj: [f32; 16],
    pub resolution: [f32; 2],
    /// Align `viewport` (WGSL `vec4`) to 16 bytes.
    pub _pad0: [f32; 2],
    /// Pixel viewport `[x, y, w, h]` (multi-view uses sub-rects).
    pub viewport: [f32; 4],
    pub world_per_px: f32,
    pub dpp: f32,
    /// Align `params` (WGSL `vec4`) to 16 bytes.
    pub _pad1: [f32; 2],
    /// `[base_step, major_every, minor_width_px, major_width_px]`.
    pub params: [f32; 4],
    /// `[axis_width_px, flags, 0, 0]`.
    pub params2: [f32; 4],
    pub minor_color: [f32; 4],
    pub major_color: [f32; 4],
    pub axis_x_color: [f32; 4],
    pub axis_y_color: [f32; 4],
}

/// Shared uniform data for 2D passes.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniform2d {
    /// Combined view-projection matrix, column-major.
    pub view_proj: [f32; 16],
    /// Viewport size in physical pixels `[width, height]`.
    pub resolution: [f32; 2],
    /// `[device_px_per_logical_px, legacy_2d_flag]`.
    pub padding: [f32; 2], // 16-byte alignment
    /// Physical-pixel origin `[x, y]` of the viewport inside the framebuffer.
    ///
    /// The single main camera uses `[0, 0]`; multi-view uses the sub-rect
    /// origin so the fragment shader can map clip coordinates (which are in
    /// full framebuffer space) onto the view's own resolution.
    pub viewport_origin: [f32; 2],
    /// Keeps `dash` (a `vec4` array) 16-byte aligned after the origin above.
    pub _pad: [f32; 2],
    /// Dash table: 16 patterns × 8 lengths, stored as 32 `vec4` so the WGSL
    /// uniform array is 16-byte aligned. Slot `0` is unused (continuous).
    /// Pattern `i` uses `dash[i*2]` (lengths 0..3) and `dash[i*2+1]` (4..7).
    pub dash: [[f32; 4]; 32],
}

/// Maximum nesting depth of Pure2D offscreen layers.
///
/// Pushing a layer when this many are already open returns
/// [`LayerError::TooDeep`] and changes nothing.
pub const MAX_LAYER_DEPTH_2D: u32 = 8;

/// Description of one offscreen compositing layer (see
/// `Renderer::push_layer_2d`).
///
/// Draws between [`Layer2d`]'s push and pop are z-sorted among themselves and
/// rendered into a transparent offscreen texture; the whole layer is then
/// composited into its parent as one command at `depth`, with `blend` and
/// group `opacity` (`colour× opacity`, all four premultiplied channels).
///
/// Group opacity differs from per-child opacity: two overlapping opaque
/// children inside a 50 % layer show no internal overlap, because they are
/// composited inside the layer at full opacity and the layer as a whole is
/// faded once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer2d {
    /// Group opacity applied once to the finished layer, in `0.0..=1.0`.
    pub opacity: f32,
    /// Blend mode used to composite the finished layer into its parent.
    pub blend: crate::render::pipeline::BlendMode2d,
    /// Depth of the whole layer in the parent's painter order.
    pub depth: f32,
}

impl Layer2d {
    /// Builds a layer description.
    ///
    /// `opacity` is clamped to `0.0..=1.0` at push time; non-finite values are
    /// rejected with [`LayerError::InvalidOpacity`].
    pub const fn new(
        opacity: f32,
        blend: crate::render::pipeline::BlendMode2d,
        depth: f32,
    ) -> Self {
        Self {
            opacity,
            blend,
            depth,
        }
    }
}

impl Default for Layer2d {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            blend: crate::render::pipeline::BlendMode2d::Normal,
            depth: 0.0,
        }
    }
}

/// Typed errors for the Pure2D offscreen-layer API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerError {
    /// A push exceeded [`MAX_LAYER_DEPTH_2D`] open layers.
    TooDeep {
        /// Maximum supported nesting depth.
        max: u32,
        /// Depth requested by the rejected push.
        requested: u32,
    },
    /// A frame ended (or a resize/render finished) with layers still open.
    ///
    /// The renderer closes them automatically and logs a warning; this error
    /// reports how many were closed.
    Unbalanced {
        /// Number of layers that were still open when the frame ended.
        open: u32,
    },
    /// [`crate::render::types::Layer2d::opacity`] was NaN or infinite.
    InvalidOpacity {
        /// The rejected value.
        opacity: f32,
    },
    /// A pop was requested with no layer open.
    Underflow,
}

impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooDeep { max, requested } => write!(
                f,
                "layer nesting depth {requested} exceeds the maximum of {max}"
            ),
            Self::Unbalanced { open } => write!(
                f,
                "{open} offscreen layer(s) were still open at the end of the frame; they were closed automatically"
            ),
            Self::InvalidOpacity { opacity } => {
                write!(f, "layer opacity must be finite, got {opacity}")
            }
            Self::Underflow => write!(f, "pop_layer_2d called with no open layer"),
        }
    }
}

impl std::error::Error for LayerError {}

/// One instance of the full-target composite quad used to draw a finished
/// offscreen layer into its parent (see `composite_2d.wgsl`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct CompositeInstance {
    /// Group opacity multiplied into the premultiplied layer colour.
    pub opacity: f32,
}

impl CompositeInstance {
    /// Vertex-buffer layout describing this instance type to wgpu.
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CompositeInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32,
            }],
        }
    }
}
