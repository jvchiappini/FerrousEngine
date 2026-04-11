use crate::geometry::{compute_tangents, Mesh, Vertex};
use crate::resources::buffer;
use std::f32::consts::PI;

/// Torus (donut) primitive centred at the origin in the XZ plane.
///
/// `radius` is the distance from the centre of the torus to the centre of
/// the tube.  `tube` is the radius of the tube itself.  `radial_segments`
/// controls the number of segments around the main ring; `tubular_segments`
/// controls the subdivision of the tube cross-section.  An `arc` of `2π`
/// closes the ring; smaller values produce partial tori.
pub fn torus(
    device: &wgpu::Device,
    radius: f32,
    tube: f32,
    radial_segments: u32,
    tubular_segments: u32,
    arc: f32,
) -> Mesh {
    let radial_segments = radial_segments.max(3);
    let tubular_segments = tubular_segments.max(3);
    let arc = arc.clamp(0.001, 2.0 * PI);

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices_u32: Vec<u32> = Vec::new();

    for j in 0..=radial_segments {
        let phi = arc * (j as f32 / radial_segments as f32);
        let cos_phi = phi.cos();
        let sin_phi = phi.sin();

        // Centre of the tube cross-section at this radial position
        let cx = radius * cos_phi;
        let cz = radius * sin_phi;

        for i in 0..=tubular_segments {
            let theta = 2.0 * PI * (i as f32 / tubular_segments as f32);
            let cos_th = theta.cos();
            let sin_th = theta.sin();

            // Vertex position on the tube surface
            let x = (radius + tube * cos_th) * cos_phi;
            let y = tube * sin_th;
            let z = (radius + tube * cos_th) * sin_phi;

            // Normal: from tube centre to surface point, normalised
            let nx = (x - cx) / tube;
            let ny = y / tube;
            let nz = (z - cz) / tube;

            let u = j as f32 / radial_segments as f32;
            let v = i as f32 / tubular_segments as f32;

            let mut vert = Vertex::new([x, y, z], [nx, ny, nz], [u, v]);
            vert.color = [1.0, 1.0, 1.0];
            vertices.push(vert);
        }
    }

    // Indices — one quad per (j,i) cell
    let stride = tubular_segments + 1;
    for j in 0..radial_segments {
        for i in 0..tubular_segments {
            let a = j * stride + i;
            let b = (j + 1) * stride + i;
            let c = (j + 1) * stride + i + 1;
            let d = j * stride + i + 1;
            // CCW quads
            indices_u32.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }

    compute_tangents(&mut vertices, &indices_u32);
    let indices_u16: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    let outer = radius + tube;
    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Torus VB", &vertices),
        index_buffer: buffer::create_index(device, "Torus IB", &indices_u16),
        index_count: indices_u16.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
        aabb: crate::scene::culling::Aabb::new(
            glam::Vec3::new(-outer, -tube, -outer),
            glam::Vec3::new(outer, tube, outer),
        ),
    }
}
