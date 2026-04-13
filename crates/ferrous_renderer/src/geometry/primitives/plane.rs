use crate::geometry::{compute_tangents, Mesh, Vertex};
use crate::resources::buffer;

/// Subdivided plane primitive in the XZ plane, centred at the origin.
///
/// Unlike the simple `quad` (which lives in the XY plane and has no
/// subdivisions), this plane lies flat in the scene (Y = 0) and supports
/// an arbitrary number of width and height segments, making it suitable
/// for terrain bases, water surfaces, and shadow receivers.
///
/// `width_segments` × `height_segments` quads → 2 × that many triangles.
pub fn plane(
    device: &wgpu::Device,
    width: f32,
    height: f32,
    width_segments: u32,
    height_segments: u32,
) -> Mesh {
    let width_segments = width_segments.max(1);
    let height_segments = height_segments.max(1);

    let hw = width * 0.5;
    let hh = height * 0.5;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices_u32: Vec<u32> = Vec::new();

    for iz in 0..=height_segments {
        let z = -hh + height * (iz as f32 / height_segments as f32);
        let v = iz as f32 / height_segments as f32;

        for ix in 0..=width_segments {
            let x = -hw + width * (ix as f32 / width_segments as f32);
            let u = ix as f32 / width_segments as f32;

            let mut vert = Vertex::new([x, 0.0, z], [0.0, 1.0, 0.0], [u, v]);
            vert.color = [1.0, 1.0, 1.0];
            vertices.push(vert);
        }
    }

    let stride = width_segments + 1;
    for iz in 0..height_segments {
        for ix in 0..width_segments {
            let a = iz * stride + ix;
            let b = iz * stride + ix + 1;
            let c = (iz + 1) * stride + ix;
            let d = (iz + 1) * stride + ix + 1;
            // CW viewed from above (+Y)
            indices_u32.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }

    compute_tangents(&mut vertices, &indices_u32);
    let indices_u16: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Plane VB", &vertices),
        index_buffer: buffer::create_index(device, "Plane IB", &indices_u16),
        index_count: indices_u16.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
        aabb: crate::scene::culling::Aabb::new(
            glam::Vec3::new(-hw, -0.001, -hh),
            glam::Vec3::new(hw, 0.001, hh),
        ),
    }
}
