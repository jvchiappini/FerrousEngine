use crate::geometry::{compute_tangents, Mesh, Vertex};
use crate::resources::buffer;
use std::f32::consts::PI;

/// Capsule primitive centred at the origin.
///
/// A capsule consists of a cylindrical body (length `height`) capped at
/// both ends with hemispheres of the given `radius`.  The total height of
/// the object (including caps) is `height + 2 * radius`.
///
/// `cap_segments` controls the spherical quality of each hemisphere;
/// `radial_segments` controls the number of sides around the axis.
pub fn capsule(
    device: &wgpu::Device,
    radius: f32,
    height: f32,
    radial_segments: u32,
    cap_segments: u32,
) -> Mesh {
    let radial_segments = radial_segments.max(3);
    let cap_segments = cap_segments.max(2);
    let half_body = height * 0.5;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices_u32: Vec<u32> = Vec::new();

    // ── Top hemisphere ─────────────────────────────────────────────────────
    // phi goes from 0 (top pole) to PI/2 (equator)
    let top_start = 0u32;
    for lat in 0..=cap_segments {
        let phi = PI * 0.5 * (lat as f32 / cap_segments as f32);
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        // v texture is 0 at top pole, 0.25 at equator of top cap
        let tv = lat as f32 / cap_segments as f32 * 0.25;

        for lon in 0..=radial_segments {
            let theta = 2.0 * PI * (lon as f32 / radial_segments as f32);
            let cos_theta = theta.cos();
            let sin_theta = theta.sin();

            let nx = sin_phi * cos_theta;
            let ny = cos_phi;
            let nz = sin_phi * sin_theta;

            let mut v = Vertex::new(
                [radius * nx, half_body + radius * ny, radius * nz],
                [nx, ny, nz],
                [lon as f32 / radial_segments as f32, tv],
            );
            v.color = [1.0, 1.0, 1.0, 1.0];
            vertices.push(v);
        }
    }

    let stride = radial_segments + 1;
    for lat in 0..cap_segments {
        for lon in 0..radial_segments {
            let a = top_start + lat * stride + lon;
            let b = top_start + lat * stride + lon + 1;
            let c = top_start + (lat + 1) * stride + lon;
            let d = top_start + (lat + 1) * stride + lon + 1;
            indices_u32.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }

    // ── Body ───────────────────────────────────────────────────────────────
    let body_rings = 1u32;
    let body_start = vertices.len() as u32;
    for r in 0..=body_rings {
        let t = r as f32 / body_rings as f32;
        let y = half_body - height * t;
        let tv_body = 0.25 + t * 0.5;  // occupies [0.25, 0.75] of UV

        for lon in 0..=radial_segments {
            let theta = 2.0 * PI * (lon as f32 / radial_segments as f32);
            let cos_theta = theta.cos();
            let sin_theta = theta.sin();

            let mut v = Vertex::new(
                [radius * cos_theta, y, radius * sin_theta],
                [cos_theta, 0.0, sin_theta],
                [lon as f32 / radial_segments as f32, tv_body],
            );
            v.color = [1.0, 1.0, 1.0, 1.0];
            vertices.push(v);
        }
    }

    for r in 0..body_rings {
        for lon in 0..radial_segments {
            let a = body_start + r * stride + lon;
            let b = body_start + r * stride + lon + 1;
            let c = body_start + (r + 1) * stride + lon;
            let d = body_start + (r + 1) * stride + lon + 1;
            indices_u32.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }

    // ── Bottom hemisphere ──────────────────────────────────────────────────
    // phi goes from PI/2 (equator) to PI (bottom pole)
    let bot_start = vertices.len() as u32;
    for lat in 0..=cap_segments {
        let phi = PI * 0.5 + PI * 0.5 * (lat as f32 / cap_segments as f32);
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        let tv = 0.75 + lat as f32 / cap_segments as f32 * 0.25;

        for lon in 0..=radial_segments {
            let theta = 2.0 * PI * (lon as f32 / radial_segments as f32);
            let cos_theta = theta.cos();
            let sin_theta = theta.sin();

            let nx = sin_phi * cos_theta;
            let ny = cos_phi;
            let nz = sin_phi * sin_theta;

            let mut v = Vertex::new(
                [radius * nx, -half_body + radius * ny, radius * nz],
                [nx, ny, nz],
                [lon as f32 / radial_segments as f32, tv],
            );
            v.color = [1.0, 1.0, 1.0, 1.0];
            vertices.push(v);
        }
    }

    for lat in 0..cap_segments {
        for lon in 0..radial_segments {
            let a = bot_start + lat * stride + lon;
            let b = bot_start + lat * stride + lon + 1;
            let c = bot_start + (lat + 1) * stride + lon;
            let d = bot_start + (lat + 1) * stride + lon + 1;
            indices_u32.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }

    compute_tangents(&mut vertices, &indices_u32);
    let indices_u16: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    let total_h = half_body + radius;
    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Capsule VB", &vertices),
        index_buffer: buffer::create_index(device, "Capsule IB", &indices_u16),
        index_count: indices_u16.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
        aabb: crate::scene::culling::Aabb::new(
            glam::Vec3::new(-radius, -total_h, -radius),
            glam::Vec3::new(radius, total_h, radius),
        ),
    }
}
