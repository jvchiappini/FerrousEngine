/// GPU vertex type used across all built-in render pipelines.
///
/// The struct is laid out exactly as expected by the WGSL shaders; a
/// 60‑byte stride is required by the new PBR topology.  `bytemuck` traits
/// are derived so the vertex arrays can be safely cast to bytes for upload.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Normal vector pointing away from the surface.
    pub normal: [f32; 3],
    /// Tangent vector plus handedness in `w` component.  The bitangent can be
    /// reconstructed in the shader via `cross(normal, tangent.xyz) * tangent.w`.
    pub tangent: [f32; 4],
    /// Linear RGB vertex color.
    pub color: [f32; 3],
    /// UV coordinates for texture lookup.
    pub uv: [f32; 2],
}

impl Vertex {
    /// Returns the `VertexBufferLayout` that matches this struct's memory
    /// layout.  Pass this to `wgpu::VertexState::buffers` when building a
    /// render pipeline.
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        // stride is known to be 60 bytes from the ordered fields above
        wgpu::VertexBufferLayout {
            array_stride: 60,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // @location(0) position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                // @location(1) normal
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                // @location(2) tangent (xyz + handedness)
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                // @location(3) color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 40,
                    shader_location: 3,
                },
                // @location(4) uv
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 52,
                    shader_location: 4,
                },
            ],
        }
    }
    /// Convenience constructor for the common case of specifying only
    /// position, normal and UVs.  Tangent and color are set to sensible
    /// defaults (unit X tangent with +1 handedness, white vertex color).
    pub fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Vertex {
            position,
            normal,
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0],
            uv,
        }
    }
}

/// Calculates per-vertex tangents using the Eric Lengyel method.  The
/// supplied indices are treated as triangles; any leftover elements are
/// ignored.  The resulting tangent is stored in the vertex's `tangent` field
/// with `w` = ±1 defining the handedness.  Normals are assumed to be
/// prepopulated and unit-length.
pub fn compute_tangents(vertices: &mut [Vertex], indices: &[u32]) {
    let len = vertices.len();
    if len == 0 {
        return;
    }

    // temporary accumulators for sdir (tangent) and tdir (bitangent)
    let mut tan1 = vec![[0.0_f32; 3]; len];
    let mut tan2 = vec![[0.0_f32; 3]; len];

    // helper ops
    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }
    fn mul(a: [f32; 3], s: f32) -> [f32; 3] {
        [a[0] * s, a[1] * s, a[2] * s]
    }
    fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
    }
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let len_sq = dot(v, v);
        if len_sq > 1e-8 {
            let inv = 1.0 / len_sq.sqrt();
            mul(v, inv)
        } else {
            v
        }
    }

    // iterate triangles
    for idx in indices.chunks(3) {
        if idx.len() < 3 {
            break;
        }
        let i0 = idx[0] as usize;
        let i1 = idx[1] as usize;
        let i2 = idx[2] as usize;
        if i0 >= len || i1 >= len || i2 >= len {
            continue;
        }

        let v0 = vertices[i0].position;
        let v1 = vertices[i1].position;
        let v2 = vertices[i2].position;
        let uv0 = vertices[i0].uv;
        let uv1 = vertices[i1].uv;
        let uv2 = vertices[i2].uv;

        let x1 = v1[0] - v0[0];
        let x2 = v2[0] - v0[0];
        let y1 = v1[1] - v0[1];
        let y2 = v2[1] - v0[1];
        let z1 = v1[2] - v0[2];
        let z2 = v2[2] - v0[2];

        let s1 = uv1[0] - uv0[0];
        let s2 = uv2[0] - uv0[0];
        let t1 = uv1[1] - uv0[1];
        let t2 = uv2[1] - uv0[1];

        let denom = s1 * t2 - s2 * t1;
        let r = if denom.abs() > 1e-8 { 1.0 / denom } else { 0.0 };

        let sdir = [
            (t2 * x1 - t1 * x2) * r,
            (t2 * y1 - t1 * y2) * r,
            (t2 * z1 - t1 * z2) * r,
        ];
        let tdir = [
            (s1 * x2 - s2 * x1) * r,
            (s1 * y2 - s2 * y1) * r,
            (s1 * z2 - s2 * z1) * r,
        ];

        tan1[i0] = add(tan1[i0], sdir);
        tan1[i1] = add(tan1[i1], sdir);
        tan1[i2] = add(tan1[i2], sdir);

        tan2[i0] = add(tan2[i0], tdir);
        tan2[i1] = add(tan2[i1], tdir);
        tan2[i2] = add(tan2[i2], tdir);
    }

    // orthogonalize and store
    for i in 0..len {
        let n = vertices[i].normal;
        let t = tan1[i];

        // Gram-Schmidt orthogonalize
        let mut t_ortho = sub(t, mul(n, dot(n, t)));
        t_ortho = normalize(t_ortho);

        // compute handedness
        let cross_nt = cross(n, t_ortho);
        let handed = if dot(cross_nt, tan2[i]) < 0.0 {
            -1.0
        } else {
            1.0
        };

        vertices[i].tangent = [t_ortho[0], t_ortho[1], t_ortho[2], handed];
    }
}
