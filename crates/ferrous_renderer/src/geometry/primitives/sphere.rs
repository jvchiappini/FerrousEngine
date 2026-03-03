use crate::geometry::{compute_tangents, Mesh, Vertex};
use crate::resources::buffer;

/// UV sphere primitive centred at the origin.
///
/// The mesh is parameterised by a radius together with the number of
/// latitudinal and longitudinal divisions.  `latitudes` controls the
/// number of horizontal rings between the poles (including the poles),
/// while `longitudes` is the number of segments around the equator.  A
/// reasonably sized value such as 16×32 yields a smooth-looking sphere with
/// only a few hundred triangles; the caller can increase these values for
/// higher fidelity.  Both parameters are clamped to sensible minima so the
/// function always returns a valid mesh.
pub fn sphere(
    device: &wgpu::Device,
    radius: f32,
    latitudes: u32,
    longitudes: u32,
) -> Mesh {
    // ensure we have at least a top and bottom ring and one longitude
    let latitudes = latitudes.max(2);
    let longitudes = longitudes.max(3);

    // build the vertex list: we create (latitudes+1)×(longitudes+1) vertices
    // so that the last longitude wraps back to the first without special
    // casing when generating indices.
    let mut vertices: Vec<Vertex> = Vec::new();
    for lat in 0..=latitudes {
        let phi = std::f32::consts::PI * (lat as f32) / (latitudes as f32);
        let y = radius * phi.cos();
        let sin_phi = radius * phi.sin();

        for lon in 0..=longitudes {
            let theta = 2.0 * std::f32::consts::PI * (lon as f32) / (longitudes as f32);
            let x = sin_phi * theta.cos();
            let z = sin_phi * theta.sin();

            let position = [x, y, z];
            // normalised position for a sphere centred at the origin
            let mut normal = position;
            let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if len > 1e-6 {
                normal = [normal[0] / len, normal[1] / len, normal[2] / len];
            }

            let u = (lon as f32) / (longitudes as f32);
            let v = (lat as f32) / (latitudes as f32);

            vertices.push(Vertex::new(position, normal, [u, v]));
        }
    }

    // indices forming two triangles per quad in the latitude/longitude grid.
    // we use u32 temporarily for tangent computation then convert to u16 for
    // the final mesh; most reasonable subdivision counts will easily fit in
    // 16 bits.
    let mut indices_u32: Vec<u32> = Vec::new();
    let stride = longitudes + 1;
    for lat in 0..latitudes {
        for lon in 0..longitudes {
            let current = lat * stride + lon;
            let next = current + stride;
            // first triangle (current, current+1, next)
            indices_u32.push(current);
            indices_u32.push(current + 1);
            indices_u32.push(next);
            // second triangle (current+1, next+1, next)
            indices_u32.push(current + 1);
            indices_u32.push(next + 1);
            indices_u32.push(next);
        }
    }

    // compute tangents for correct normal mapping
    let mut vertices_mut = vertices; // make mutable
    compute_tangents(&mut vertices_mut, &indices_u32);

    // convert indices for GPU upload
    let indices: Vec<u16> = indices_u32.iter().map(|&i| i as u16).collect();

    Mesh {
        vertex_buffer: buffer::create_vertex(device, "Sphere VB", &vertices_mut),
        index_buffer: buffer::create_index(device, "Sphere IB", &indices),
        index_count: indices.len() as u32,
        vertex_count: vertices_mut.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
    }
}
