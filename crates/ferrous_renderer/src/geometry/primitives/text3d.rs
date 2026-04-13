use crate::geometry::mesh::Mesh;
use crate::geometry::vertex::Vertex;
use glam::{Vec2, Vec3, Vec4};

pub struct Text3dBuilder {
    pub text: String,
    pub font_data: Vec<u8>,
    pub depth: f32,
    pub quality: u8,
    pub bevel_enabled: bool,
    pub bevel_thickness: f32,
    pub bevel_size: f32,
}

impl Default for Text3dBuilder {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_data: Vec::new(),
            depth: 1.0,
            quality: 24,
            bevel_enabled: false,
            bevel_thickness: 0.1,
            bevel_size: 0.1,
        }
    }
}

impl Text3dBuilder {
    pub fn new(text: &str, font_data: &[u8]) -> Self {
        Self {
            text: text.to_string(),
            font_data: font_data.to_vec(),
            ..Default::default()
        }
    }

    pub fn depth(mut self, depth: f32) -> Self {
        self.depth = depth;
        self
    }

    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }

    pub fn bevel(mut self, enabled: bool, thickness: f32, size: f32) -> Self {
        self.bevel_enabled = enabled;
        self.bevel_thickness = thickness;
        self.bevel_size = size;
        self
    }

    pub fn build(self, device: &wgpu::Device) -> anyhow::Result<Mesh> {
        if self.font_data.is_empty() {
            anyhow::bail!("Font data cannot be empty");
        }

        let face = fontmesh::Face::parse(&self.font_data, 0)
            .map_err(|e| anyhow::anyhow!("Failed to parse font: {:?}", e))?;

        let mut all_vertices = Vec::new();
        let mut all_indices: Vec<u32> = Vec::new();
        
        let mut cursor_x = 0.0;
        let mut min_pos = Vec3::splat(f32::MAX);
        let mut max_pos = Vec3::splat(f32::MIN);

        for c in self.text.chars() {
            if c == ' ' {
                if let Some(advance) = fontmesh::glyph_advance(&face, '-') {
                    cursor_x += advance;
                } else {
                    cursor_x += 0.5; // fallback space width
                }
                continue;
            }

            // Generate 3D mesh for the character
            let char_mesh = fontmesh::char_to_mesh_3d(&face, c, self.depth, self.quality);
            
            let mesh_3d = match char_mesh {
                Ok(m) => m,
                Err(_) => {
                    // Try next char or skip if missing glyph
                    continue;
                }
            };

            let base_idx = all_vertices.len() as u32;

            // Prepare for beveling: group vertices by position to ensure connectivity
            struct VertInfo {
                pos: Vec3,
                norm: Vec3,
                orig_idx: usize,
            }
            let mut v_infos: Vec<VertInfo> = mesh_3d.vertices.iter().enumerate().map(|(i, v)| {
                VertInfo {
                    pos: Vec3::new(v.x, v.y, v.z),
                    norm: Vec3::new(mesh_3d.normals[i].x, mesh_3d.normals[i].y, mesh_3d.normals[i].z),
                    orig_idx: i,
                }
            }).collect();

            if self.bevel_enabled {
                // 1. Group by position
                use std::collections::HashMap;
                let mut pos_map: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
                for (i, vi) in v_infos.iter().enumerate() {
                    let key = [
                        (vi.pos.x * 100.0) as i32,
                        (vi.pos.y * 100.0) as i32,
                        (vi.pos.z * 100.0) as i32,
                    ];
                    pos_map.entry(key).or_default().push(i);
                }

                // 2. Bevel each position consistently
                for indices in pos_map.values() {
                    let mut avg_side_norm = Vec3::ZERO;
                    let mut side_count = 0;
                    let mut is_front = false;
                    let mut is_back = false;

                    for &idx in indices {
                        let n = v_infos[idx].norm;
                        if n.z.abs() < 0.2 {
                            avg_side_norm += n;
                            side_count += 1;
                        } 
                        if v_infos[idx].pos.z > self.depth * 0.4 {
                            is_front = true;
                        } else if v_infos[idx].pos.z < -self.depth * 0.4 {
                            is_back = true;
                        }
                    }

                    if side_count > 0 {
                        avg_side_norm = avg_side_norm.normalize_or_zero();
                        let offset_xy = avg_side_norm * self.bevel_size;
                        
                        for &idx in indices {
                            // Move along contours
                            v_infos[idx].pos.x -= offset_xy.x;
                            v_infos[idx].pos.y -= offset_xy.y;
                            
                            // Move inward in Z
                            if is_front {
                                v_infos[idx].pos.z -= self.bevel_thickness;
                            } else if is_back {
                                v_infos[idx].pos.z += self.bevel_thickness;
                            }
                        }
                    }
                }
            }

            // Final vertex push
            for vi in v_infos {
                let mut pos = vi.pos;
                pos.x += cursor_x;

                // Update AABB
                min_pos = min_pos.min(pos);
                max_pos = max_pos.max(pos);

                let uv = Vec2::new(pos.x, pos.y);
                let mut vert = Vertex::new(pos.to_array(), vi.norm.to_array(), uv.to_array());
                vert.tangent = [1.0, 0.0, 0.0, 1.0];
                vert.color = [1.0, 1.0, 1.0];
                all_vertices.push(vert);
            }

            for chunk in mesh_3d.indices.chunks_exact(3) {
                all_indices.push(base_idx + chunk[0]);
                all_indices.push(base_idx + chunk[2]);
                all_indices.push(base_idx + chunk[1]);
            }

            // Advance cursor for next char
            if let Some(advance) = fontmesh::glyph_advance(&face, c) {
                cursor_x += advance;
            }
        }

        if all_vertices.is_empty() {
            return Ok(Mesh::empty(device));
        }

        // --- Final Mesh Construction with Flat Shading ---
        // To achieve crisp, non-melted edges (especially on bevels), we duplicate 
        // vertices per triangle to ensure flat normals.
        let mut flat_vertices = Vec::new();
        let mut flat_indices = Vec::new();
        
        for chunk in all_indices.chunks_exact(3) {
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            let v0 = Vec3::from_array(all_vertices[i0].position);
            let v1 = Vec3::from_array(all_vertices[i1].position);
            let v2 = Vec3::from_array(all_vertices[i2].position);

            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let face_normal = edge1.cross(edge2).normalize_or_zero();

            let mut v0_obj = all_vertices[i0].clone();
            let mut v1_obj = all_vertices[i1].clone();
            let mut v2_obj = all_vertices[i2].clone();

            v0_obj.normal = face_normal.to_array();
            v1_obj.normal = face_normal.to_array();
            v2_obj.normal = face_normal.to_array();

            let base = flat_vertices.len() as u32;
            flat_vertices.push(v0_obj);
            flat_vertices.push(v1_obj);
            flat_vertices.push(v2_obj);
            
            flat_indices.push(base);
            flat_indices.push(base + 1);
            flat_indices.push(base + 2);
        }

        crate::geometry::compute_tangents(&mut flat_vertices, &flat_indices);

        let vb = crate::resources::buffer::create_vertex(device, "Text3D VB", &flat_vertices);
        let ib = crate::resources::buffer::create_index(device, "Text3D IB", &flat_indices);

        let aabb = crate::scene::culling::Aabb::new(min_pos, max_pos);

        Ok(Mesh {
            vertex_buffer: vb,
            index_buffer: ib,
            index_count: flat_indices.len() as u32,
            vertex_count: flat_vertices.len() as u32,
            index_format: wgpu::IndexFormat::Uint32,
            aabb,
        })
    }
}
