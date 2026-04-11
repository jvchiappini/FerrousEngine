use crate::geometry::{compute_tangents, Mesh, Vertex};
use crate::resources::buffer;
use std::f32::consts::PI;

/// Flat circle (disc) primitive in the XZ plane, centred at the origin.
///
/// `segments` controls the number of triangular wedges.  More segments
/// yield a smoother edge.  The normal points upward (+Y).
pub fn circle(device: &wgpu::Device, radius: f32, segments: u32) -> Mesh {
    let segments = segments.max(3);

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices_u32: Vec<u32> = Vec::new();

    // Centre vertex
    let mut center = Vertex::new([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5]);
    center.color = [1.0, 1.0, 1.0];
    vertices.push(center);

    for s in 0..=segments {
        let theta = 2.0 * PI * (s as f32 / segments as f32);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        let mut v = Vertex::new(
            [radius * cos_t, 0.0, radius * sin_t],
            [0.0, 1.0, 0.0],
            [cos_t * 0.5 + 0.5, sin_t * 0.5 + 0.5],
        );
        v.color = [1.0, 1.0, 1.0];
        vertices.push(v);
    }

    // Fan triangles — CCW viewed from above
    for s in 0..segments {
        indices_u32.extend_from_slice(&[0, s + 2, s + 1]);
    }

    compute_tangents(&mut vertices, &indices_u32);
    let indices_u16: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Circle VB", &vertices),
        index_buffer: buffer::create_index(device, "Circle IB", &indices_u16),
        index_count: indices_u16.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
        aabb: crate::scene::culling::Aabb::new(
            glam::Vec3::new(-radius, -0.001, -radius),
            glam::Vec3::new(radius, 0.001, radius),
        ),
    }
}

/// Ring (annulus) primitive in the XZ plane, centred at the origin.
///
/// The ring spans from `inner_radius` to `outer_radius`.  `segments`
/// controls the number of angular divisions; `rings` the number of radial
/// subdivisions between the inner and outer edges.
pub fn ring(
    device: &wgpu::Device,
    inner_radius: f32,
    outer_radius: f32,
    segments: u32,
    rings: u32,
) -> Mesh {
    let segments = segments.max(3);
    let rings = rings.max(1);
    let inner = inner_radius.min(outer_radius - 0.001).max(0.0);
    let outer = outer_radius.max(inner + 0.001);

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices_u32: Vec<u32> = Vec::new();

    for r in 0..=rings {
        let t = r as f32 / rings as f32;
        let radius = inner + (outer - inner) * t;
        let tv = t; // radial UV

        for s in 0..=segments {
            let theta = 2.0 * PI * (s as f32 / segments as f32);
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            let u = s as f32 / segments as f32;
            let mut v = Vertex::new(
                [radius * cos_t, 0.0, radius * sin_t],
                [0.0, 1.0, 0.0],
                [u, tv],
            );
            v.color = [1.0, 1.0, 1.0];
            vertices.push(v);
        }
    }

    let stride = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let a = r * stride + s;
            let b = r * stride + s + 1;
            let c = (r + 1) * stride + s;
            let d = (r + 1) * stride + s + 1;
            // CCW viewed from above
            indices_u32.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    compute_tangents(&mut vertices, &indices_u32);
    let indices_u16: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Ring VB", &vertices),
        index_buffer: buffer::create_index(device, "Ring IB", &indices_u16),
        index_count: indices_u16.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
        aabb: crate::scene::culling::Aabb::new(
            glam::Vec3::new(-outer, -0.001, -outer),
            glam::Vec3::new(outer, 0.001, outer),
        ),
    }
}
