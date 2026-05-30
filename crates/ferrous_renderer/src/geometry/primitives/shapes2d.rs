//! Vectorial 2D shapes tessellated into 3D meshes using `lyon_tessellation`.
//!
//! Philosophy: "2D in Scalar is flat geometry in 3D world space."
//! Every vertex is placed in the XY plane with `z = 0.0` and `normal = [0, 0, 1]`.
//! This means the shapes:
//!   - Share the Z-buffer with 3D objects
//!   - Rotate with the camera
//!   - Can project shadows (via the `ShadowCaster` marker component)
//!   - Work with `set_position`, `set_color`, etc. without any special casing
//!
//! The rendered color comes from `MaterialComponent::descriptor::base_color`.
//! For pure "unlit" / solid-color rendering, assign a `Material` with
//! `style_override: Some(RenderStyle::FlatShaded)` — this disables PBR
//! lighting while still allowing the entity to participate in the shadow pass.

use lyon_tessellation::{
    math::{point, Box2D},
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers,
};

use crate::geometry::{Mesh, Vertex};
use crate::resources::buffer;

// ── Internal vertices ──────────────────────────────────────────────────────

/// Adapter: maps a lyon fill vertex to our `Vertex` type.
struct FillCtor { color: [f32; 4] }

impl lyon_tessellation::FillVertexConstructor<Vertex> for FillCtor {
    fn new_vertex(&mut self, v: FillVertex) -> Vertex {
        let p = v.position();
        Vertex {
            position: [p.x, p.y, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: self.color,
            uv: [(p.x + 1.0) * 0.5, (p.y + 1.0) * 0.5],
        }
    }
}

/// Adapter: maps a lyon stroke vertex to our `Vertex` type.
struct StrokeCtor { color: [f32; 4] }

impl lyon_tessellation::StrokeVertexConstructor<Vertex> for StrokeCtor {
    fn new_vertex(&mut self, v: StrokeVertex) -> Vertex {
        let p = v.position();
        Vertex {
            position: [p.x, p.y, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: self.color,
            uv: [p.x, p.y],
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a `VertexBuffers<Vertex, u32>` to a GPU `Mesh`.
fn buffers_to_mesh(
    device: &wgpu::Device,
    label: &str,
    buffers: VertexBuffers<Vertex, u32>,
) -> Mesh {
    let indices = buffers.indices;
    let vertices = buffers.vertices;

    if indices.is_empty() || vertices.is_empty() {
        return Mesh::empty(device);
    }

    // Calculate AABB from vertices
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in &vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }

    Mesh {
        vertex_buffer: buffer::create_vertex(device, &format!("{label} VB"), &vertices),
        index_buffer: buffer::create_index(device, &format!("{label} IB"), &indices),
        index_count: indices.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint32,
        aabb: crate::scene::culling::Aabb::new(glam::Vec3::from(min), glam::Vec3::from(max)),
    }
}

// ── Path Conversion ─────────────────────────────────────────────────────────

use ferrous_core::scene::world::types::{PathCommand, PathData};
use lyon_tessellation::path::Path;

fn build_lyon_path(path_data: &PathData) -> Path {
    use lyon_tessellation::path::Builder;
    let mut builder = Builder::new();
    let mut contour_open = false;
    for cmd in &path_data.commands {
        match cmd {
            PathCommand::MoveTo(p) => {
                // If a previous contour was open (no Close command), end it explicitly.
                if contour_open {
                    builder.end(false);
                }
                builder.begin(point(p.x, p.y));
                contour_open = true;
            }
            PathCommand::LineTo(p) => {
                builder.line_to(point(p.x, p.y));
            }
            PathCommand::CubicTo(c1, c2, p) => {
                builder.cubic_bezier_to(point(c1.x, c1.y), point(c2.x, c2.y), point(p.x, p.y));
            }
            PathCommand::Close => {
                builder.end(true);
                contour_open = false;
            }
        }
    }
    // End any remaining open contour as an open path (not closed).
    if contour_open {
        builder.end(false);
    }
    builder.build()
}

/// Dynamic tessellation of any [`PathData`].
/// Supports simultaneous fill and stroke.
pub fn path_to_mesh(
    device: &wgpu::Device,
    path_data: &PathData,
    do_fill: bool,
    stroke_thickness: f32,
    fill_color: [f32; 4],
    stroke_color: [f32; 4],
) -> Mesh {
    let path = build_lyon_path(path_data);
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();

    if do_fill {
        let mut tessellator = FillTessellator::new();
        let options = FillOptions::tolerance(0.001);
        tessellator
            .tessellate_path(
                &path,
                &options,
                &mut BuffersBuilder::new(&mut buffers, FillCtor { color: fill_color }),
            )
            .ok();
    }

    // Clamp to a minimum visible thickness (mirrored from line_2d).
    // Prevents 0.05-unit strokes from rounding to <1px and being discarded
    // when MSAA is absent.
    let effective_thickness = if stroke_thickness > 0.0 {
        stroke_thickness.max(1e-3)
    } else {
        0.0
    };

    if effective_thickness > 0.0 {
        let mut tessellator = StrokeTessellator::new();
        let options = StrokeOptions::default()
            .with_line_width(effective_thickness)
            .with_line_cap(lyon_tessellation::LineCap::Butt)
            .with_line_join(lyon_tessellation::LineJoin::Miter)
            .with_tolerance(0.01);
        tessellator
            .tessellate_path(
                &path,
                &options,
                &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: stroke_color }),
            )
            .ok();
    }

    buffers_to_mesh(device, "Path2D", buffers)
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Filled circle disc in the XY plane (z = 0), centred at the origin.
pub fn circle_2d(device: &wgpu::Device, radius: f32, resolution: u32, do_fill: bool, stroke_thickness: f32, fill_c: [f32;4], stroke_c: [f32;4]) -> Mesh {
    use lyon_tessellation::path::Winding;
    let mut builder = lyon_tessellation::path::Builder::new();
    builder.add_circle(point(0.0, 0.0), radius, Winding::Positive);
    let path = builder.build();

    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    
    if do_fill {
        let mut tessellator = FillTessellator::new();
        tessellator.tessellate_path(&path, &FillOptions::tolerance(0.001), &mut BuffersBuilder::new(&mut buffers, FillCtor { color: fill_c })).ok();
    }
    
    if stroke_thickness > 0.0 {
        let mut tessellator = StrokeTessellator::new();
        let opts = StrokeOptions::default()
            .with_line_width(stroke_thickness)
            .with_line_cap(lyon_tessellation::LineCap::Round)
            .with_line_join(lyon_tessellation::LineJoin::Round);
        tessellator.tessellate_path(&path, &opts, &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: stroke_c })).ok();
    }

