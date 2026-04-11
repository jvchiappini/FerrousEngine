use crate::geometry::{compute_tangents, Mesh, Vertex};
use crate::resources::buffer;
use std::f32::consts::PI;

/// Cylinder (or cone/frustum) primitive centred at the origin.
///
/// The cylinder spans from `y = -height/2` to `y = height/2`.  Setting
/// `radius_top = 0.0` produces a cone; equal radii produce a standard
/// cylinder.  `segments` controls the number of sides around the axis.
/// `rings` controls the number of horizontal subdivisions on the body.
/// End caps are optional and are always flat.
pub fn cylinder(
    device: &wgpu::Device,
    radius_top: f32,
    radius_bottom: f32,
    height: f32,
    segments: u32,
    rings: u32,
    open_ended: bool,
) -> Mesh {
    let segments = segments.max(3);
    let rings = rings.max(1);
    let half_h = height * 0.5;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices_u32: Vec<u32> = Vec::new();

    // ── Body ──────────────────────────────────────────────────────────────────
    // For each ring row (0 = bottom, rings = top) and each longitude segment,
    // we generate one vertex.  The seam is duplicated so UVs tile properly.
    let body_start = vertices.len() as u32;
    for r in 0..=rings {
        let t = r as f32 / rings as f32;                // 0 = bottom, 1 = top
        let radius = radius_bottom + (radius_top - radius_bottom) * t;
        let y = -half_h + height * t;

        // Outward normal slope (for a frustum the normal is tilted)
        let slope = (radius_bottom - radius_top) / height;
        let n_len = (1.0 + slope * slope).sqrt();

        for s in 0..=segments {
            let u = s as f32 / segments as f32;
            let theta = u * 2.0 * PI;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            let nx = cos_t / n_len;
            let ny = slope / n_len;
            let nz = sin_t / n_len;

            let mut v = Vertex::new(
                [radius * cos_t, y, radius * sin_t],
                [nx, ny, nz],
                [u, 1.0 - t],
            );
            v.color = [1.0, 1.0, 1.0];
            vertices.push(v);
        }
    }

    let stride = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let a = body_start + r * stride + s;
            let b = body_start + r * stride + s + 1;
            let c = body_start + (r + 1) * stride + s;
            let d = body_start + (r + 1) * stride + s + 1;
            // Two CCW triangles
            indices_u32.extend_from_slice(&[a, b, d, a, d, c]);
        }
    }

    // ── Caps ──────────────────────────────────────────────────────────────────
    if !open_ended {
        for &(cap_y, cap_radius, normal_y) in &[
            (-half_h, radius_bottom, -1.0_f32),
            (half_h,  radius_top,    1.0_f32),
        ] {
            if cap_radius < 1e-6 { continue; }

            let center_idx = vertices.len() as u32;
            let mut center = Vertex::new(
                [0.0, cap_y, 0.0],
                [0.0, normal_y, 0.0],
                [0.5, 0.5],
            );
            center.color = [1.0, 1.0, 1.0];
            vertices.push(center);

            let rim_start = vertices.len() as u32;
            for s in 0..=segments {
                let u = s as f32 / segments as f32;
                let theta = u * 2.0 * PI;
                let cos_t = theta.cos();
                let sin_t = theta.sin();
                let mut v = Vertex::new(
                    [cap_radius * cos_t, cap_y, cap_radius * sin_t],
                    [0.0, normal_y, 0.0],
                    [cos_t * 0.5 + 0.5, sin_t * 0.5 + 0.5],
                );
                v.color = [1.0, 1.0, 1.0];
                vertices.push(v);
            }

            for s in 0..segments {
                if normal_y > 0.0 {
                    // top cap — CCW when viewed from above
                    indices_u32.extend_from_slice(&[
                        center_idx,
                        rim_start + s + 1,
                        rim_start + s,
                    ]);
                } else {
                    // bottom cap — CCW when viewed from below
                    indices_u32.extend_from_slice(&[
                        center_idx,
                        rim_start + s,
                        rim_start + s + 1,
                    ]);
                }
            }
        }
    }

    // ── GPU upload ────────────────────────────────────────────────────────────
    compute_tangents(&mut vertices, &indices_u32);
    let indices_u16: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    let r_max = radius_bottom.max(radius_top);
    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Cylinder VB", &vertices),
        index_buffer: buffer::create_index(device, "Cylinder IB", &indices_u16),
        index_count: indices_u16.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
        aabb: crate::scene::culling::Aabb::new(
            glam::Vec3::new(-r_max, -half_h, -r_max),
            glam::Vec3::new(r_max, half_h, r_max),
        ),
    }
}
