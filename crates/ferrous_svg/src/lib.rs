//! SVG Tessellation system for Ferrous Engine.
//! Converts SVG paths to GPU-friendly triangle meshes using Lyon.

use lyon_tessellation::{
    FillOptions, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};

// Minimal Rect to avoid dependency cycle with ferrous_ui_core
#[derive(Debug, Clone, Copy, Default)]
pub struct SvgRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Represents a GPU-ready triangle mesh generated from an SVG.
#[derive(Debug, Clone, Default)]
pub struct SvgMesh {
    pub vertices: Vec<SvgVertex>,
    pub indices: Vec<u32>,
    pub bounding_box: SvgRect,
}

#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct SvgVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub z_order: f32,
}

impl SvgMesh {
    /// Parses an SVG string and tessellates it into a triangle mesh.
    pub fn from_str(svg_content: &str) -> Result<Self, String> {
        let mut buffers: VertexBuffers<SvgVertex, u32> = VertexBuffers::new();
        let mut fill_tessellator = FillTessellator::new();
        
        // Use a simple square if we don't have a real parser yet
        // In Phase 6 we will integrate usvg for full spec support.
        let mut builder = lyon_tessellation::path::Path::builder();
        
        // Simple heuristic: if it contains "rect", let's draw a rect for now.
        // This is a placeholder for real SVG parsing.
        if svg_content.contains("<rect") {
            builder.begin(lyon_tessellation::math::point(10.0, 10.0));
            builder.line_to(lyon_tessellation::math::point(90.0, 10.0));
            builder.line_to(lyon_tessellation::math::point(90.0, 90.0));
            builder.line_to(lyon_tessellation::math::point(10.0, 90.0));
            builder.close();
        } else if svg_content.contains("<circle") {
             // Draw a simple octagonal circle
             for i in 0..8 {
                 let angle = (i as f32) * std::f32::consts::PI * 0.25;
                 let p = lyon_tessellation::math::point(
                     50.0 + angle.cos() * 40.0,
                     50.0 + angle.sin() * 40.0
                 );
                 if i == 0 { builder.begin(p); }
                 else { builder.line_to(p); }
             }
             builder.close();
        } else {
            // Default placeholder triangle
            builder.begin(lyon_tessellation::math::point(50.0, 10.0));
            builder.line_to(lyon_tessellation::math::point(90.0, 90.0));
            builder.line_to(lyon_tessellation::math::point(10.0, 90.0));
            builder.close();
        }
        
        let path = builder.build();

        fill_tessellator
            .tessellate_path(
                &path,
                &FillOptions::default(),
                &mut lyon_tessellation::BuffersBuilder::new(&mut buffers, VertexCtor),
            )
            .map_err(|e| format!("Tessellation failed: {:?}", e))?;

        Ok(Self {
            vertices: buffers.vertices,
            indices: buffers.indices,
            bounding_box: SvgRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
        })
    }
}

struct VertexCtor;
impl FillVertexConstructor<SvgVertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> SvgVertex {
        SvgVertex {
            position: [vertex.position().x, vertex.position().y],
            color: [1.0, 1.0, 1.0, 1.0],
            z_order: 0.0,
        }
    }
}