    buffers_to_mesh(device, "Circle2D", buffers)
}

/// Filled axis-aligned rectangle in the XY plane (z = 0), centred at the origin.
pub fn rect_2d(device: &wgpu::Device, width: f32, height: f32, do_fill: bool, stroke_thickness: f32, fill_c: [f32;4], stroke_c: [f32;4]) -> Mesh {
    let hw = width * 0.5;
    let hh = height * 0.5;
    
    use lyon_tessellation::path::Builder;
    let mut builder = Builder::new();
    builder.add_rectangle(&Box2D::new(point(-hw, -hh), point(hw, hh)), lyon_tessellation::path::Winding::Positive);
    let path = builder.build();

    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();

    if do_fill {
        let mut tessellator = FillTessellator::new();
        tessellator.tessellate_path(&path, &FillOptions::tolerance(0.001), &mut BuffersBuilder::new(&mut buffers, FillCtor { color: fill_c })).ok();
    }

    if stroke_thickness > 0.0 {
        let mut tessellator = StrokeTessellator::new();
        let opts = StrokeOptions::default()
            .with_line_width(stroke_thickness)
            .with_line_cap(lyon_tessellation::LineCap::Round)
            .with_line_join(lyon_tessellation::LineJoin::Round);
        tessellator.tessellate_path(&path, &opts, &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: stroke_c })).ok();
    }

    buffers_to_mesh(device, "Rect2D", buffers)
}

/// Thick line segment in the XY plane (z = 0).
pub fn line_2d(device: &wgpu::Device, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: [f32;4]) -> Mesh {
    let thickness = thickness.max(1e-4);

    let mut builder = lyon_tessellation::path::Path::builder();
    builder.begin(point(x0, y0));
    builder.line_to(point(x1, y1));
    builder.end(false);
    let path = builder.build();

    let opts = StrokeOptions::default()
        .with_line_width(thickness)
        .with_line_cap(lyon_tessellation::LineCap::Butt)
        .with_line_join(lyon_tessellation::LineJoin::Miter)
        .with_tolerance(0.01);

    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    tessellator
        .tessellate_path(
            &path,
            &opts,
            &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color }),
        )
        .expect("line_2d: stroke tessellation failed");

    buffers_to_mesh(device, "Line2D", buffers)
}
